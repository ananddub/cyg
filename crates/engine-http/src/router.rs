//! # Endpoint Router
//!
//! Two-tier dispatch, two HashMaps:
//! - **Tier 1 — static routes** (no `:param`/`*`): `HashMap<path, HashMap<Method, Handler>>`
//!   — O(1) lookup, the fast path for API endpoints like `/status` or `/telemetry`.
//! - **Tier 2 — dynamic routes** (`/:engine`, `/rooms/:id`): a second
//!   `HashMap<segment_count, Vec<Route>>`. A request hashes its path segment
//!   count once and only scans routes with the identical length — usually a
//!   handful — instead of every registered pattern.
//! - `*` wildcard patterns (rare) live in a small overflow list checked last.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::types::{Method, Request, Response};

/// Path depth handled without any allocation during dispatch.
const MAX_INLINE_SEGMENTS: usize = 16;

/// `:param` captures handled without any allocation during dispatch.
const MAX_INLINE_PARAMS: usize = 8;

/// A boxed endpoint handler.
pub type Handler = Box<dyn Fn(&Request) -> Response + Send + Sync>;

#[derive(Clone)]
pub(crate) enum Segment {
    Literal(String),
    Param(String),
    Wildcard,
}

pub(crate) struct Route {
    pub(crate) method: Method,
    pub(crate) segments: Vec<Segment>,
    pub(crate) handler: Handler,
}

/// Captures for one route match, held inline for the common handful of
/// `:params` and only spilled to the heap for exotic patterns. Reused across
/// every candidate route in a dispatch, so matching allocates nothing.
pub(crate) enum Captures<'r, 'p> {
    Inline {
        buf: [(&'r str, &'p str); MAX_INLINE_PARAMS],
        len: usize,
    },
    Heap(Vec<(&'r str, &'p str)>),
}

impl<'r, 'p> Captures<'r, 'p> {
    pub(crate) fn inline() -> Self {
        Self::Inline {
            buf: [("", ""); MAX_INLINE_PARAMS],
            len: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        match self {
            Captures::Inline { len, .. } => *len = 0,
            Captures::Heap(v) => v.clear(),
        }
    }

    fn push(&mut self, name: &'r str, value: &'p str) {
        if matches!(self, Captures::Inline { len, .. } if *len < MAX_INLINE_PARAMS) {
            if let Captures::Inline { buf, len } = self {
                buf[*len] = (name, value);
                *len += 1;
            }
            return;
        }
        let mut vec = match self {
            Captures::Heap(v) => std::mem::take(v),
            Captures::Inline { buf, len } => buf[..*len].to_vec(),
        };
        vec.push((name, value));
        *self = Captures::Heap(vec);
    }

    pub(crate) fn as_slice(&self) -> &[(&'r str, &'p str)] {
        match self {
            Captures::Inline { buf, len } => &buf[..*len],
            Captures::Heap(v) => v,
        }
    }
}

impl Route {
    /// Minimum number of path segments this route can match. A trailing
    /// wildcard consumes zero-or-more, so the floor is one less per wildcard.
    fn min_segments(&self) -> usize {
        self.segments.iter().filter(|s| !matches!(s, Segment::Wildcard)).count()
    }

    fn has_wildcard(&self) -> bool {
        self.segments.iter().any(|s| matches!(s, Segment::Wildcard))
    }
}

/// A collection of registered endpoints.
#[derive(Default)]
pub struct Router {
    /// Tier 1: (normalized path) -> handlers by method, O(1) exact dispatch.
    exact: HashMap<String, HashMap<Method, Handler>>,
    /// Tier 2: dynamic `:param` patterns bucketed by their segment count.
    dynamic: HashMap<usize, Vec<Route>>,
    /// `*` wildcard patterns — matched against any path long enough.
    wildcards: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for `(method, pattern)`. Plain paths go into the
    /// exact HashMap; `:name` patterns go into the length-bucketed dynamic
    /// HashMap; `*` patterns go into the wildcard overflow.
    pub fn route(
        mut self,
        method: Method,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        let handler: Handler = Box::new(handler);
        if !pattern.contains(':') && !pattern.contains('*') {
            self.exact
                .entry(normalize_path(pattern).into_owned())
                .or_default()
                .insert(method, handler);
            return self;
        }

        let segments = compile_pattern(pattern);
        let route = Route { method, segments, handler };
        if route.has_wildcard() {
            self.wildcards.push(route);
        } else {
            let count = route.segments.len();
            self.dynamic.entry(count).or_default().push(route);
        }
        self
    }

