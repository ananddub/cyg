use auto_di::Container;
use auto_route::{build_routes, controller, get, openapi_json, scalar_routes};
use axum::{
    Form, Json,
    body::Body,
    extract::{Path, Query},
    http::{Request, StatusCode},
    response::sse::{Event, Sse},
};
use futures_util::stream;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{convert::Infallible, pin::Pin};
use tower::ServiceExt;

type TestEventStream = Pin<Box<dyn futures_util::Stream<Item = Result<Event, Infallible>> + Send>>;
type TestSse = Sse<TestEventStream>;

#[derive(Clone, Debug, Serialize, poem_openapi::Object, ts_rs::TS)]
struct Metrics {
    cpu: u64,
}

struct ServerController;

#[controller("/servers")]
impl ServerController {
    fn new() -> Self {
        Self
    }

    #[get("/{server_id}/metrics")]
    #[live]
    #[on("metrics:update")]
    async fn metrics(&self, Path(server_id): Path<i64>) -> Json<Metrics> {
        Json(Metrics {
            cpu: server_id as u64,
        })
    }

    #[get("/{server_id}/search")]
    #[live("metric_search", table = "metrics")]
    async fn search_metrics(
        &self,
        _claims: Claims,
        Path(server_id): Path<i64>,
        Query(_query): Query<SearchQuery>,
    ) -> Json<Metrics> {
        Json(Metrics {
            cpu: server_id as u64,
        })
    }
}

struct DefaultLiveController;

#[derive(Clone)]
struct TestUser {
    user_id: i64,
}

#[derive(Clone)]
struct Claims {
    user: TestUser,
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Claims {
    type Rejection = StatusCode;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            user: TestUser { user_id: 42 },
        })
    }
}

#[controller("/default-live")]
impl DefaultLiveController {
    fn new() -> Self {
        Self
    }

    #[get("")]
    #[live("server_status", tables = ["servers", "metrics"])]
    async fn status(&self) -> Json<Metrics> {
        Json(Metrics { cpu: 1 })
    }

    #[get("/latest")]
    #[live("latest_status", strategy = latest)]
    async fn latest(&self) -> Json<Metrics> {
        Json(Metrics { cpu: 2 })
    }

    #[get("/stream")]
    #[live("status_stream", strategy = stream, capacity = 64, replay = 10)]
    async fn stream(&self) -> Json<Metrics> {
        Json(Metrics { cpu: 3 })
    }

    #[get("/private")]
    #[live("private_status", strategy = latest)]
    async fn private(&self, _claims: Claims) -> Json<Metrics> {
        Json(Metrics { cpu: 4 })
    }
}

#[test]
fn generates_typed_live_publish_handle() {
    let publisher = server_live::metrics(7).expect("live publisher");
    assert_eq!(publisher.endpoint(), "ServerController::metrics");
    assert_eq!(
        server_live::metrics_subscription(7).unwrap().client_name(),
        "metrics"
    );
    assert_eq!(
        defaultlive_live::status().unwrap().endpoint(),
        "DefaultLiveController::status"
    );
    assert_eq!(
        defaultlive_live::status_subscription()
            .unwrap()
            .client_name(),
        "server_status"
    );
    assert_eq!(
        DefaultLiveController::server_status().unwrap().endpoint(),
        "DefaultLiveController::status"
    );
    assert_eq!(
        DefaultLiveController::server_status_subscription()
            .unwrap()
            .client_name(),
        "server_status"
    );
    assert_eq!(
        DefaultLiveController::latest_status().unwrap().endpoint(),
        "DefaultLiveController::latest"
    );
    assert_eq!(
        DefaultLiveController::status_stream().unwrap().endpoint(),
        "DefaultLiveController::stream"
    );
    let claims = Claims {
        user: TestUser { user_id: 42 },
    };
    assert_eq!(
        DefaultLiveController::private_status(&claims)
            .unwrap()
            .endpoint(),
        "DefaultLiveController::private"
    );
    assert_eq!(
        DefaultLiveController::private_status_subscription()
            .unwrap()
            .args(),
        &serde_json::json!([])
    );
}

