use poem_openapi::{
    ParameterStyle,
    registry::{MetaParamIn, MetaSchemaRef, Registry},
    types::Type,
};

pub struct OpenApiRouteDescriptor {
    pub(crate) method: &'static str,
    pub(crate) path: &'static str,
    pub(crate) operation_id: &'static str,
    pub(crate) tag: &'static str,
    pub(crate) tag_description: Option<&'static str>,
    pub(crate) summary: Option<&'static str>,
    pub(crate) description: Option<&'static str>,
    pub(crate) params: &'static [OpenApiParamDescriptor],
    pub(crate) request_description: Option<&'static str>,
    pub(crate) request: Option<OpenApiSchemaDescriptor>,
    pub(crate) response_description: Option<&'static str>,
    pub(crate) response: Option<OpenApiSchemaDescriptor>,
}
impl OpenApiRouteDescriptor {
    /// The method as the spec spells it, lowercase (`get`, `post`, ...).
    pub fn method(&self) -> &'static str {
        self.method
    }

    /// The path this route answers, e.g. `/v1/leaderboard/{id}`.
    pub fn path(&self) -> &'static str {
        self.path
    }

    /// `METHOD /path`, the label an operator reads in the spec.
    pub fn operation(&self) -> String {
        format!("{} {}", self.method.to_ascii_uppercase(), self.path)
    }

    /// Whether the handler's signature told the spec what a success looks like.
    pub fn documents_response(&self) -> bool {
        self.response.is_some()
    }

    /// Whether the published response schema is the free-form JSON object.
    /// A concrete DTO has a name and fields in the spec; the free-form
    /// fallback is only for a genuinely dynamic payload.
    pub fn response_is_free_form(&self) -> bool {
        self.response.is_some_and(|schema| schema.is_free_form())
    }

    /// Whether the handler's signature told the spec what it accepts.
    pub fn documents_request(&self) -> bool {
        self.request.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        method: &'static str,
        path: &'static str,
        operation_id: &'static str,
        tag: &'static str,
        tag_description: Option<&'static str>,
        summary: Option<&'static str>,
        description: Option<&'static str>,
        params: &'static [OpenApiParamDescriptor],
        request_description: Option<&'static str>,
        request: Option<OpenApiSchemaDescriptor>,
        response_description: Option<&'static str>,
        response: Option<OpenApiSchemaDescriptor>,
    ) -> Self {
        Self {
            method,
            path,
            operation_id,
            tag,
            tag_description,
            summary,
            description,
            params,
            request_description,
            request,
            response_description,
            response,
        }
    }
}
inventory::collect!(OpenApiRouteDescriptor);

#[derive(Clone, Copy)]
pub struct OpenApiSchemaDescriptor {
    pub(crate) schema_ref: fn() -> MetaSchemaRef,
    pub(crate) register: fn(&mut Registry),
    pub(crate) content_type: &'static str,
    pub(crate) free_form: bool,
}
impl OpenApiSchemaDescriptor {
    pub const fn json<T: Type>() -> Self {
        Self {
            schema_ref: schema_ref::<T>,
            register: register_type::<T>,
            content_type: "application/json",
            free_form: false,
        }
    }
    /// The book's free-form JSON object: a schema that promises only "a JSON
    /// value". Used when no concrete DTO can name the payload.
    pub const fn free_form_json() -> Self {
        Self {
            schema_ref: schema_ref::<serde_json::Value>,
            register: register_type::<serde_json::Value>,
            content_type: "application/json",
            free_form: true,
        }
    }
    pub const fn is_free_form(&self) -> bool {
        self.free_form
    }
    pub const fn form<T: Type>() -> Self {
        Self {
            schema_ref: schema_ref::<T>,
            register: register_type::<T>,
            content_type: "application/x-www-form-urlencoded",
            free_form: false,
        }
    }
    pub const fn sse() -> Self {
        Self {
            schema_ref: sse_schema_ref,
            register: noop_register,
            content_type: "text/event-stream",
            free_form: false,
        }
    }
    pub const fn sse_json<T: Type>() -> Self {
        Self {
            schema_ref: schema_ref::<T>,
            register: register_type::<T>,
            content_type: "text/event-stream",
            free_form: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct OpenApiParamDescriptor {
    pub(crate) name: &'static str,
    pub(crate) in_type: MetaParamIn,
    pub(crate) schema: OpenApiSchemaDescriptor,
    pub(crate) required: bool,
    pub(crate) deprecated: bool,
    pub(crate) explode: bool,
    pub(crate) style: Option<ParameterStyle>,
}
impl OpenApiParamDescriptor {
    pub const fn path<T: Type>(name: &'static str) -> Self {
        Self {
            name,
            in_type: MetaParamIn::Path,
            schema: OpenApiSchemaDescriptor::json::<T>(),
            required: true,
            deprecated: false,
            explode: false,
            style: None,
        }
    }
    pub const fn query<T: Type>(name: &'static str) -> Self {
        Self {
            name,
            in_type: MetaParamIn::Query,
            schema: OpenApiSchemaDescriptor::json::<T>(),
            required: true,
            deprecated: false,
            explode: true,
            style: Some(ParameterStyle::Form),
        }
    }
}
fn schema_ref<T: Type>() -> MetaSchemaRef {
    T::schema_ref()
}
fn register_type<T: Type>(registry: &mut Registry) {
    T::register(registry);
}
fn noop_register(_: &mut Registry) {}
fn sse_schema_ref() -> MetaSchemaRef {
    let mut schema = poem_openapi::registry::MetaSchema::new_with_format("string", "event-stream");
    schema.description =
        Some("Server-sent events stream. Each message is encoded as text/event-stream frames.");
    schema.example = Some(serde_json::json!(
        "event: message\ndata: {\"type\":\"message\"}\n\n"
    ));
    MetaSchemaRef::Inline(Box::new(schema))
}

/// Every route the derive macros registered, for tooling that audits the spec.
pub fn documented_routes() -> impl IntoIterator<Item = &'static OpenApiRouteDescriptor> {
    inventory::iter::<OpenApiRouteDescriptor>
}
