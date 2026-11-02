use std::time::{Duration, Instant};

use engine_agones::{AgonesClient, SidecarConfig};

use super::fake::FakeSidecar;
use super::request::Request;

impl FakeSidecar {
    pub(crate) fn client(&self, timeout: Duration) -> AgonesClient {
        AgonesClient::new(self.config(timeout))
    }

    pub(crate) fn config(&self, timeout: Duration) -> SidecarConfig {
        SidecarConfig {
            addr: self.addr.to_string(),
            timeout,
        }
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.inner.lock().unwrap().requests.clone()
    }

    pub(crate) fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.path).collect()
    }

    pub(crate) fn count(&self, path: &str) -> usize {
        self.requests()
            .into_iter()
            .filter(|r| r.path == path)
            .count()
    }

    pub(crate) fn request_for(&self, path: &str) -> Option<Request> {
        self.requests().into_iter().find(|r| r.path == path)
    }

    pub(crate) fn wait_for_requests(&self, path: &str, at_least: usize, budget: Duration) -> usize {
        let until = Instant::now() + budget;
        loop {
            let seen = self.count(path);
            if seen >= at_least || Instant::now() >= until {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