    pub fn get(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Get, pattern, handler)
    }

    pub fn post(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Post, pattern, handler)
    }

    pub fn put(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Put, pattern, handler)
    }

    pub fn delete(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Delete, pattern, handler)
    }

    /// Number of registered endpoints across both tiers.
    pub fn len(&self) -> usize {
        self.exact.values().map(|m| m.len()).sum::<usize>()
            + self.dynamic.values().map(|v| v.len()).sum::<usize>()
            + self.wildcards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Match and execute the route for the request path/method.
    /// Exact-map hit is O(1); dynamic routes hash to a single length bucket.
    /// 404 when the path matches nothing, 405 when the path matches but no
    /// method does.
    ///
    /// Hot path allocates nothing: the normalized path is borrowed from the
    /// request whenever it is already canonical, and segments are slices of it.
    pub fn dispatch(&self, mut request: Request) -> Response {
        if let Some(handlers) = self.exact.get(&*normalize_path(&request.path)) {
            if let Some(handler) = handlers.get(&request.method) {
                return handler(&request);
            }
            return Response::error(405, "method not allowed");
        }

        // Segments are slices of the request path, held in a stack array for
        // the common depth; only unusually deep paths allocate.
        let trimmed = request.path.trim_matches('/');
        let mut inline: [&str; MAX_INLINE_SEGMENTS] = [""; MAX_INLINE_SEGMENTS];
        let mut count = 0usize;
        let mut spilled = false;
        for seg in trimmed.split('/').filter(|s| !s.is_empty()) {
            if count < MAX_INLINE_SEGMENTS {
                inline[count] = seg;
            } else {
                spilled = true;
            }
            count += 1;
        }
        let deep: Vec<&str>;
        let path_segments: &[&str] = if spilled {
            deep = request.path.split('/').filter(|s| !s.is_empty()).collect();
            &deep[..]
        } else {
            &inline[..count]
        };
        let path_len = path_segments.len();
        let mut path_matched = false;
        let mut captures = Captures::inline();

        // Tier 2: one hash lookup narrows to routes of identical length.
        if let Some(bucket) = self.dynamic.get(&path_len) {
            for route in bucket {
                if !match_segments(&route.segments, path_segments, &mut captures) {
                    continue;
                }
                if route.method != request.method {
                    path_matched = true;
                    continue;
                }
                for (name, value) in captures.as_slice() {
                    request.params.insert(name.to_string(), value.to_string());
                }
                return (route.handler)(&request);
            }
        }

        // Wildcard overflow: only routes whose non-wildcard segment count fits.
        for route in &self.wildcards {
            if route.min_segments() > path_len {
                continue;
            }
            if !match_segments(&route.segments, path_segments, &mut captures) {
                continue;
            }
            if route.method != request.method {
                path_matched = true;
                continue;
            }
            for (name, value) in captures.as_slice() {
                request.params.insert(name.to_string(), value.to_string());
            }
            return (route.handler)(&request);
        }

        if path_matched {
            return Response::error(405, "method not allowed");
        }

        Response::error(404, format!("no route for {}", request.path))
    }
}

/// Canonical key form for exact routes: strips redundant trailing slashes.
/// Borrowed when the path is already canonical, so the hot lookup is free.
fn normalize_path(path: &str) -> Cow<'_, str> {
    if path == "/" {
        return Cow::Borrowed("/");
    }
    if path.starts_with('/') && !path.ends_with('/') && !path.contains("//") {
        return Cow::Borrowed(path);
    }
    Cow::Owned(format!("/{}", path.trim_matches('/')))
}

fn compile_pattern(pattern: &str) -> Vec<Segment> {
    split_path(pattern)
        .map(|seg| {
            if seg == "*" {
                Segment::Wildcard
            } else if let Some(name) = seg.strip_prefix(':') {
                Segment::Param(name.to_string())
            } else {
                Segment::Literal(seg.to_string())
            }
        })
        .collect()
}

fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.trim_matches('/').split('/').filter(|s| !s.is_empty())
}

