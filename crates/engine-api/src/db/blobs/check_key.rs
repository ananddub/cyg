use super::*;

pub(super) fn check_key(key: &str) -> Result<(), BlobError> {
    let bad = key.is_empty()
        || key.starts_with('/')
        || key.starts_with('\\')
        || key.contains(['\0', '\\']);
    let traversal = key
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..");
    if bad || traversal {
        return Err(BlobError::InvalidKey(key.to_string()));
    }
    Ok(())
}
pub(super) fn temp_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", Uuid::new_v4()));
    PathBuf::from(tmp)
}
#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::super::*;
    use crate::db::KvBudget;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_root(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("cyg-blobs-{tag}-{}-{n}", std::process::id()))
    }

    #[tokio::test]
    async fn dir_round_trips_overwrites_and_deletes() {
        let root = temp_root("dir");
        let store = BlobStore::open(&format!("file:{}", root.display()), "mem://", None)
            .await
            .expect("a file: config is disk-backed and needs no datastore");
        assert!(matches!(&store, BlobStore::Dir { root: r } if *r == root));
        let key = "wasm/018f/1.wasm";

        assert_eq!(store.get(key).await.unwrap(), None, "nothing stored yet");
        store.put(key, b"first").await.unwrap();
        assert_eq!(store.get(key).await.unwrap().as_deref(), Some(&b"first"[..]));
        store.put(key, b"second").await.unwrap();
        assert_eq!(store.get(key).await.unwrap().as_deref(), Some(&b"second"[..]));

        store.delete(key).await.unwrap();
        assert_eq!(store.get(key).await.unwrap(), None);
        store.delete(key).await.unwrap();

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn keys_cannot_escape_the_root() {
        let store = BlobStore::Dir {
            root: temp_root("traversal"),
        };
        for key in ["", "/abs", "../up", "a/../b", "a//b", "./a", "a\\b"] {
            assert!(
                matches!(store.put(key, b"x").await, Err(BlobError::InvalidKey(_))),
                "{key} must be refused"
            );
        }
    }

    #[tokio::test]
    async fn memory_bucket_round_trips() {
        let db = Arc::new(
            Datastore::open("mem://", "blobs", "blobs", None, None, KvBudget::default())
                .await
                .expect("open mem://"),
        );
        let store = BlobStore::open("memory://", "mem://", Some(db))
            .await
            .expect("the memory bucket is definable");
        let key = "maps/018f/1.map3d";

        assert_eq!(store.get(key).await.unwrap(), None, "nothing stored yet");
        store.put(key, b"arena").await.unwrap();
        assert_eq!(store.get(key).await.unwrap().as_deref(), Some(&b"arena"[..]));
        store.delete(key).await.unwrap();
        assert_eq!(store.get(key).await.unwrap(), None);
    }

    #[tokio::test]
    async fn embedded_defaults_resolve() {
        let disk = BlobStore::open("", "surrealkv://data/cyg", None)
            .await
            .expect("an embedded store falls back to disk");
        assert!(matches!(disk, BlobStore::Dir { root } if root == PathBuf::from(DEFAULT_DIR)));

        let db = Arc::new(
            Datastore::open("mem://", "blobs", "blobs", None, None, KvBudget::default())
                .await
                .expect("open mem://"),
        );
        let memory = BlobStore::open("  ", "mem://", Some(db))
            .await
            .expect("a mem:// run gets an in-process bucket");
        assert!(matches!(memory, BlobStore::Bucket { .. }));
    }

    #[tokio::test]
    async fn a_remote_endpoint_needs_a_backend() {
        let err = BlobStore::open("", "wss://db.example.com", None)
            .await
            .expect_err("a shared datastore cannot fall back to local disk");
        assert!(matches!(err, BlobError::Unconfigured));
    }
}
