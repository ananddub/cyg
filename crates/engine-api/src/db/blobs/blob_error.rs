use super::*;

pub(super) const BUCKET: &str = "blobs";
pub(super) const DEFAULT_DIR: &str = "data/uploads";
#[derive(Debug, Clone)]
pub enum BlobError {
    Unconfigured,
    Config(String),
    InvalidKey(String),
    Io(String),
    Bucket(String),
}
impl fmt::Display for BlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlobError::Unconfigured => {
                write!(f, "the blob store is not configured (`[control.session] blobs`)")
            }
            BlobError::Config(message) => write!(f, "blob store configuration: {message}"),
            BlobError::InvalidKey(key) => write!(f, "invalid blob key `{key}`"),
            BlobError::Io(message) => write!(f, "blob storage io: {message}"),
            BlobError::Bucket(message) => write!(f, "blob bucket: {message}"),
        }
    }
}
impl std::error::Error for BlobError {}
impl From<crate::db::DbError> for BlobError {
    fn from(err: crate::db::DbError) -> Self {
        BlobError::Bucket(err.to_string())
    }
}
pub enum BlobStore {
    Dir { root: PathBuf },
    Bucket { name: String, db: Arc<Datastore> },
}
impl fmt::Debug for BlobStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlobStore::Dir { root } => f.debug_struct("Dir").field("root", root).finish(),
            BlobStore::Bucket { name, .. } => f.debug_struct("Bucket").field("name", name).finish(),
        }
    }
}