/// Match request path segments against a route pattern. Returns captured
/// params as borrowed slices; only the matched route pays for owning them.
pub(crate) fn match_segments<'r, 'p>(
    pattern: &'r [Segment],
    path: &[&'p str],
    captures: &mut Captures<'r, 'p>,
) -> bool {
    captures.clear();
    let mut pi = 0;

    for seg in pattern {
        match seg {
            Segment::Wildcard => {
                // Trailing `*` matches the rest of the path (possibly empty).
                return true;
            }
            Segment::Literal(lit) => {
                if pi >= path.len() || path[pi] != lit.as_str() {
                    return false;
                }
                pi += 1;
            }
            Segment::Param(name) => {
                if pi >= path.len() {
                    return false;
                }
                captures.push(name.as_str(), path[pi]);
                pi += 1;
            }
        }
    }

    pi == path.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: Method, path: &str) -> Request {
        Request {
            method,
            target: path.to_string(),
            path: path.to_string(),
            query: HashMap::new(),
            headers: crate::types::Headers::new(),
            body: Vec::new(),
            params: HashMap::new(),
        }
    }

    #[test]
    fn exact_match_and_params() {
        let router = Router::new()
            .get("/health", |_| Response::text("ok"))
            .get("/rooms/:id/players/:pid", |r| {
                Response::text(format!("{}-{}", r.param("id").unwrap(), r.param("pid").unwrap()))
            });

        let resp = router.dispatch(req(Method::Get, "/health"));
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"ok");

        let resp = router.dispatch(req(Method::Get, "/rooms/7/players/9"));
        assert_eq!(resp.body, b"7-9");
    }

    #[test]
    fn single_param_route() {
        let router = Router::new()
            .get("/:engine", |r| Response::text(r.param("engine").unwrap().to_string()));
        assert_eq!(router.dispatch(req(Method::Get, "/quic")).body, b"quic");
        assert_eq!(router.dispatch(req(Method::Get, "/tcp")).body, b"tcp");
        // two-segment path does not belong to the 1-segment bucket
        assert_eq!(router.dispatch(req(Method::Get, "/a/b")).status, 404);
    }

    #[test]
    fn static_wins_over_dynamic_same_length() {
        let router = Router::new()
            .get("/engines", |_| Response::text("static"))
            .get("/:name", |r| Response::text(format!("dyn-{}", r.param("name").unwrap())));
        // "/engines" hits the exact HashMap directly (1 segment).
        assert_eq!(router.dispatch(req(Method::Get, "/engines")).body, b"static");
        assert_eq!(router.dispatch(req(Method::Get, "/other")).body, b"dyn-other");
    }

    #[test]
    fn dynamic_bucket_isolates_by_length() {
        let router = Router::new()
            .get("/a/:x", |_| Response::text("two"))
            .get("/b/:x/:y", |_| Response::text("three"));
        assert_eq!(router.dispatch(req(Method::Get, "/a/1")).body, b"two");
        assert_eq!(router.dispatch(req(Method::Get, "/b/1/2")).body, b"three");
        assert_eq!(router.dispatch(req(Method::Get, "/a/1/2")).status, 404);
    }

    #[test]
    fn wildcard_matches_rest() {
        let router = Router::new().get("/static/*", |_| Response::text("served"));
        assert_eq!(router.dispatch(req(Method::Get, "/static/a/b.css")).status, 200);
        assert_eq!(router.dispatch(req(Method::Get, "/static/")).status, 200);
        assert_eq!(router.dispatch(req(Method::Get, "/static")).status, 200);
        assert_eq!(router.dispatch(req(Method::Get, "/other")).status, 404);
    }

    #[test]
    fn method_mismatch_is_405() {
        let router = Router::new().post("/rooms", |_| Response::text("created"));
        let resp = router.dispatch(req(Method::Get, "/rooms"));
        assert_eq!(resp.status, 405);
    }

    #[test]
    fn method_mismatch_on_dynamic_is_405() {
        let router = Router::new()
            .post("/rooms/:id", |_| Response::text("ok"))
            .get("/other", |_| Response::text("x"));
        assert_eq!(router.dispatch(req(Method::Get, "/rooms/42")).status, 405);
    }

    #[test]
    fn unknown_path_is_404() {
        let router = Router::new().get("/a", |_| Response::text("a"));
        assert_eq!(router.dispatch(req(Method::Get, "/b")).status, 404);
    }

    #[test]
    fn root_path() {
        let router = Router::new().get("/", |_| Response::text("root"));
        assert_eq!(router.dispatch(req(Method::Get, "/")).status, 200);
    }
}
