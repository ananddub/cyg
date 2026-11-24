//! Book patterns that are cheaper to enforce once, in front of the router.
//!
//! * `PATCH` is the book's partial update (`ch. 8`): the handler still sees a
//!   whole resource, because this layer reads the current representation,
//!   applies the patch (or only the fields named by `?updateMask=`), and forwards
//!   a `PUT`. `?validateOnly=true` returns the merged representation without
//!   forwarding anything.
//! * Revisions (`ch. 28`): every single-resource `GET` gets an `ETag` and an
//!   `@revision` field derived from the row's own `version`/`updated_at`. A
//!   write carrying `If-Match` first reads the current tag and refuses a stale
//!   write with `412`.
//!
//! Both rules are applied from the resource representation, so a new controller
//! gets them without a second implementation.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header};
use axum::response::IntoResponse;
use serde_json::{Map, Value};
use tower::Service;
use tower::util::Oneshot;

use crate::error::V2Error;

const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct BookCompat<S> {
    inner: S,
}

impl<S> BookCompat<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<Request<Body>> for BookCompat<S>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let request = match crate::app::book_signing::verify(request).await {
                Ok(request) => request,
                Err(response) => return Ok(response),
            };
            let method = request.method().clone();
            let filter = query_param(request.uri().query(), "filter");
            let versioned = request.uri().path().starts_with("/v1/");
            if method == Method::POST
                && versioned
                && !request.uri().path().starts_with("/v1/console/games")
                && let Some((base, id)) = undelete_target(request.uri().path())
            {
                return Ok(undelete(inner, request, base, id).await);
            }
            let versioned = request.uri().path().starts_with("/v1/");
            if method == Method::POST
                && versioned
                && !request.uri().path().starts_with("/v1/console/games")
                && let Some((action, base)) = batch_action(request.uri().path())
            {
                return Ok(batch(inner, request, action, base).await);
            }
            // The games domain already has a first-class PATCH (typed mask and
            // validateOnly); do not double-merge it here.
            if method == Method::PATCH && versioned && !is_games_patch(request.uri().path()) {
                return Ok(patch(inner, request).await);
            }
            if matches!(method, Method::PUT | Method::PATCH | Method::DELETE)
                && let Some(expected) = expected_if_match(request.headers())
            {
                let current_request = Request::builder()
                    .method(Method::GET)
                    .uri(request.uri().clone())
                    .body(Body::empty())
                    .expect("a cloned URI is valid");
                let mut current_request = current_request;
                *current_request.headers_mut() = forwarding_headers(request.headers());
                match fetch_json(inner.clone(), current_request).await {
                    Ok(current) => {
                        if revision(&current).is_none_or(|current| current != expected) {
                            return Ok(precondition_failed(&expected));
                        }
                    }
                    Err(response) => return Ok(response),
                }
            }
            let response = oneshot(inner, request).await;
            if method == Method::GET {
                let response = match filter {
                    Some(filter) => match apply_filter(response, &filter).await {
                        Ok(response) => response,
                        Err(error) => error.into_response(),
                    },
                    None => response,
                };
                return Ok(decorate(response).await);
            }
            Ok(response)
        })
    }
}

