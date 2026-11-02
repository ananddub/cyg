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
