use syn::Type;

use crate::model::{OpenApiParam, ParamSource};
use crate::types::wrapper_inner_type;

pub(crate) fn infer_params(path: &str, arguments: &[Type]) -> Vec<OpenApiParam> {
    let mut params = infer_path_params(path, arguments);
    params.extend(arguments.iter().filter_map(|argument| {
        wrapper_inner_type(argument, &["Query"]).map(|ty| OpenApiParam {
            name: "query".to_owned(),
            ty,
            source: ParamSource::Query,
        })
    }));
    params
}

fn infer_path_params(path: &str, arguments: &[Type]) -> Vec<OpenApiParam> {
    let names = path_parameter_names(path);
    let path_types = arguments
        .iter()
        .find_map(|argument| wrapper_inner_type(argument, &["Path"]))
        .map(|ty| path_inner_types(&ty))
        .unwrap_or_default();

    names
        .into_iter()
        .enumerate()
        .map(|(index, name)| OpenApiParam {
            name,
            ty: path_types.get(index).cloned().unwrap_or_else(string_type),
            source: ParamSource::Path,
        })
        .collect()
}

fn path_inner_types(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Tuple(tuple) => tuple.elems.iter().cloned().collect(),
        Type::Paren(paren) => path_inner_types(&paren.elem),
        Type::Reference(reference) => path_inner_types(&reference.elem),
        _ => vec![ty.clone()],
    }
}

pub(crate) fn path_parameter_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            Some(
                segment
                    .strip_prefix('{')?
                    .strip_suffix('}')?
                    .trim_start_matches('*')
                    .to_owned(),
            )
        })
        .collect()
}

fn string_type() -> Type {
    syn::parse_quote!(::std::string::String)
}
