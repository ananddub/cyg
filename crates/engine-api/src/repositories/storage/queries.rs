use super::*;

const INSERT_INDEX: &str = "
CREATE type::record('storage_index', $name) CONTENT {
    name: $name, collection: $collection, `key`: $key,
    fields: $fields, sortable_fields: $sortable_fields,
    max_entries: $max_entries, index_only: $index_only,
    user_id: $user_id, create_time: $create_time
} RETURN AFTER";

const GET_INDEX: &str = "
SELECT name, collection, `key`, fields, sortable_fields, max_entries, index_only, user_id
FROM type::record('storage_index', $name) LIMIT 1";

const LIST_INDEXES: &str = "
SELECT name, collection, `key`, fields, sortable_fields, max_entries, index_only, user_id
FROM storage_index ORDER BY name ASC";

const DELETE_INDEX: &str = "DELETE type::record('storage_index', $name)";

const INDEX_READ_HEAD: &str = "SELECT collection, `key`, user_id, ";
const INDEX_READ_TAIL: &str = ", `version`, read, write, create_time, update_time";

impl Queries {
    pub async fn storage_objects(
        &self,
        caller: Uuid,
        wanted: &[(String, String, Uuid)],
    ) -> Result<Vec<StorageRow>, DbError> {
        let mut params =
            BTreeMap::<String, Value>::from([("caller".to_string(), json!(caller.to_string()))]);
        let mut clause = String::new();
        for (index, (collection, key, owner)) in wanted.iter().enumerate() {
            if index > 0 {
                clause.push_str(" OR ");
            }
            clause.push_str(&format!(
                "(collection = $c{index} AND `key` = $k{index} \
                  AND (user_id = $u{index} OR user_id = $u{index}))"
            ));
            params.insert(format!("c{index}"), json!(collection));
            params.insert(format!("k{index}"), json!(key));
            params.insert(format!("u{index}"), json!(owner.to_string()));
        }
        let surql = format!(
            "SELECT {STORAGE_COLUMNS} FROM storage \
             WHERE (read = 2 OR (read = 1 AND (user_id = $caller OR user_id = $caller))) \
               AND ({clause})"
        );
        self.all(&surql, params).await
    }

    pub async fn storage_object(
        &self,
        caller: Uuid,
        collection: &str,
        key: &str,
        owner: Uuid,
        version: Option<&str>,
    ) -> Result<Option<StorageRow>, DbError> {
        let mut params = BTreeMap::from([
            ("collection".to_string(), json!(collection)),
            ("key".to_string(), json!(key)),
            ("user_id".to_string(), json!(owner.to_string())),
            ("caller".to_string(), json!(caller.to_string())),
        ]);
        let versioned = match version {
            Some(version) => {
                params.insert("version".to_string(), json!(version));
                " AND `version` = $version"
            }
            None => "",
        };
        let surql = format!(
            "SELECT {STORAGE_COLUMNS} FROM storage \
             WHERE collection = $collection AND `key` = $key \
               AND (user_id = $user_id OR user_id = $user_id) \
               AND (read = 2 OR (read = 1 AND (user_id = $caller OR user_id = $caller))){versioned} \
             LIMIT 1"
        );
        self.opt(&surql, params).await
    }

    pub async fn storage_current(
        &self,
        collection: &str,
        key: &str,
        owner: Uuid,
    ) -> Result<Option<StorageCurrentRow>, DbError> {
        self.opt(
            "SELECT read, write, `version`, create_time, update_time FROM storage \
             WHERE collection = $collection AND `key` = $key \
               AND (user_id = $user_id OR user_id = $user_id) LIMIT 1",
            params!(
                collection = collection,
                key = key,
                user_id = owner.to_string(),
            ),
        )
        .await
    }

    pub async fn storage_deletable(
        &self,
        owner: Uuid,
        ids: &[ObjectId],
    ) -> Result<Vec<StorageDeleteRow>, DbError> {
        let (surql, params) = delete_lookup(owner, ids);
        self.all(&surql, params).await
    }

    pub async fn storage_delete(&self, owner: Uuid, ids: &[ObjectId]) -> Result<(), DbError> {
        let (surql, params) = delete_statements(owner, ids);
        self.exec(&surql, params).await
    }