async fn oneshot<S>(inner: S, request: Request<Body>) -> Response<Body>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible> + 'static,
    S::Future: Send + 'static,
{
    match Oneshot::new(inner, request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

/// `POST /resource/{id}:undelete` is the book's custom method for a soft
/// delete. The global layer can only restore a representation that carries a
/// `status`; a resource without one is refused instead of guessed at.
fn undelete_target(path: &str) -> Option<(String, String)> {
    let (base, last) = path.rsplit_once('/')?;
    let (id, action) = last.rsplit_once(':')?;
    if action != "undelete" || id.is_empty() || base.is_empty() {
        return None;
    }
    Some((base.to_owned(), id.to_owned()))
}

async fn undelete<S>(inner: S, request: Request<Body>, base: String, id: String) -> Response<Body>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let (parts, _) = request.into_parts();
    let uri = format!("{base}/{id}");
    let mut current = match get_json(inner.clone(), &uri, &parts.headers).await {
        Ok(current) => current,
        Err(response) => return response,
    };
    let Some(status) = current.get("status").and_then(Value::as_str) else {
        return V2Error::invalid_argument(
            "This resource has no soft-delete state, so it cannot be undeleted.",
        )
        .into_response();
    };
    if status != "archived" {
        return V2Error::invalid_argument(
            "This resource is not archived, so there is nothing to undelete.",
        )
        .into_response();
    }
    let Some(object) = current.as_object_mut() else {
        return V2Error::invalid_argument("The resource representation is not an object.")
            .into_response();
    };
    object.insert("status".to_owned(), Value::String("active".to_owned()));
    let bytes = match serde_json::to_vec(&current) {
        Ok(bytes) => bytes,
        Err(error) => {
            return V2Error::internal(format!("Could not encode the restored resource: {error}"))
                .into_response();
        }
    };
    let mut put = Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .body(Body::from(bytes))
        .expect("a built restore URI is valid");
    *put.headers_mut() = forwarding_headers(&parts.headers);
    put.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let response = oneshot(inner, put).await;
    if !response.status().is_success() {
        return response;
    }
    decorate(axum::Json(current).into_response()).await
}

#[derive(Clone, Copy)]
enum BatchAction {
    Get,
    Delete,
    DeleteByFilter,
}

fn batch_action(path: &str) -> Option<(BatchAction, String)> {
    let (base, last) = path.rsplit_once('/')?;
    let (base, action) = if let Some((collection, action)) = last.rsplit_once(':') {
        (format!("{base}/{collection}"), action)
    } else {
        (base.to_owned(), last)
    };
    let action = match action {
        "batch-get" => BatchAction::Get,
        "batch-delete" => BatchAction::Delete,
        "delete-by-filter" => BatchAction::DeleteByFilter,
        _ => return None,
    };
    if base.is_empty() || base == "/v1" || base == "/" {
        return None;
    }
    Some((action, base))
}

async fn batch<S>(
    inner: S,
    request: Request<Body>,
    action: BatchAction,
    base: String,
) -> Response<Body>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => return V2Error::invalid_argument("Batch body too large.").into_response(),
    };
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                return V2Error::invalid_argument(format!("Invalid batch body: {error}"))
                    .into_response();
            }
        }
    };
    let query = batch_query(parts.uri.query().unwrap_or_default());

    if matches!(action, BatchAction::DeleteByFilter) {
        let raw_filter = body
            .get("filter")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| query_pairs(&query).remove("filter"));
        let Some(raw_filter) = raw_filter else {
            return V2Error::invalid_argument("A criteria delete needs a `filter` expression.")
                .into_response();
        };
        let expression = match crate::app::book_filter::parse(&raw_filter) {
            Ok(expression) => expression,
            Err(error) => return V2Error::invalid_argument(error).into_response(),
        };
        let list_uri = format!("{base}?limit=100&{query}");
        let list = match get_json(inner.clone(), &list_uri, &parts.headers).await {
            Ok(list) => list,
            Err(response) => return response,
        };
        let Some(values) = first_array(&list) else {
            return V2Error::invalid_argument(
                "The criteria-delete target has no list representation.",
            )
            .into_response();
        };
        let ids: Vec<String> = values
            .iter()
            .filter(|value| crate::app::book_filter::matches(&expression, value))
            .filter_map(|value| value.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        return delete_ids(inner, &base, &query, &parts.headers, ids).await;
    }

    let ids = match body.get("ids").and_then(Value::as_array) {
        Some(ids) => ids,
        None => {
            return V2Error::invalid_argument("A batch body must carry an `ids` array.")
                .into_response();
        }
    };
    let mut unique = Vec::new();
    for id in ids {
        let Some(id) = id.as_str() else {
            return V2Error::invalid_argument("Every batch id must be a string.").into_response();
        };
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('?')
            || id.contains('#')
        {
            return V2Error::invalid_argument(format!("Invalid batch id `{id}`.")).into_response();
        }
        if !unique.iter().any(|seen| seen == id) {
            unique.push(id.to_owned());
        }
    }
    if unique.is_empty() {
        return V2Error::invalid_argument("A batch must name at least one id.").into_response();
    }
    if unique.len() > 100 {
        return V2Error::invalid_argument("A batch names at most 100 ids.").into_response();
    }
    if matches!(action, BatchAction::Delete) {
        return delete_ids(inner, &base, &query, &parts.headers, unique).await;
    }

    let mut items = Vec::new();
    let mut missing = Vec::new();
    for id in unique {
        match get_json(
            inner.clone(),
            &format!("{base}/{id}?{query}"),
            &parts.headers,
        )
        .await
        {
            Ok(value) => items.push(value),
            Err(_) => missing.push(id),
        }
    }
    axum::Json(serde_json::json!({ "items": items, "missing": missing })).into_response()
}

