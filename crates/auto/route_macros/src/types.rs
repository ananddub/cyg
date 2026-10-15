use syn::{GenericArgument, PathArguments, Type};

pub(crate) fn wrapper_inner_type(ty: &Type, wrappers: &[&str]) -> Option<Type> {
    match ty {
        Type::Paren(paren) => wrapper_inner_type(&paren.elem, wrappers),
        Type::Reference(reference) => wrapper_inner_type(&reference.elem, wrappers),
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last()?;
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return None;
            };
            if segment.ident == "Option" {
                return arguments.args.iter().find_map(|argument| match argument {
                    GenericArgument::Type(ty) => wrapper_inner_type(ty, wrappers),
                    _ => None,
                });
            }
            if !wrappers.iter().any(|wrapper| segment.ident == wrapper) {
                return None;
            }
            arguments.args.iter().find_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty.clone()),
                _ => None,
            })
        }
        _ => None,
    }
}
