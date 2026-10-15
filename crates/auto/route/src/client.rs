#[doc(hidden)]
pub struct LiveClientRouteDescriptor {
    pub client_name: &'static str,
    pub namespace: &'static str,
    pub endpoint: &'static str,
    pub event: &'static str,
    pub path: &'static str,
    pub arguments: &'static [LiveClientArgumentDescriptor],
    pub result_name: fn() -> String,
    pub result_decl: fn() -> String,
}

impl LiveClientRouteDescriptor {
    pub const fn new(
        client_name: &'static str,
        namespace: &'static str,
        endpoint: &'static str,
        event: &'static str,
        path: &'static str,
        arguments: &'static [LiveClientArgumentDescriptor],
        result_name: fn() -> String,
        result_decl: fn() -> String,
    ) -> Self {
        Self {
            client_name,
            namespace,
            endpoint,
            event,
            path,
            arguments,
            result_name,
            result_decl,
        }
    }
}

inventory::collect!(LiveClientRouteDescriptor);

#[doc(hidden)]
#[derive(Clone, Copy, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveClientArgumentDescriptor {
    Path {
        index: usize,
        names: &'static [&'static str],
        #[serde(skip)]
        type_name: fn() -> String,
    },
    Query {
        index: usize,
        #[serde(skip)]
        type_name: fn() -> String,
    },
}

pub fn live_client_manifest() -> serde_json::Value {
    let mut types = std::collections::BTreeMap::new();
    let endpoints = inventory::iter::<LiveClientRouteDescriptor>
        .into_iter()
        .map(|item| {
            let group = controller_name(item.endpoint);
            let group_path = client_group_path(&group);
            let member = format!("use{}", pascal_case(item.client_name));
            let result = (item.result_name)();
            let declaration = (item.result_decl)();
            if !declaration.is_empty() {
                types.insert(result.clone(), declaration);
            }
            let parameters = item
                .arguments
                .iter()
                .map(|argument| match argument {
                    LiveClientArgumentDescriptor::Path {
                        names, type_name, ..
                    } => {
                        format!(
                            "{}: {}",
                            names.first().copied().unwrap_or("arg"),
                            type_name()
                        )
                    }
                    LiveClientArgumentDescriptor::Query { type_name, .. } => {
                        format!("query: {}", type_name())
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            serde_json::json!({
                "hook": format!("use{group}{}", pascal_case(item.client_name)),
            "group": group,
            "groupPath": group_path,
            "member": member,
                "namespace": item.namespace,
                "endpoint": item.endpoint,
                "event": item.event,
                "path": item.path,
                "arguments": item.arguments,
                "parameters": parameters,
                "result": result,
            })
        })
        .collect::<Vec<_>>();
    let types = types
        .into_iter()
        .map(|(name, definition)| serde_json::json!({"name": name, "definition": definition}))
        .collect::<Vec<_>>();
    serde_json::json!({"types": types, "endpoints": endpoints})
}

fn client_group_path(group: &str) -> Vec<String> {
    let database_member = match group {
        "Postgres" | "Mysql" | "Mariadb" | "Mongo" | "Redis" | "Libsql" => Some(group),
        "DatabaseManagement" => Some("Management"),
        "DatabaseNetwork" => Some("Network"),
        _ => None,
    };

    database_member
        .map(|member| vec!["Database".to_owned(), member.to_owned()])
        .unwrap_or_else(|| vec![group.to_owned()])
}

#[doc(hidden)]
pub fn ts_name<T: ts_rs::TS>() -> String {
    let cfg = ts_rs::Config::default();
    let name = T::name(&cfg);
    if name.starts_with('[') {
        return name;
    }
    T::inline(&cfg)
}
#[doc(hidden)]
pub fn ts_decl<T: ts_rs::TS>() -> String {
    String::new()
}

fn controller_name(endpoint: &str) -> String {
    endpoint
        .split("::")
        .next()
        .map(|name| name.strip_suffix("Controller").unwrap_or(name))
        .map(pascal_case)
        .unwrap_or_default()
}

fn pascal_case(value: &str) -> String {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}