fn batch_query(raw: &str) -> String {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = pair.split_once('=').map(|(key, _)| key).unwrap_or(pair);
            !matches!(
                key,
                "filter"
                    | "updateMask"
                    | "update_mask"
                    | "validateOnly"
                    | "validate_only"
                    | "cursor"
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

async fn get_json<S>(inner: S, uri: &str, headers: &HeaderMap) -> Result<Value, Response<Body>>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible> + 'static,
    S::Future: Send + 'static,
{
    let mut get = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .expect("a built batch URI is valid");
    *get.headers_mut() = forwarding_headers(headers);
    fetch(inner, get).await
}

fn first_array(value: &Value) -> Option<&Vec<Value>> {
    if let Value::Array(array) = value {
        return Some(array);
    }
    let Value::Object(object) = value else {
        return None;
    };
    if object.len() == 1
        && let Some(Value::Array(array)) = object.values().next()
    {
        return Some(array);
    }
    object.values().find_map(|candidate| match candidate {
        Value::Array(array) => Some(array),
        _ => None,
    })
}

async fn delete_ids<S>(
    inner: S,
    base: &str,
    query: &str,
    headers: &HeaderMap,
    ids: Vec<String>,
) -> Response<Body>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let mut deleted = 0usize;
    let mut failed = Vec::new();
    for id in ids {
        let uri = if query.is_empty() {
            format!("{base}/{id}")
        } else {
            format!("{base}/{id}?{query}")
        };
        let mut delete = Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .expect("a built delete URI is valid");
        *delete.headers_mut() = forwarding_headers(headers);
        let response = oneshot(inner.clone(), delete).await;
        if response.status().is_success() {
            deleted += 1;
        } else {
            failed.push(serde_json::json!({
                "id": id,
                "status": response.status().as_u16(),
            }));
        }
    }
    axum::Json(serde_json::json!({ "deleted": deleted, "failed": failed })).into_response()
}

async fn patch<S>(inner: S, request: Request<Body>) -> Response<Body>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => return V2Error::invalid_argument("Patch body too large.").into_response(),
    };
    let patch: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return V2Error::invalid_argument(format!("Invalid patch body: {error}"))
                .into_response();
        }
    };
    let query = query_pairs(parts.uri.query().unwrap_or_default());
    let mask = match query.get("updateMask").or_else(|| query.get("update_mask")) {
        Some(raw) => match parse_mask(raw) {
            Ok(mask) => Some(mask),
            Err(error) => return error.into_response(),
        },
        None => None,
    };
    let validate_only = matches!(
        query
            .get("validateOnly")
            .or_else(|| query.get("validate_only"))
            .map(String::as_str),
        Some("true" | "1" | "t" | "T" | "TRUE")
    );

    let mut get = Request::builder()
        .method(Method::GET)
        .uri(without_control_params(parts.uri.clone()))
        .body(Body::empty())
        .expect("a cloned URI is valid");
    *get.headers_mut() = forwarding_headers(&parts.headers);
    let current = match fetch(inner.clone(), get).await {
        Ok(current) => unwrap_resource(current),
        Err(response) => return response,
    };
    let merged = match merge(current, patch, mask.as_deref()) {
        Ok(merged) => merged,
        Err(error) => return error.into_response(),
    };
    if validate_only {
        let mut response = axum::Json(merged).into_response();
        response
            .headers_mut()
            .insert("x-cyg-validate-only", HeaderValue::from_static("true"));
        return response;
    }

    let bytes = match serde_json::to_vec(&merged) {
        Ok(bytes) => bytes,
        Err(error) => {
            return V2Error::invalid_argument(format!("Could not encode the patch: {error}"))
                .into_response();
        }
    };
    let mut put = Request::builder()
        .method(Method::PUT)
        .uri(parts.uri)
        .body(Body::from(bytes))
        .expect("the original URI is valid");
    *put.headers_mut() = forwarding_headers(&parts.headers);
    put.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let response = oneshot(inner, put).await;
    if !response.status().is_success() {
        return response;
    }
    // A number of write handlers answer with an empty acknowledgement. The
    // book's partial update returns the resource as it now stands.
    decorate(axum::Json(merged).into_response()).await
}