    pub async fn storage_delete_object(
        &self,
        collection: &str,
        key: &str,
        user_id: Uuid,
    ) -> Result<bool, DbError> {
        let deleted: Option<StorageRow> = self
            .opt(
                "DELETE storage WHERE collection = $collection AND key = $key \
                    AND user_id = $user_id RETURN BEFORE",
                params!(
                    collection = collection,
                    key = key,
                    user_id = user_id.to_string(),
                ),
            )
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn storage_exec(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<(), DbError> {
        self.exec(surql, params).await
    }

    pub async fn storage_listing(
        &self,
        listing: StorageListing,
        collection: &str,
        owner: Option<Uuid>,
        after: Option<&StorageCursor>,
        limit: i64,
    ) -> Result<Vec<StorageRow>, DbError> {
        let page = limit + 1;
        let mut params = BTreeMap::<String, Value>::from([
            ("collection".to_string(), json!(collection)),
            ("limit".to_string(), json!(page)),
        ]);
        if let Some(owner) = owner {
            params.insert("user".to_string(), json!(owner.to_string()));
        }
        let surql = match (listing, after) {
            (StorageListing::PublicAll, None) => public_all(true),
            (StorageListing::PublicAll, Some(c)) => {
                params.insert("key".to_string(), json!(c.key));
                params.insert("user".to_string(), json!(c.user_id.to_string()));
                public_all(false)
            }
            (StorageListing::Own, None) => own(true),
            (StorageListing::Own, Some(c)) => {
                params.insert("read".to_string(), json!(c.read));
                params.insert("key".to_string(), json!(c.key));
                own(false)
            }
            (StorageListing::PublicUser, None) => public_user(true),
            (StorageListing::PublicUser, Some(c)) => {
                params.insert("key".to_string(), json!(c.key));
                public_user(false)
            }
        };
        self.all(&surql, params).await
    }

    pub async fn storage_index(&self, name: &str) -> Result<Option<StorageIndexRow>, DbError> {
        self.opt(GET_INDEX, params!(name = name)).await
    }

    pub async fn storage_indexes(&self) -> Result<Vec<StorageIndexRow>, DbError> {
        self.all(LIST_INDEXES, BTreeMap::new()).await
    }

    pub async fn storage_insert_index(
        &self,
        content: &StorageIndexContent,
    ) -> Result<Option<StorageIndexRow>, DbError> {
        let row = self
            .opt(
                INSERT_INDEX,
                params!(
                    name = content.name,
                    collection = content.collection,
                    key = content.key,
                    fields = json!(content.fields),
                    sortable_fields = json!(content.sortable_fields),
                    max_entries = content.max_entries,
                    index_only = content.index_only,
                    user_id = content.user_id.to_string(),
                    create_time = time::encode(content.create_time),
                ),
            )
            .await?;
        Ok(row)
    }

    pub async fn storage_delete_index(&self, name: &str) -> Result<(), DbError> {
        self.exec(DELETE_INDEX, params!(name = name)).await
    }

    pub async fn storage_index_page(
        &self,
        def: &StorageIndexRow,
        terms: &[StorageIndexTerm],
        sorts: &[(StorageIndexSort, bool)],
        caller: Uuid,
        page: i64,
        offset: i64,
    ) -> Result<Vec<StorageRow>, DbError> {
        let mut params = BTreeMap::new();
        let surql = index_query(def, terms, sorts, caller, page, offset, &mut params);
        self.all(&surql, params).await
    }
}

pub(crate) fn storage_insert_statement(
    index: usize,
    object: &WriteObject,
    owner: Uuid,
    params: &mut BTreeMap<String, Value>,
) -> String {
    let name = format!("o{index}");
    let now = time::encode(Utc::now());
    params.insert(
        name.clone(),
        json!({
            "collection": object.collection,
            "key": object.key,
            "user_id": owner.to_string(),
            "value": serde_json::from_str::<Value>(&object.value)
                .expect("check_write validated the value"),
            "version": version_of(&object.value),
            "read": object.permission_read.unwrap_or(PERMIT_OWNER as i32),
            "write": object.permission_write.unwrap_or(PERMIT_OWNER as i32),
            "create_time": now,
            "update_time": now,
        }),
    );
    format!("CREATE storage CONTENT ${name} RETURN NOTHING")
}

pub(crate) fn storage_update_statement(
    index: usize,
    object: &WriteObject,
    version: &str,
    params: &mut BTreeMap<String, Value>,
) -> String {
    let (c, k, v, ver, r, w, ut) = (
        format!("c{index}"),
        format!("k{index}"),
        format!("v{index}"),
        format!("ver{index}"),
        format!("r{index}"),
        format!("w{index}"),
        format!("ut{index}"),
    );
    params.insert(c.clone(), json!(object.collection));
    params.insert(k.clone(), json!(object.key));
    params.insert(
        v.clone(),
        serde_json::from_str::<Value>(&object.value).expect("check_write validated the value"),
    );
    params.insert(ver.clone(), json!(version));
    params.insert(
        r.clone(),
        json!(object.permission_read.unwrap_or(PERMIT_OWNER as i32)),
    );
    params.insert(
        w.clone(),
        json!(object.permission_write.unwrap_or(PERMIT_OWNER as i32)),
    );
    params.insert(ut.clone(), json!(time::encode(Utc::now())));
    format!(
        "UPDATE storage SET `value` = ${v}, `version` = ${ver}, read = ${r}, write = ${w}, \
         update_time = ${ut} \
         WHERE collection = ${c} AND `key` = ${k} \
           AND (user_id = $owner OR user_id = $owner) RETURN NOTHING"
    )
}

pub(crate) fn storage_sort_column(field: &str) -> Option<&'static str> {
    match field {
        "collection" => Some("collection"),
        "key" => Some("`key`"),
        "user_id" => Some("user_id"),
        "version" => Some("`version`"),
        "read" => Some("read"),
        "write" => Some("write"),
        "create_time" => Some("create_time"),
        "update_time" => Some("update_time"),
        _ => None,
    }
}

fn delete_lookup(owner: Uuid, ids: &[ObjectId]) -> (String, BTreeMap<String, Value>) {
    let mut params =
        BTreeMap::<String, Value>::from([("owner".to_string(), json!(owner.to_string()))]);
    let mut wanted = String::new();
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            wanted.push_str(" OR ");
        }
        wanted.push_str(&format!("(collection = $c{index} AND `key` = $k{index})"));
        params.insert(format!("c{index}"), json!(id.collection));
        params.insert(format!("k{index}"), json!(id.key));
    }
    let surql = format!(
        "SELECT collection, `key`, `version`, write FROM storage \
         WHERE (user_id = $owner OR user_id = $owner) AND ({wanted})"
    );
    (surql, params)
}

