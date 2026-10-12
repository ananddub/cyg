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

use std::collections::HashMap;

use crate::types::{Method, Request, Response};

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
                .entry(normalize_path(pattern))
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
    pub fn dispatch(&self, mut request: Request) -> Response {
        let path = normalize_path(&request.path);

        if let Some(handlers) = self.exact.get(&path) {
            if let Some(handler) = handlers.get(&request.method) {
                return handler(&request);
            }
            return Response::error(405, "method not allowed");
        }

        let path_segments = split_path(&request.path);
        let path_len = path_segments.len();
        let mut path_matched = false;

        // Tier 2: one hash lookup narrows to routes of identical length.
        if let Some(bucket) = self.dynamic.get(&path_len) {
            for route in bucket {
                let Some(captures) = match_segments(&route.segments, &path_segments) else {
                    continue;
                };
                if route.method != request.method {
                    path_matched = true;
                    continue;
                }
                for (name, value) in captures {
                    request.params.insert(name, value);
                }
                return (route.handler)(&request);
            }
        }

        // Wildcard overflow: only routes whose non-wildcard segment count fits.
        for route in &self.wildcards {
            if route.min_segments() > path_len {
                continue;
            }
            let Some(captures) = match_segments(&route.segments, &path_segments) else {
                continue;
            };
            if route.method != request.method {
                path_matched = true;
                continue;
            }
            for (name, value) in captures {
                request.params.insert(name, value);
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
fn normalize_path(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }
    format!("/{}", path.trim_matches('/'))
}

fn compile_pattern(pattern: &str) -> Vec<Segment> {
    split_path(pattern)
        .into_iter()
        .map(|seg| {
            if seg == "*" {
                Segment::Wildcard
            } else if let Some(name) = seg.strip_prefix(':') {
                Segment::Param(name.to_string())
            } else {
                Segment::Literal(seg)
            }
        })
        .collect()
}

fn split_path(path: &str) -> Vec<String> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Match request path segments against a route pattern. Returns captured
/// params on success.
pub(crate) fn match_segments(
    pattern: &[Segment],
    path: &[String],
) -> Option<Vec<(String, String)>> {
    let mut captures = Vec::new();
    let mut pi = 0;

    for seg in pattern {
        match seg {
            Segment::Wildcard => {
                // Trailing `*` matches the rest of the path (possibly empty).
                return Some(captures);
            }
            Segment::Literal(lit) => {
                if pi >= path.len() || &path[pi] != lit {
                    return None;
                }
                pi += 1;
            }
            Segment::Param(name) => {
                if pi >= path.len() {
                    return None;
                }
                captures.push((name.clone(), path[pi].clone()));
                pi += 1;
            }
        }
    }

    if pi == path.len() {
        Some(captures)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