/// The games controller owns PATCH already; its `updateMask` rules are typed
/// (including nested `config.tick_rate`) and must not be re-merged here.
fn is_games_patch(path: &str) -> bool {
    path.starts_with("/v1/console/games/")
}

fn query_param(raw: Option<&str>, name: &str) -> Option<String> {
    query_pairs(raw?).remove(name)
}

fn query_pairs(raw: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(key);
        let value = percent_decode(value);
        out.insert(key, value);
    }
    out
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn without_control_params(uri: axum::http::Uri) -> axum::http::Uri {
    let Some(query) = uri.query().map(str::to_owned) else {
        return uri;
    };
    let path = uri.path().to_owned();
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split_once('=').map(|(key, _)| key).unwrap_or(pair);
            !matches!(
                key,
                "updateMask" | "update_mask" | "validateOnly" | "validate_only"
            )
        })
        .filter(|pair| !pair.is_empty())
        .collect();
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        if kept.is_empty() {
            parts
                .path_and_query
                .as_ref()
                .expect("path present")
                .path()
                .to_owned()
        } else {
            format!(
                "{}?{}",
                parts.path_and_query.as_ref().expect("path present").path(),
                kept.join("&")
            )
        }
        .parse()
        .expect("a rebuilt query is valid"),
    );
    axum::http::Uri::from_parts(parts).expect("the URI parts stay valid")
}

fn forwarding_headers(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in headers {
        if matches!(
            name.as_str(),
            "content-length" | "content-type" | "host" | "transfer-encoding"
        ) {
            continue;
        }
        out.append(name, value.clone());
    }
    out
}

fn parse_mask(raw: &str) -> Result<Vec<String>, V2Error> {
    let mask: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(str::to_owned)
        .collect();
    if mask.is_empty() {
        return Err(V2Error::invalid_argument(
            "updateMask must name at least one field.",
        ));
    }
    Ok(mask)
}

/// Apply a JSON merge patch, optionally restricted to the fields named by
/// `updateMask`. A mask that names a field the patch does not set is refused:
/// silently ignoring it would tell the caller their edit landed when it did
/// not.
pub fn merge(mut base: Value, patch: Value, mask: Option<&[String]>) -> Result<Value, V2Error> {
    let Value::Object(patch) = patch else {
        return Err(V2Error::invalid_argument(
            "A patch body must be a JSON object.",
        ));
    };
    if !base.is_object() {
        return Err(V2Error::invalid_argument(
            "The current representation is not a JSON object, so it cannot be patched.",
        ));
    }
    match mask {
        Some(mask) => {
            for path in mask {
                let Some(value) = value_at(&patch, path) else {
                    return Err(V2Error::invalid_argument(format!(
                        "updateMask names `{path}`, but the patch body does not set it."
                    )));
                };
                if !path_exists(&base, path) {
                    return Err(V2Error::invalid_argument(format!(
                        "updateMask names unknown field `{path}`."
                    )));
                }
                set_path(&mut base, path, value.clone())?;
            }
        }
        None => {
            for (key, value) in patch {
                // A field the representation already has is replaced whole; a
                // new optional field is allowed.
                if let Some(slot) = base.get_mut(&key) {
                    *slot = value;
                } else if let Value::Object(object) = &mut base {
                    object.insert(key, value);
                }
            }
        }
    }
    Ok(base)
}