#[test]
fn generates_live_http_refetch_metadata() {
    let manifest = auto_route::live_client_manifest();
    let endpoint = manifest["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["endpoint"] == "ServerController::search_metrics")
        .expect("live client endpoint");

    assert_eq!(endpoint["hook"], "useServerMetricSearch");
    assert_eq!(endpoint["path"], "/servers/{server_id}/search");
    assert_eq!(endpoint["arguments"][0]["kind"], "path");
    assert_eq!(endpoint["arguments"][0]["index"], 0);
    assert_eq!(endpoint["arguments"][0]["names"], json!(["server_id"]));
    assert_eq!(endpoint["arguments"][1]["kind"], "query");
    assert_eq!(endpoint["arguments"][1]["index"], 1);
    assert_eq!(
        endpoint["parameters"],
        "server_id: bigint, query: { term: string, page: number | null, }"
    );
}

struct UserController {
    prefix: String,
}

#[derive(Deserialize, Serialize, poem_openapi::Object, ts_rs::TS)]
struct SearchQuery {
    term: String,
    page: Option<i32>,
}

#[derive(Deserialize, poem_openapi::Object)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(poem_openapi::Object)]
struct TypedSseEvent {
    kind: String,
    line: String,
}

#[test]
fn generates_openapi_from_registered_routes() {
    let spec = openapi_json();

    assert_eq!(spec["openapi"], "3.0.0");
    assert!(spec["paths"]["/health"]["get"].is_object());
    assert!(spec["paths"]["/users"]["post"].is_object());

    let get_user = &spec["paths"]["/users/{id}"]["get"];
    assert_eq!(get_user["operationId"], "UserController::get_user");
    assert_eq!(get_user["tags"], json!(["Users"]));
    assert_eq!(get_user["summary"], "Fetch one user");
    assert_eq!(get_user["description"], "Returns a user by id.");
    assert_eq!(get_user["parameters"][0]["name"], "id");
    assert_eq!(get_user["parameters"][0]["in"], "path");
    assert_eq!(get_user["parameters"][0]["required"], true);
    assert_eq!(get_user["parameters"][0]["schema"]["type"], "integer");

    let module = &spec["paths"]["/module/{id}"]["get"];
    assert_eq!(module["operationId"], "module_routes::find");
    assert_eq!(module["tags"], json!(["module"]));
    assert_eq!(module["parameters"][0]["schema"]["type"], "integer");

    let create_user = &spec["paths"]["/users"]["post"];
    assert_eq!(create_user["summary"], "Create user");
    assert_eq!(
        create_user["requestBody"]["description"],
        "JSON user payload"
    );
    assert_eq!(
        create_user["responses"]["200"]["description"],
        "Created user payload"
    );
    assert!(create_user["requestBody"]["required"].as_bool().unwrap());
    assert!(
        create_user["requestBody"]["content"]["application/json"]["schema"].is_object(),
        "POST /users should expose the inferred Json<Value> request schema"
    );
    assert!(
        create_user["responses"]["200"]["content"]["application/json"]["schema"].is_object(),
        "POST /users should expose the inferred Json<Value> response schema"
    );
    assert!(
        get_user["responses"]["200"]["content"]["application/json"]["schema"].is_object(),
        "GET /users/{{id}} should expose the inferred Json<Value> response schema"
    );

    let search = &spec["paths"]["/search"]["get"];
    assert_eq!(search["summary"], "Search users");
    assert_eq!(search["description"], "Searches users by query params.");
    assert_eq!(search["parameters"][0]["name"], "query");
    assert_eq!(search["parameters"][0]["in"], "query");
    assert_eq!(search["parameters"][0]["style"], "form");
    assert_eq!(search["parameters"][0]["explode"], true);
    assert!(search["parameters"][0]["schema"].is_object());

    let login = &spec["paths"]["/login"]["post"];
    assert!(
        login["requestBody"]["content"]["application/x-www-form-urlencoded"]["schema"].is_object()
    );
    assert!(
        login["responses"]["200"]["content"]["application/json"]["schema"].is_object(),
        "form handlers should still expose the JSON response schema"
    );

    let events = &spec["paths"]["/events"]["get"];
    assert!(
        events["responses"]["200"]["content"]["text/event-stream"]["schema"].is_object(),
        "SSE handlers should expose text/event-stream response content"
    );
    assert!(
        events["responses"]["200"]["content"]["application/json"].is_null(),
        "SSE handlers should not be documented as JSON responses"
    );
    assert_eq!(
        events["responses"]["200"]["content"]["text/event-stream"]["schema"]["format"],
        "event-stream"
    );

    let aliased_events = &spec["paths"]["/aliased-events"]["get"];
    assert!(
        aliased_events["responses"]["200"]["content"]["text/event-stream"]["schema"].is_object(),
        "SSE aliases inside Result should expose text/event-stream response content"
    );
    assert_eq!(
        aliased_events["responses"]["200"]["content"]["text/event-stream"]["schema"]["format"],
        "event-stream"
    );

    let typed_events = &spec["paths"]["/typed-events"]["get"];
    let typed_schema = &typed_events["responses"]["200"]["content"]["text/event-stream"]["schema"];
    assert!(
        typed_schema["$ref"]
            .as_str()
            .is_some_and(|reference| reference.ends_with("/TypedSseEvent")),
        "typed SSE handlers should expose the declared event payload schema"
    );
}

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}

