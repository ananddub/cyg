use syn::spanned::Spanned;
use syn::{Attribute, Ident, ItemFn, LitStr};

use crate::docs::{doc_comment_summary, join_paths, marker_options, merged_docs};
use crate::model::{ModuleRoute, ResponseBody};
use crate::options::ControllerOptions;
use crate::params::infer_params;
use crate::schema::{
    infer_request_body, infer_response_body, route_argument_types, validate_route_function,
};

pub(crate) fn collect_module_route(
    function: &mut ItemFn,
    attribute: Attribute,
    method: Ident,
    controller_options: &ControllerOptions,
    routes: &mut Vec<ModuleRoute>,
) -> syn::Result<()> {
    validate_route_function(function, "module route functions")?;
    let argument_types = route_argument_types(function.sig.inputs.iter())?;
    let route_options = marker_options(&attribute)?;
    let route_path = route_options.path.value();
    routes.push(ModuleRoute {
        method,
        handler: function.sig.ident.clone(),
        path: LitStr::new(
            &join_paths(&controller_options.path.value(), &route_path),
            attribute.span(),
        ),
        docs: merged_docs(&route_options.docs, doc_comment_summary(&function.attrs)),
        request_body: infer_request_body(&argument_types),
        response_body: route_options
            .sse_body
            .map(|ty| ResponseBody::Sse(Some(ty)))
            .or_else(|| infer_response_body(&function.sig.output)),
        params: infer_params(
            &join_paths(&controller_options.path.value(), &route_path),
            &argument_types,
        ),
    });
    Ok(())
}