fn value_at<'a>(object: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut current = object.get(path.split('.').next()?)?;
    for part in path.split('.').skip(1) {
        current = current.get(part)?;
    }
    Some(current)
}

fn path_exists(value: &Value, path: &str) -> bool {
    let mut current = value;
    for part in path.split('.') {
        let Some(next) = current.get(part) else {
            return false;
        };
        current = next;
    }
    true
}

fn set_path(base: &mut Value, path: &str, value: Value) -> Result<(), V2Error> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = base;
    for part in &parts[..parts.len() - 1] {
        let Some(next) = current.get_mut(*part) else {
            return Err(V2Error::invalid_argument(format!(
                "updateMask names unknown field `{path}`."
            )));
        };
        if !next.is_object() {
            return Err(V2Error::invalid_argument(format!(
                "updateMask field `{path}` does not descend through an object."
            )));
        }
        current = next;
    }
    let Some(slot) = current.get_mut(parts[parts.len() - 1]) else {
        return Err(V2Error::invalid_argument(format!(
            "updateMask names unknown field `{path}`."
        )));
    };
    *slot = value;
    Ok(())
}

async fn fetch_json<S>(inner: S, request: Request<Body>) -> Result<Value, Response<Body>>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible> + 'static,
    S::Future: Send + 'static,
{
    fetch(inner, request).await
}

async fn fetch<S>(inner: S, request: Request<Body>) -> Result<Value, Response<Body>>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = Infallible> + 'static,
    S::Future: Send + 'static,
{
    let response = oneshot(inner, request).await;
    if !response.status().is_success() {
        return Err(response);
    }
    let (_parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY).await.map_err(|_| {
        V2Error::internal("The current representation is too large.").into_response()
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        V2Error::invalid_argument("The resource has no JSON representation to patch.")
            .into_response()
    })?;
    Ok(unwrap_resource(value))
}

/// Most read routes answer with a one-field envelope (`{"group": {...}}`).
/// The write routes take the inner object, so PATCH reads through the envelope.
fn unwrap_resource(value: Value) -> Value {
    if let Value::Object(object) = &value
        && object.len() == 1
        && let Some(inner) = object.values().next()
        && inner.is_object()
    {
        return inner.clone();
    }
    value
}

fn expected_if_match(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::IF_MATCH)?.to_str().ok()?.trim();
    if raw == "*" {
        return None;
    }
    Some(
        raw.strip_prefix("W/")
            .unwrap_or(raw)
            .trim_matches('"')
            .to_owned(),
    )
}

/// The revision is the row's own generation counter when it has one, otherwise
/// its last write timestamp. A representation with neither cannot be
/// conditionally written and is left alone.
fn revision(value: &Value) -> Option<String> {
    let raw = value
        .get("version")
        .or_else(|| value.get("@revision"))
        .or_else(|| value.get("updated_at"))
        .or_else(|| value.get("updatedAt"))
        .or_else(|| value.get("update_time"))
        .or_else(|| value.get("updateTime"))
        .or_else(|| value.get("modified_at"))
        .or_else(|| value.get("modifiedAt"))?;
    match raw {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn revision_in(value: &Value) -> Option<String> {
    revision(value).or_else(|| {
        let Value::Object(object) = value else {
            return None;
        };
        if object.len() != 1 {
            return None;
        }
        revision(object.values().next()?)
    })
}

fn precondition_failed(expected: &str) -> Response<Body> {
    let mut response = V2Error::new(
        crate::error::FAILED_PRECONDITION,
        format!("This resource no longer matches If-Match `{expected}`. Re-read it and retry."),
    )
    .into_response();
    *response.status_mut() = StatusCode::PRECONDITION_FAILED;
    response
}

async fn apply_filter(response: Response<Body>, raw: &str) -> Result<Response<Body>, V2Error> {
    if !response.status().is_success() {
        return Ok(response);
    }
    let expression = crate::app::book_filter::parse(raw).map_err(V2Error::invalid_argument)?;
    let (mut parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY)
        .await
        .map_err(|_| V2Error::internal("The filtered representation is too large."))?;
    let mut json: Value = match serde_json::from_slice(&bytes) {
        Ok(json) => json,
        Err(_) => return Ok(Response::from_parts(parts, Body::from(bytes))),
    };
    let Some(array) = list_array(&mut json) else {
        return Err(V2Error::invalid_argument(
            "The `filter` parameter is only valid on a list route.",
        ));
    };
    array.retain(|value| crate::app::book_filter::matches(&expression, value));
    // The page was produced before filtering, so its continuation token would
    // point past rows that were never considered. Dropping it is the honest
    // answer; a filtered list is one page.
    drop_tokens(&mut json);
    let bytes = serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec());
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string()).expect("a length is ASCII"),
    );
    Ok(Response::from_parts(parts, Body::from(bytes)))
}