#[get(
    "/search",
    summary = "Search users",
    description = "Searches users by query params."
)]
async fn search(Query(_query): Query<SearchQuery>) -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[auto_route::post("/login")]
async fn login(Form(_form): Form<LoginForm>) -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[get("/events")]
async fn events() -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    Sse::new(stream::empty())
}

#[get("/aliased-events")]
async fn aliased_events() -> Result<TestSse, (StatusCode, String)> {
    let stream: TestEventStream = Box::pin(stream::empty());
    Ok(Sse::new(stream))
}

#[get("/typed-events", sse = TypedSseEvent)]
async fn typed_events() -> Result<TestSse, (StatusCode, String)> {
    let stream: TestEventStream = Box::pin(stream::empty());
    Ok(Sse::new(stream))
}

#[controller("/module")]
mod module_routes {
    use axum::extract::Path;

    #[get("/:id")]
    async fn find(Path(id): Path<u64>) -> String {
        format!("module-{id}")
    }
}

#[controller("/users", tag = "Users", description = "User management endpoints.")]
impl UserController {
    pub fn new() -> Self {
        Self {
            prefix: "user".to_owned(),
        }
    }

    #[get("/:id", description = "Returns a user by id.")]
    /// Fetch one user
    async fn get_user(&self, Path(id): Path<u64>) -> Json<Value> {
        Json(json!({ "name": format!("{}-{id}", self.prefix) }))
    }

    #[post(
        summary = "Create user",
        request_description = "JSON user payload",
        response_description = "Created user payload"
    )]
    async fn create_user(&self, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
        (StatusCode::CREATED, Json(body))
    }
}

#[tokio::test]
async fn dispatches_get_and_post_to_the_di_managed_controller() {
    let container = Container::new().unwrap();
    let app = build_routes(&container).await.unwrap();

    let response = app
        .clone()
        .oneshot(Request::get("/users/42").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({ "name": "user-42" })
    );

    let response = app
        .clone()
        .oneshot(
            Request::post("/users")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"ok");

    let response = app
        .oneshot(Request::get("/module/7").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"module-7");
}

#[tokio::test]
async fn serves_scalar_api_reference() {
    let app = scalar_routes("/scalar", "/openapi.json");

    let response = app
        .clone()
        .oneshot(Request::get("/scalar").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("/openapi.json"));
    assert!(html.contains("/scalar/scalar.js"));

    let response = app
        .oneshot(
            Request::get("/scalar/scalar.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