fn delete_statements(owner: Uuid, ids: &[ObjectId]) -> (String, BTreeMap<String, Value>) {
    let mut params =
        BTreeMap::<String, Value>::from([("owner".to_string(), json!(owner.to_string()))]);
    let mut statements: Vec<String> = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let mut statement = format!(
            "DELETE storage WHERE collection = $c{index} AND `key` = $k{index} \
             AND (user_id = $owner OR user_id = $owner) AND write > 0"
        );
        params.insert(format!("c{index}"), json!(id.collection));
        params.insert(format!("k{index}"), json!(id.key));
        if !id.version.is_empty() {
            let name = format!("v{index}");
            params.insert(name.clone(), json!(id.version));
            statement.push_str(&format!(" AND `version` = ${name}"));
        }
        statements.push(statement);
    }
    (statements.join(";\n"), params)
}

fn public_all(first: bool) -> String {
    let cursor = if first {
        String::new()
    } else {
        " AND (read > 2 OR (read = 2 AND (`key` > $key OR (`key` = $key AND user_id > $user))))"
            .to_string()
    };
    format!(
        "SELECT {STORAGE_COLUMNS} FROM storage \
         WHERE collection = $collection AND read >= 2{cursor} \
         ORDER BY read ASC, `key` ASC, user_id ASC LIMIT $limit"
    )
}

fn own(first: bool) -> String {
    let cursor = if first {
        String::new()
    } else {
        " AND (read > $read OR (read = $read AND `key` > $key))".to_string()
    };
    format!(
        "SELECT {STORAGE_COLUMNS} FROM storage \
         WHERE collection = $collection \
           AND (user_id = $user OR user_id = $user) AND read >= 1{cursor} \
         ORDER BY read ASC, `key` ASC LIMIT $limit"
    )
}

fn public_user(first: bool) -> String {
    let cursor = if first {
        String::new()
    } else {
        " AND `key` > $key".to_string()
    };
    format!(
        "SELECT {STORAGE_COLUMNS} FROM storage \
         WHERE collection = $collection \
           AND (user_id = $user OR user_id = $user) AND read = 2{cursor} \
         ORDER BY `key` ASC LIMIT $limit"
    )
}

