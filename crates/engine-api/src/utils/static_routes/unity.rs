/// Which Unity build-relative path a matched URL asks for, or `None` when the
/// URL is not one of the three Unity prefixes.
///
/// `/unity/…` drops its prefix; `/Build/…` and `/TemplateData/…` keep theirs,
/// since the export already lays those out as directories under the build folder.
pub(crate) fn unity_subpath(path: &str) -> Option<&str> {
    if let Some(sub) = path.strip_prefix("/unity/") {
        return Some(sub);
    }
    if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
        // Skip the leading slash only; both prefixes are already the directory
        // names Unity wrote into `index.html`.
        return Some(&path[1..]);
    }
    None
}
