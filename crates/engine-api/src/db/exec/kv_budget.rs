use super::*;

const MIB: u64 = 1024 * 1024;
#[derive(Debug, Clone, Copy, Default)]
pub struct KvBudget {
    pub memtable_mib: Option<u64>,
    pub cache_mib: Option<u64>,
}
impl KvBudget {
    pub(super) fn config(self) -> ConfigMap {
        let mut config = ConfigMap::empty();
        if let Some(mib) = self.memtable_mib {
            config = config.with_key_value(
                "surrealkv_max_memtable_size",
                mib.saturating_mul(MIB).to_string(),
            );
        }
        if let Some(mib) = self.cache_mib {
            config = config.with_key_value(
                "surrealkv_block_cache_capacity",
                mib.saturating_mul(MIB).to_string(),
            );
        }
        config
    }
}
pub struct Datastore {
    pub(super) client: Client,
}
pub(super) enum Client {
    Sdk(Surreal<Any>),
    Kv(Surreal<Db>),
}
