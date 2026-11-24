use syn::spanned::Spanned;
use syn::{ImplItem, LitStr};

use crate::docs::{
    doc_comment_summary, join_paths, marker_options, merged_docs, validate_route_signature,
};
use crate::model::{RequestBody, RequestContent, ResponseBody, Route};
use crate::params::infer_params;
use crate::schema::{infer_request_body, infer_response_body, is_result_type, response_body_type};

mod harvest;
mod live_resolve;
mod module;

pub(crate) use module::collect_module_route;

use harvest::{harvest_attributes, receiver_arguments};
use live_resolve::resolve_live;

pub(crate) fn scan_impl_routes(
    items: &mut [ImplItem],
    controller_path: &str,
) -> syn::Result<Vec<Route>> {
    let mut routes = Vec::new();
    for impl_item in items {
        let ImplItem::Fn(function) = impl_item else {
            continue;
        };
        let mut harvested = harvest_attributes(function)?;

        if harvested.route_attributes.len() > 1 {
            return Err(syn::Error::new(
                function.sig.span(),
                "a controller method can have only one route attribute",
            ));
        }
        let Some((attribute, method)) = harvested.route_attributes.pop() else {
            continue;
        };

        if !harvested.is_live && harvested.live_event.is_some() {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "controller #[on] requires #[live]",
            ));
        }

        validate_route_signature(&function.sig, "controller route methods")?;

        let argument_types = receiver_arguments(&function.sig)?;

        let live_return_type = match &function.sig.output {
            syn::ReturnType::Type(_, ty) if harvested.is_live => match response_body_type(ty) {
                Some(ResponseBody::Json(inner)) => Some(inner),
                _ => Some((**ty).clone()),
            },
            _ => None,
        };
        let live_returns_result = matches!(
            &function.sig.output,
            syn::ReturnType::Type(_, ty) if is_result_type(ty)
        );
        let live_strategy = resolve_live(&function.sig, &mut harvested)?;

        let route_options = marker_options(&attribute)?;
        let route_path = route_options.path.value();
        let full_path = join_paths(controller_path, &route_path);
        let registered_path = crate::docs::without_version_prefix(&full_path);

        let request_body = route_options
            .request_body
            .map(|ty| RequestBody {
                ty,
                content: route_options
                    .request_content
                    .unwrap_or(RequestContent::Json),
            })
            .or_else(|| infer_request_body(&argument_types));
        routes.push(Route {
            method,
            handler: function.sig.ident.clone(),
            path: LitStr::new(&full_path, attribute.span()),
            route_path: LitStr::new(&registered_path, attribute.span()),
            docs: merged_docs(&route_options.docs, doc_comment_summary(&function.attrs)),
            request_body,
            response_body: route_options
                .response_body
                .map(ResponseBody::Json)
                .or_else(|| route_options.sse_body.map(|ty| ResponseBody::Sse(Some(ty))))
                .or_else(|| infer_response_body(&function.sig.output)),
            params: infer_params(&full_path, &argument_types),
            argument_types,
            live_event: harvested.live_event,
            live_client_name: harvested.live_client_name,
            live_table: harvested.live_table,
            live_tables: harvested.live_tables,
            live_strategy,
            live_capacity: harvested.live_capacity,
            live_replay: harvested.live_replay,
            live_return_type,
            live_returns_result,
        });
    }
    Ok(routes)
}
