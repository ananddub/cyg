#[allow(dead_code)]
pub(crate) fn unity_subpath(path: &str) -> Option<&str> {
    if let Some(sub) = path.strip_prefix("/unity/") {
        return Some(sub);
    }
    if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
        return Some(&path[1..]);
    }
    None
}
