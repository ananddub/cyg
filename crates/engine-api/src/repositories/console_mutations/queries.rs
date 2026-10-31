use super::*;

impl Queries {
    pub async fn count_console_users(
        &self,
    ) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM console_user))",
                BTreeMap::new(),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn find_console_user_by_id(
        &self,
        id: Uuid,
    ) -> Result<FindConsoleUserByIdRow, DbError> {
        self.one(
            &format!(
                "SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, \
                        mfa_required FROM {}",
                record_uuid("console_user", "id")
            ),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn find_console_user_mfa(
        &self,
        username: &str,
    ) -> Result<FindConsoleUserMfaRow, DbError> {
        self.one(
            "SELECT id, username, email, acl, password, disable_time, mfa_secret, \
                    mfa_recovery_codes, mfa_required FROM console_user \
             WHERE username = $username OR email = $username LIMIT 1",
            params!(username = username),
        )
        .await
    }
    pub async fn find_console_user_mfa_by_id(
        &self,
        id: Uuid,
    ) -> Result<FindConsoleUserMfaByIdRow, DbError> {
        self.one(
            &format!(
                "SELECT id, username, email, acl, password, disable_time, mfa_secret, \
                        mfa_recovery_codes, mfa_required FROM {}",
                record_uuid("console_user", "id")
            ),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn upsert_console_user(
        &self,
        params_in: &UpsertConsoleUserParams,
    ) -> Result<(), DbError> {
        let existing: Option<ConsoleUser> = self
            .opt(
                &format!("SELECT * FROM {}", record_uuid("console_user", "id")),
                params!(id = params_in.id.to_string()),
            )
            .await?;
        if existing.is_some() {
            return self
                .exec(UPSERT_CONSOLE_USER, upsert_console_user_params(params_in))
                .await;
        }

        let now = Utc::now();
        let row = ConsoleUser {
            id: params_in.id,
            username: params_in.username.clone(),
            email: params_in.email.clone(),
            acl: params_in.acl.clone(),
            metadata: serde_json::json!({}),
            password: Some(params_in.password.clone()),
            role: 4,
            create_time: now,
            update_time: now,
            disable_time: time::epoch(),
            mfa_secret: None,
            mfa_recovery_codes: None,
            mfa_required: Some(params_in.mfa_required),
        };
        match self
            .create("console_user", Key::Uuid(params_in.id), &row)
            .await
        {
            Ok(_) => Ok(()),
            Err(DbError::Unique(_)) => {
                self.exec(UPSERT_CONSOLE_USER, upsert_console_user_params(params_in))
                    .await
            }
            Err(err) => Err(err),
        }
    }
    pub async fn insert_console_user(
        &self,
        params_in: &InsertConsoleUserParams,
    ) -> Result<(), DbError> {
        let now = Utc::now();
        let row = ConsoleUser {
            id: params_in.id,
            username: params_in.username.clone(),
            email: params_in.email.clone(),
            acl: params_in.acl.clone(),
            metadata: serde_json::json!({}),
            password: Some(params_in.password.clone()),
            role: 4,
            create_time: now,
            update_time: now,
            disable_time: time::epoch(),
            mfa_secret: None,
            mfa_recovery_codes: None,
            mfa_required: Some(params_in.mfa_required),
        };
        self.create("console_user", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
    pub async fn update_console_user(
        &self,
        params_in: &UpdateConsoleUserParams,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET username = $username, \
                email = $email, acl = $acl, metadata = $metadata, mfa_required = $mfa_required, \
                update_time = $now RETURN AFTER",
            params!(
                username = params_in.username,
                email = params_in.email,
                acl = params_in.acl,
                metadata = params_in.metadata,
                mfa_required = params_in.mfa_required,
                id = params_in.id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn delete_acl_template(
        &self,
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            &format!("DELETE {} RETURN BEFORE", record_uuid("console_acl_template", "id")),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn list_audit_logs_filtered(
        &self,
        params_in: &ListAuditLogsFilteredParams,
    ) -> Result<Vec<ConsoleAuditLog>, DbError> {
        self.all(
            "SELECT create_time, id, console_user_id, console_username, email, action, resource, \
                    message, metadata FROM console_audit_log \
             WHERE ($action = '' OR action = $action) \
               AND ($resource = '' OR resource = $resource) \
               AND ($console_username = '' OR console_username = $console_username) \
               AND create_time >= $from AND create_time <= $to \
             ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(
                action = params_in.action,
                resource = params_in.resource,
                console_username = params_in.console_username,
                from = time::encode(params_in.create_time_start),
                to = time::encode(params_in.create_time_end),
                limit = params_in.limit,
                offset = params_in.offset,
            ),
        )
        .await
    }
    pub async fn insert_audit_log(
        &self,
        params_in: &InsertAuditLogParams,
    ) -> Result<(), DbError> {
        let row = ConsoleAuditLog {
            id: params_in.id,
            console_user_id: params_in.console_user_id,
            console_username: params_in.console_username.clone(),
            email: params_in.email.clone(),
            action: params_in.action.clone(),
            resource: params_in.resource.clone(),
            message: params_in.message.clone(),
            metadata: params_in.metadata.clone(),
            create_time: Utc::now(),
        };
        self.create("console_audit_log", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
}

impl Queries {
    pub async fn delete_user_note(
        &self,
        id: Uuid,
        user_id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE users_notes WHERE id = type::record('users_notes', <uuid>$id) \
                AND user_id = $user_id RETURN BEFORE",
            params!(id = id.to_string(), user_id = user_id.to_string()),
        )
        .await
    }
    pub async fn delete_storage_collection(
        &self,
        collection: &str,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE storage WHERE collection = $collection RETURN BEFORE",
            params!(collection = collection),
        )
        .await
    }
    pub async fn list_storage_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ListStorageForUserRow>, DbError> {
        let rows: Vec<StoredObject> = self
            .all(
                "SELECT id, collection, key, user_id, value, version, read, write, create_time, \
                        update_time FROM storage WHERE user_id = $user_id \
                 ORDER BY collection ASC, id ASC",
                params!(user_id = user_id.to_string()),
            )
            .await?;
        Ok(rows.into_iter().map(ListStorageRow::from).collect())
    }
    pub async fn list_user_edges(
        &self,
        source_id: Uuid,
    ) -> Result<Vec<ListUserEdgesRow>, DbError> {
        self.all(
            "SELECT destination_id, state, position, metadata FROM user_edge \
             WHERE source_id = $source_id ORDER BY state ASC, position ASC",
            params!(source_id = source_id.to_string()),
        )
        .await
    }
    pub async fn insert_notification(
        &self,
        params_in: &InsertNotificationParams,
    ) -> Result<(), DbError> {
        let row = Notification {
            id: params_in.id,
            user_id: params_in.user_id,
            subject: params_in.subject.clone(),
            content: params_in.content.clone(),
            code: params_in.code,
            sender_id: params_in.sender_id,
            create_time: Utc::now(),
        };
        self.create("notification", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
    pub async fn upsert_setting(
        &self,
        name: &str,
        value: serde_json::Value,
    ) -> Result<(), DbError> {
        let row = Setting {
            name: name.to_string(),
            value,
            update_time: Utc::now(),
        };
        self.exec(
            "DELETE setting WHERE name = $name; \
             CREATE type::record('setting', $name) CONTENT $data RETURN AFTER;",
            params!(name = name, data = row),
        )
        .await
    }
    pub async fn delete_setting(
        &self,
        name: &str,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('setting', $name) RETURN BEFORE",
            params!(name = name),
        )
        .await
    }
    pub async fn insert_acl_template(
        &self,
        params_in: &InsertAclTemplateParams,
    ) -> Result<(), DbError> {
        let now = Utc::now();
        let row = ConsoleAclTemplate {
            id: params_in.id,
            name: params_in.name.clone(),
            description: params_in.description.clone(),
            acl: params_in.acl.clone(),
            create_time: now,
            update_time: now,
        };
        self.create("console_acl_template", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
    pub async fn update_acl_template(
        &self,
        params_in: &UpdateAclTemplateParams,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_acl_template', <uuid>$id) SET name = $name, \
                description = $description, acl = $acl, update_time = $now RETURN AFTER",
            params!(
                name = params_in.name,
                description = params_in.description,
                acl = params_in.acl,
                id = params_in.id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn find_acl_template(
        &self,
        id: Uuid,
    ) -> Result<ConsoleAclTemplate, DbError> {
        self.one(
            &format!(
                "SELECT id, name, description, acl, create_time, update_time FROM {}",
                record_uuid("console_acl_template", "id")
            ),
            params!(id = id.to_string()),
        )
        .await
    }
}

impl Queries {
    pub async fn update_console_user_password(
        &self,
        d: &[u8],
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('console_user', <uuid>$id) SET password = $password, \
                update_time = $now",
            params!(
                password = bytes::encode(d),
                id = id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn set_console_user_disabled(
        &self,
        disable_time: DateTime<Utc>,
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET disable_time = $disable_time, \
                update_time = $now RETURN AFTER",
            params!(
                disable_time = time::encode(disable_time),
                id = id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn set_console_user_mfa(
        &self,
        params_in: &SetConsoleUserMfaParams,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET mfa_secret = $mfa_secret, \
                mfa_recovery_codes = $mfa_recovery_codes, mfa_required = $mfa_required, \
                update_time = $now RETURN AFTER",
            params!(
                mfa_secret = bytes::encode(&params_in.mfa_secret),
                mfa_recovery_codes = bytes::encode(&params_in.mfa_recovery_codes),
                mfa_required = params_in.mfa_required,
                id = params_in.id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn clear_console_user_mfa(
        &self,
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET mfa_secret = NULL, \
                mfa_recovery_codes = NULL, mfa_required = false, update_time = $now RETURN AFTER",
            params!(id = id.to_string(), now = time::encode(Utc::now())),
        )
        .await
    }
    pub async fn set_console_user_mfa_required(
        &self,
        id: Uuid,
        required: bool,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET mfa_required = $required, \
                update_time = $now RETURN AFTER",
            params!(
                required = required,
                id = id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn reset_console_user_mfa_factor(
        &self,
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET mfa_secret = NULL, \
                mfa_recovery_codes = NULL, update_time = $now RETURN AFTER",
            params!(id = id.to_string(), now = time::encode(Utc::now())),
        )
        .await
    }
    pub async fn import_account_bundle(
        &self,
        params_in: &ImportAccountParams,
    ) -> Result<(), DbError> {
        let account = &params_in.account;
        let now = time::encode(Utc::now());

        let devices: Vec<Value> = params_in
            .devices
            .iter()
            .map(|device| {
                json!({
                    "id": device.id,
                    "data": {
                        "user_id": account.id.to_string(),
                        "preferences": {},
                        "push_token_amazon": "",
                        "push_token_android": "",
                        "push_token_huawei": "",
                        "push_token_ios": "",
                        "push_token_web": "",
                        "provider": device.provider,
                    },
                })
            })
            .collect();

        let notes: Vec<Value> = params_in
            .notes
            .iter()
            .map(|note| {
                json!({
                    "id": note.id.to_string(),
                    "data": {
                        "user_id": account.id.to_string(),
                        "note": note.note,
                        "create_time": time::encode(note.create_time),
                        "update_time": time::encode(note.update_time),
                    },
                })
            })
            .collect();

        let ledger: Vec<Value> = params_in
            .wallet_ledger
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id.to_string(),
                    "data": {
                        "user_id": account.id.to_string(),
                        "changeset": entry.changeset,
                        "metadata": entry.metadata,
                        "create_time": time::encode(entry.create_time),
                        "update_time": time::encode(entry.update_time),
                    },
                })
            })
            .collect();

        let storage: Vec<Value> = params_in
            .storage
            .iter()
            .map(|object| {
                let value = serde_json::from_str(&object.value)
                    .unwrap_or_else(|_| Value::String(object.value.clone()));
                json!({
                    "collection": object.collection,
                    "key": object.object_key,
                    "user_id": account.id.to_string(),
                    "value": value,
                    "version": object.version,
                    "read": object.read,
                    "write": object.write,
                    "create_time": time::encode(object.create_time),
                    "update_time": time::encode(object.update_time),
                })
            })
            .collect();

        let edges: Vec<Value> = params_in
            .edges
            .iter()
            .map(|edge| {
                json!({
                    "source_id": account.id.to_string(),
                    "destination_id": edge.destination_id.to_string(),
                    "state": edge.state,
                    "position": edge.position,
                    "metadata": edge.metadata,
                    "update_time": now,
                })
            })
            .collect();

        let mut account = serde_json::to_value(account)?;
        if let Value::Object(map) = &mut account {
            map.remove("id");
        }

        let surql = "\
            CREATE type::record('users', <uuid>$id) CONTENT $account; \
            FOR $device IN $devices { \
                CREATE type::record('user_device', $device.id) CONTENT $device.data; \
            }; \
            FOR $note IN $notes { \
                CREATE type::record('users_notes', <uuid>$note.id) CONTENT $note.data; \
            }; \
            FOR $entry IN $ledger { \
                CREATE type::record('wallet_ledger', <uuid>$entry.id) CONTENT $entry.data; \
            }; \
            FOR $object IN $storage { \
                CREATE storage CONTENT $object; \
            }; \
            FOR $edge IN $edges { \
                IF array::len((SELECT VALUE id FROM user_edge \
                    WHERE source_id = $edge.source_id \
                      AND destination_id = $edge.destination_id)) = 0 { \
                    CREATE user_edge CONTENT $edge; \
                }; \
            };";

        let binds = BTreeMap::from([
            ("id".to_string(), json!(params_in.account.id.to_string())),
            ("account".to_string(), account),
            ("devices".to_string(), Value::Array(devices)),
            ("notes".to_string(), Value::Array(notes)),
            ("ledger".to_string(), Value::Array(ledger)),
            ("storage".to_string(), Value::Array(storage)),
            ("edges".to_string(), Value::Array(edges)),
        ]);
        self.exec(surql, binds).await
    }
    pub async fn set_console_user_recovery_codes(
        &self,
        mfa_recovery_codes: &[u8],
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('console_user', <uuid>$id) SET mfa_recovery_codes = $codes, \
                update_time = $now RETURN AFTER",
            params!(
                codes = bytes::encode(mfa_recovery_codes),
                id = id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn delete_console_user(
        &self,
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            &format!("DELETE {}", record_uuid("console_user", "id")),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn set_account_disabled(
        &self,
        disable_time: DateTime<Utc>,
        id: Uuid,
    ) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE type::record('users', <uuid>$id) SET disable_time = $disable_time, \
                update_time = $now RETURN AFTER",
            params!(
                disable_time = time::encode(disable_time),
                id = id.to_string(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
    pub async fn insert_user_note(
        &self,
        params_in: &InsertUserNoteParams,
    ) -> Result<(), DbError> {
        let now = Utc::now();
        let row = UsersNotes {
            id: params_in.id,
            user_id: params_in.user_id,
            create_time: now,
            update_time: now,
            note: params_in.note.clone(),
            create_id: Some(params_in.create_id),
            update_id: None,
        };
        self.create("users_notes", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
    pub async fn insert_user_note_anonymous(
        &self,
        params_in: &InsertUserNoteAnonymousParams,
    ) -> Result<(), DbError> {
        let now = Utc::now();
        let row = UsersNotes {
            id: params_in.id,
            user_id: params_in.user_id,
            create_time: now,
            update_time: now,
            note: params_in.note.clone(),
            create_id: None,
            update_id: None,
        };
        self.create("users_notes", Key::Uuid(params_in.id), &row)
            .await?;
        Ok(())
    }
}