/// The first JSON array in the reply, either at the top level or inside the
/// one-field envelope list routes use.
fn list_array(value: &mut Value) -> Option<&mut Vec<Value>> {
    if let Value::Array(array) = value {
        return Some(array);
    }
    let Value::Object(object) = value else {
        return None;
    };
    if object.len() == 1 {
        return match object.values_mut().next() {
            Some(Value::Array(array)) => Some(array),
            _ => None,
        };
    }
    object.values_mut().find_map(|candidate| match candidate {
        Value::Array(array) => Some(array),
        _ => None,
    })
}

fn drop_tokens(value: &mut Value) {
    let Value::Object(object) = value else {
        return;
    };
    for key in [
        "nextPageToken",
        "prevPageToken",
        "next_cursor",
        "prev_cursor",
        "cursor",
    ] {
        object.remove(key);
    }
    if object.len() == 1
        && let Some(Value::Object(inner)) = object.values_mut().next()
    {
        for key in [
            "nextPageToken",
            "prevPageToken",
            "next_cursor",
            "prev_cursor",
            "cursor",
        ] {
            inner.remove(key);
        }
    }
}

async fn decorate(response: Response<Body>) -> Response<Body> {
    if !response.status().is_success() {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_BODY).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(mut json) = serde_json::from_slice::<Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let Some(tag) = revision_in(&json) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    if let Value::Object(object) = &mut json {
        let target = if object.len() == 1
            && let Some(Value::Object(inner)) = object.values_mut().next()
        {
            inner
        } else {
            object
        };
        target
            .entry("@revision".to_owned())
            .or_insert_with(|| Value::String(tag.clone()));
    }
    if let Ok(value) = format!("\"{tag}\"").parse() {
        parts.headers.insert(header::ETAG, value);
    }
    let bytes = serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec());
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string()).expect("a length is an ASCII header"),
    );
    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_mask_changes_only_the_named_fields() {
        let base = json!({"name": "old", "status": "active", "config": {"tick_rate": 20}});
        let patch = json!({"name": "new", "status": "archived", "config": {"tick_rate": 30}});
        let mask = vec!["name".to_owned(), "config.tick_rate".to_owned()];
        let merged = merge(base, patch, Some(&mask)).expect("the mask is valid");
        assert_eq!(merged["name"], "new");
        assert_eq!(merged["config"]["tick_rate"], 30);
        assert_eq!(merged["status"], "active");
    }

    #[test]
    fn a_mask_naming_a_missing_patch_field_is_refused() {
        let base = json!({"name": "old"});
        let patch = json!({"name": "new"});
        let error = merge(base, patch, Some(&["status".to_owned()])).unwrap_err();
        assert_eq!(error.kind, crate::error::INVALID_ARGUMENT);
    }

    #[test]
    fn an_absent_mask_applies_every_field_in_the_body() {
        let base = json!({"name": "old", "status": "active"});
        let merged = merge(base, json!({"name": "new"}), None).unwrap();
        assert_eq!(merged["name"], "new");
        assert_eq!(merged["status"], "active");
    }

    #[test]
    fn a_revision_prefers_the_version_counter() {
        assert_eq!(revision(&json!({"version": 7})).as_deref(), Some("7"));
        assert_eq!(
            revision(&json!({"updated_at": "2026-01-01T00:00:00Z"})).as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }
}