fn index_query(
    def: &StorageIndexRow,
    terms: &[StorageIndexTerm],
    sorts: &[(StorageIndexSort, bool)],
    caller: Uuid,
    page: i64,
    offset: i64,
    params: &mut BTreeMap<String, Value>,
) -> String {
    let mut projection = String::from(INDEX_READ_HEAD);
    if def.index_only {
        let picked: Vec<String> = StorageIndexRow::strings(&def.fields)
            .into_iter()
            .filter(|field| {
                !field.is_empty()
                    && field
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            })
            .map(|field| {
                let access = bind(&field, params);
                format!("'{field}': `value`[{access}]")
            })
            .collect();
        projection.push_str(&format!("{{ {} }} AS `value`", picked.join(", ")));
    } else {
        projection.push_str("`value`");
    }
    projection.push_str(INDEX_READ_TAIL);

    let collection = bind(&def.collection, params);
    let owner = bind(&caller.to_string(), params);
    let mut filters = format!(
        "collection = {collection} AND (read = 2 OR (read = 1 AND (user_id = <uuid>{owner} OR user_id = {owner})))"
    );
    if !def.key.is_empty() {
        let key = bind(&def.key, params);
        filters.push_str(&format!(" AND `key` = {key}"));
    }
    for term in terms {
        filters.push_str(" AND (");
        term_sql(term, params, &mut filters);
        filters.push(')');
    }

    let mut order = Vec::new();
    let mut computed = Vec::new();
    for (sort, descending) in sorts {
        let direction = if *descending { " DESC" } else { " ASC" };
        match sort {
            StorageIndexSort::Column(column) => order.push(format!("{column}{direction}")),
            StorageIndexSort::ValueField(field) => {
                let access = bind(field, params);
                let alias = format!("s{}", computed.len());
                computed.push(format!(
                    "IF type::is_number(`value`[{access}]) OR type::is_string(`value`[{access}]) \
                     OR type::is_bool(`value`[{access}]) THEN `value`[{access}] ELSE NONE END \
                     AS {alias}"
                ));
                order.push(format!("{alias}{direction}"));
            }
        }
    }
    order.push("collection ASC".to_string());
    order.push("`key` ASC".to_string());
    order.push("user_id ASC".to_string());

    let limit = bind(&page, params);
    let start = bind(&offset, params);
    let tail = format!(" ORDER BY {} LIMIT {limit} START {start}", order.join(", "));
    if computed.is_empty() {
        format!(
            "{} FROM storage WHERE {filters}{tail}",
            projection.trim_start()
        )
    } else {
        format!(
            "{} FROM (SELECT *, {} FROM storage WHERE {filters}){tail}",
            projection.trim_start(),
            computed.join(", ")
        )
    }
}

fn term_sql(term: &StorageIndexTerm, params: &mut BTreeMap<String, Value>, sql: &mut String) {
    match term {
        StorageIndexTerm::Eq {
            field,
            text,
            number,
        } => {
            let access = bind(field, params);
            let value = bind(text, params);
            sql.push_str(&format!("`value`[{access}] = {value}"));
            if let Some(parsed) = number.as_deref().and_then(|n| n.parse::<f64>().ok()) {
                let number = bind(&parsed, params);
                sql.push_str(&format!(
                    " OR (type::is_number(`value`[{access}]) AND `value`[{access}] = {number})"
                ));
            }
        }
        StorageIndexTerm::Range {
            field,
            lower,
            upper,
        } => {
            let bounds: Vec<(&StorageIndexBound, bool)> = lower
                .iter()
                .map(|b| (b, true))
                .chain(upper.iter().map(|b| (b, false)))
                .collect();
            let access = bind(field, params);
            let numeric = bounds.iter().all(|(b, _)| b.number.is_some());
            let guard = if numeric { "is_number" } else { "is_string" };
            sql.push_str(&format!("type::{guard}(`value`[{access}])"));
            for (bound, lower_side) in bounds {
                let value = if numeric {
                    bind(
                        &bound.number.as_deref().and_then(|n| n.parse::<f64>().ok()),
                        params,
                    )
                } else {
                    bind(&bound.text, params)
                };
                let op = operator(bound, lower_side);
                sql.push_str(&format!(" AND `value`[{access}]{op}{value}"));
            }
        }
    }
}

fn bind<T: serde::Serialize>(value: T, params: &mut BTreeMap<String, Value>) -> String {
    let name = format!("p{}", params.len());
    params.insert(
        name.clone(),
        serde_json::to_value(value).expect("a bindable index parameter"),
    );
    format!("${name}")
}

fn operator(bound: &StorageIndexBound, lower: bool) -> &'static str {
    match (lower, bound.inclusive) {
        (true, true) => " >= ",
        (true, false) => " > ",
        (false, true) => " <= ",
        (false, false) => " < ",
    }
}
