#[derive(Clone, Debug)]
pub struct BootModule {
    pub version: i32,

    pub sha256: String,

    pub bytes: Vec<u8>,
}
