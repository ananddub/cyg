use crate::{OpenApiParamDescriptor, OpenApiRouteDescriptor, OpenApiSchemaDescriptor};
use poem_openapi::{
    __private::poem::{endpoint::BoxEndpoint, http::Method},
    OpenApi, OpenApiService,
    registry::*,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

pub struct AutoRouteOpenApi;
impl OpenApi for AutoRouteOpenApi {
    fn meta() -> Vec<MetaApi> {
        let mut paths = BTreeMap::<String, Vec<MetaOperation>>::new();
        for descriptor in inventory::iter::<OpenApiRouteDescriptor> {
            paths
                .entry(documented_path(descriptor.path))
                .or_default()
                .push(operation(descriptor));
        }
        vec![MetaApi {
            paths: paths
                .into_iter()
                .map(|(path, operations)| MetaPath { path, operations })
                .collect(),
        }]
    }
    fn register(registry: &mut Registry) {
        for descriptor in inventory::iter::<OpenApiRouteDescriptor> {
            registry.tags.insert(MetaTag {
                name: descriptor.tag,
                description: descriptor.tag_description,
                external_docs: None,
            });
            if let Some(schema) = descriptor.request {
                (schema.register)(registry);
            }
            if let Some(schema) = descriptor.response {
                (schema.register)(registry);
            }
            for param in descriptor.params {
                (param.schema.register)(registry);
            }
        }
    }
    fn add_routes(self, _: &mut HashMap<String, HashMap<Method, BoxEndpoint<'static>>>) {}
}

pub fn openapi_json() -> Value {
    serde_json::from_str(&openapi_spec()).expect("poem-openapi spec must be valid JSON")
}

/// The spec as documented, with the base its controller paths were authored
/// under (`/v1` unless the plane mounted another one).
pub fn openapi_spec() -> String {
    OpenApiService::new(
        AutoRouteOpenApi,
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    )
    .spec()
}

/// The spec with every versioned path published under `base`. The plane calls
/// this with its canonical base before serving, so `/v1` is what a client sees
/// even if the controller attributes still spell an older version.
pub fn openapi_spec_at(base: &str) -> String {
    set_api_base(base);
    openapi_spec()
}

static API_BASE: OnceLock<String> = OnceLock::new();

/// Publish every versioned route under `base`. The first call wins: a process
/// serves one base, and a second one would make the spec disagree with itself.
pub fn set_api_base(base: &str) {
    let _ = API_BASE.set(base.trim_end_matches('/').to_owned());
}

fn api_base() -> &'static str {
    API_BASE.get().map(String::as_str).unwrap_or("/v1")
}

/// Rewrite the leading version segment of a documented path to the served base:
/// `/v1/leaderboard/{id}` published at `/api/v1` becomes
/// `/api/v1/leaderboard/{id}`. Paths outside the versioned tree are untouched.
fn documented_path(path: &str) -> String {
    let mut segments = path.trim_start_matches('/').split('/');
    let Some(version) = segments.next() else {
        return path.to_owned();
    };
    if version != "v1" {
        return path.to_owned();
    }
    let rest: Vec<&str> = segments.collect();
    let mut out = api_base().to_owned();
    for segment in rest {
        out.push('/');
        out.push_str(segment);
    }
    out
}

fn operation(item: &OpenApiRouteDescriptor) -> MetaOperation {
    MetaOperation {
        method: method(item.method),
        tags: vec![item.tag],
        summary: item.summary,
        description: item.description,
        external_docs: None,
        params: item.params.iter().map(|param| param.to_meta()).collect(),
        request: item.request.map(|schema| MetaRequest {
            description: item.request_description,
            content: vec![schema.media_type()],
            required: true,
        }),
        responses: MetaResponses {
            responses: vec![MetaResponse {
                description: item.response_description.unwrap_or("Successful response"),
                status: Some(200),
                status_range: None,
                content: item
                    .response
                    .map(|schema| vec![schema.media_type()])
                    .unwrap_or_default(),
                headers: Vec::new(),
            }],
        },
        deprecated: false,
        security: Vec::new(),
        operation_id: Some(item.operation_id),
        code_samples: Vec::new(),
    }
}
impl OpenApiSchemaDescriptor {
    fn media_type(self) -> MetaMediaType {
        MetaMediaType {
            content_type: self.content_type,
            schema: (self.schema_ref)(),
        }
    }
}
impl OpenApiParamDescriptor {
    fn to_meta(self) -> MetaOperationParam {
        MetaOperationParam {
            name: self.name.trim_start_matches('*').to_owned(),
            schema: (self.schema.schema_ref)(),
            in_type: self.in_type,
            description: None,
            required: self.required,
            deprecated: self.deprecated,
            explode: self.explode,
            style: self.style,
        }
    }
}
fn method(value: &str) -> Method {
    match value {
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "PATCH" => Method::PATCH,
        "OPTIONS" => Method::OPTIONS,
        "HEAD" => Method::HEAD,
        _ => Method::GET,
    }
}
