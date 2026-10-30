use super::*;

impl BlobStore {
    pub async fn open(
        config: &str,
        endpoint: &str,
        db: Option<Arc<Datastore>>,
    ) -> Result<Self, BlobError> {
        let config = config.trim();
        if let Some(path) = config.strip_prefix("file:") {
            return Ok(Self::Dir {
                root: dir_root(path)?,
            });
        }
        if !config.is_empty() {
            let db = db.ok_or(BlobError::Unconfigured)?;
            return Self::bucket(db, config).await;
        }
        if endpoint.starts_with("mem://") {
            return Self::bucket(db.ok_or(BlobError::Unconfigured)?, "memory://").await;
        }
        if is_embedded(endpoint) {
            return Ok(Self::Dir {
                root: PathBuf::from(DEFAULT_DIR),
            });
        }
        Err(BlobError::Unconfigured)
    }

    async fn bucket(db: Arc<Datastore>, backend: &str) -> Result<Self, BlobError> {
        db.exec_params(
            &format!("DEFINE BUCKET OVERWRITE {BUCKET} BACKEND $backend"),
            &std::collections::BTreeMap::from([(
                "backend".to_string(),
                serde_json::json!(backend),
            )]),
        )
        .await?;
        Ok(Self::Bucket {
            name: BUCKET.to_string(),
            db,
        })
    }

    pub async fn put(&self, key: &str, value: &[u8]) -> Result<(), BlobError> {
        check_key(key)?;
        match self {
            BlobStore::Dir { root } => {
                let path = root.join(key);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|err| BlobError::Io(format!("{}: {err}", parent.display())))?;
                }
                let tmp = temp_path(&path);
                tokio::fs::write(&tmp, value)
                    .await
                    .map_err(|err| BlobError::Io(format!("{}: {err}", tmp.display())))?;
                if let Err(err) = tokio::fs::rename(&tmp, &path).await {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    return Err(BlobError::Io(format!("{}: {err}", path.display())));
                }
                Ok(())
            }
            BlobStore::Bucket { name, db } => {
                db.exec_params(
                    "RETURN file::put(type::file($bucket, $key), encoding::base64::decode($data));",
                    &std::collections::BTreeMap::from([
                        ("bucket".to_string(), serde_json::json!(name)),
                        ("key".to_string(), serde_json::json!(key)),
                        ("data".to_string(), serde_json::json!(bytes::encode(value))),
                    ]),
                )
                .await?;
                Ok(())
            }
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, BlobError> {
        check_key(key)?;
        match self {
            BlobStore::Dir { root } => {
                let path = root.join(key);
                match tokio::fs::read(&path).await {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(err) => Err(BlobError::Io(format!("{}: {err}", path.display()))),
                }
            }
            BlobStore::Bucket { name, db } => {
                let rows = db
                    .query_params::<Option<serde_json::Value>>(
                        "RETURN IF file::exists(type::file($bucket, $key)) \
                         THEN file::get(type::file($bucket, $key)) ELSE NONE END;",
                        &std::collections::BTreeMap::from([
                            ("bucket".to_string(), serde_json::json!(name)),
                            ("key".to_string(), serde_json::json!(key)),
                        ]),
                    )
                    .await?;
                let Some(Some(value)) = rows.into_iter().next() else {
                    return Ok(None);
                };
                bytes::decode(value).map(Some).map_err(BlobError::Bucket)
            }
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), BlobError> {
        check_key(key)?;
        match self {
            BlobStore::Dir { root } => {
                let path = root.join(key);
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => Err(BlobError::Io(format!("{}: {err}", path.display()))),
                }
            }
            BlobStore::Bucket { name, db } => {
                db.exec_params(
                    "RETURN file::delete(type::file($bucket, $key));",
                    &std::collections::BTreeMap::from([
                        ("bucket".to_string(), serde_json::json!(name)),
                        ("key".to_string(), serde_json::json!(key)),
                    ]),
                )
                .await?;
                Ok(())
            }
        }
    }
}
fn dir_root(path: &str) -> Result<PathBuf, BlobError> {
    let path = path.strip_prefix("//").unwrap_or(path);
    if path.trim().is_empty() {
        return Err(BlobError::Config(
            "`blobs = \"file:\"` needs a directory".to_string(),
        ));
    }
    Ok(PathBuf::from(path))
}
