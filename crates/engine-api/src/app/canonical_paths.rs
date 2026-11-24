//! Canonical custom-method URLs.
//!
//! `API Design Patterns` (ch. 9) writes an action on a resource as
//! `/resources/{id}:verb`. Axum's router does not allow a literal after a path
//! parameter in the same segment, so this adapter rewrites the canonical URL
//! into the equivalent nested route *before* the router matches. Handlers stay
//! ordinary Axum routes; clients get the book's URL shape.
//!
//! Only a segment whose suffix is a lowercase action word is rewritten. A
//! resource id never contains a colon, so an accidental colon in the middle of
//! a path cannot silently change the route.

use std::convert::Infallible;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::{Request, Response, Uri};
use tower::Service;

#[derive(Clone)]
pub struct CanonicalPaths<S> {
    inner: S,
}

impl<S> CanonicalPaths<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<Request<Body>> for CanonicalPaths<S>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible> + Clone,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if let Some(path) = canonical_path(request.uri().path()) {
            let query = request.uri().query().map(|query| format!("?{query}"));
            let uri = format!("{path}{}", query.as_deref().unwrap_or_default())
                .parse::<Uri>()
                .expect("a path built from a valid URI stays valid");
            *request.uri_mut() = uri;
        }
        self.inner.call(request)
    }
}

/// Rewrite `/a/b/123:kick?x=1` into `/a/b/123/kick?x=1`. A path without a
/// custom-method segment is returned unchanged (as `None`).
pub fn canonical_path(path: &str) -> Option<String> {
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let mut segments: Vec<&str> = path.split('/').collect();
    let mut changed = false;
    let mut rewritten: Vec<String> = Vec::with_capacity(segments.len() + 1);
    for segment in segments.drain(..) {
        if let Some((id, action)) = segment.rsplit_once(':') {
            if !id.is_empty() && is_action(action) {
                rewritten.push(id.to_owned());
                rewritten.push(action.to_owned());
                changed = true;
                continue;
            }
        }
        rewritten.push(segment.to_owned());
    }
    if !changed {
        return None;
    }
    let mut out = rewritten.join("/");
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    Some(out)
}

/// The custom verbs this API actually exposes. An arbitrary `key:value` path
/// segment (a storage key, for example) must not be rewritten just because it
/// contains a colon.
const ACTIONS: &[&str] = &[
    "activate",
    "ban",
    "batch-create",
    "batch-delete",
    "batch-get",
    "batch-update",
    "block",
    "buy",
    "cancel",
    "claim",
    "consume",
    "create",
    "custom",
    "deactivate",
    "define",
    "delete",
    "delete-by-filter",
    "demote",
    "dry-run",
    "equip",
    "export",
    "grant",
    "import",
    "join",
    "kick",
    "leave",
    "promote",
    "publish",
    "purchase",
    "refund",
    "reset",
    "retry",
    "rollback",
    "settle",
    "stop",
    "subscribe",
    "test",
    "unban",
    "undelete",
    "unsubscribe",
    "upload",
    "validate",
    "verify",
];

fn is_action(action: &str) -> bool {
    ACTIONS.contains(&action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resource_action_becomes_a_nested_route() {
        assert_eq!(
            canonical_path("/v1/group/0190:kick").as_deref(),
            Some("/v1/group/0190/kick")
        );
        assert_eq!(
            canonical_path("/v1/console/games/abc:undelete?x=1").as_deref(),
            Some("/v1/console/games/abc/undelete?x=1")
        );
    }

    #[test]
    fn ordinary_paths_are_left_alone() {
        assert_eq!(canonical_path("/v1/leaderboard"), None);
        assert_eq!(canonical_path("/v1/storage/object/a:b"), None);
        assert_eq!(
            canonical_path("/v1/storage/object/a:kick").as_deref(),
            Some("/v1/storage/object/a/kick"),
            "a known verb still rewrites"
        );
    }
}
