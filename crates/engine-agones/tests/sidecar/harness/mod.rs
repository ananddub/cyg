use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use engine_agones::{Lifecycle, LifecycleEvent};

mod fake;
mod probe;
mod request;
mod responses;

pub(crate) use fake::FakeSidecar;
pub(crate) use request::closed_port;
pub(crate) use responses::{
    chunked, close_delimited, framed, garbage, hang_up, non_utf8_body, ok, truncated_body,
};

pub(crate) fn ready_retries_seen(lifecycle: &Arc<Lifecycle>, budget: Duration) -> Vec<Option<String>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let runner = {
        let lifecycle = Arc::clone(lifecycle);
        std::thread::spawn(move || {
            lifecycle.run_until_stopped(|event| {
                if let LifecycleEvent::ReadyRetried(error) = event {
                    sink.lock()
                        .expect("the collector does not panic")
                        .push(error.map(|e| e.kind().to_string()));
                }
            });
        })
    };

    std::thread::sleep(budget);
    lifecycle.stop();
    runner.join().expect("the beat loop ended cleanly");
    seen.lock().expect("the collector does not panic").clone()
}
