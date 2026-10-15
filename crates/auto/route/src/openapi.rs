use crate::{OpenApiParamDescriptor, OpenApiRouteDescriptor, OpenApiSchemaDescriptor};
use poem_openapi::{
    __private::poem::{endpoint::BoxEndpoint, http::Method},
    OpenApi, OpenApiService,
    registry::*,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub struct AutoRouteOpenApi;
impl OpenApi for AutoRouteOpenApi {
    fn meta() -> Vec<MetaApi> {
        let mut paths = BTreeMap::<String, Vec<MetaOperation>>::new();
        for descriptor in inventory::iter::<OpenApiRouteDescriptor> {
            paths
                .entry(descriptor.path.to_owned())
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
pub fn openapi_spec() -> String {
    OpenApiService::new(
        AutoRouteOpenApi,
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    )
    .spec()
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
