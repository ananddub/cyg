use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub addr: String,
    pub timeout: Duration,
}

impl SidecarConfig {
    pub fn new(host: &str, port: u16, timeout: Duration) -> SidecarConfig {
        SidecarConfig {
            addr: format!("{host}:{port}"),
            timeout,
        }
    }

    pub fn loopback(port: u16) -> SidecarConfig {
        SidecarConfig::new("127.0.0.1", port, Duration::from_secs(1))
    }

    pub fn with_timeout(mut self, timeout: Duration) -> SidecarConfig {
        self.timeout = timeout;
        self
    }
}
