use std::collections::VecDeque;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::request::{Request, read_request};

pub(crate) struct FakeInner {
    pub(super) requests: Vec<Request>,
    replies: VecDeque<Vec<u8>>,
    fallback: Vec<u8>,
    routes: Vec<(String, Vec<u8>)>,
    hang: bool,
}

pub(crate) struct FakeSidecar {
    pub(super) addr: SocketAddr,
    pub(super) inner: Arc<Mutex<FakeInner>>,
    stop: Arc<AtomicBool>,
    serve: Option<JoinHandle<()>>,
}

impl FakeSidecar {
    pub(crate) fn scripted(script: Vec<Vec<u8>>, fallback: Vec<u8>) -> FakeSidecar {
        FakeSidecar::with_routes(script, fallback, Vec::new())
    }

    pub(crate) fn routing(routes: Vec<(&str, Vec<u8>)>, fallback: Vec<u8>) -> FakeSidecar {
        FakeSidecar::with_routes(
            Vec::new(),
            fallback,
            routes
                .into_iter()
                .map(|(path, reply)| (path.to_string(), reply))
                .collect(),
        )
    }

    fn with_routes(
        script: Vec<Vec<u8>>,
        fallback: Vec<u8>,
        routes: Vec<(String, Vec<u8>)>,
    ) -> FakeSidecar {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake sidecar binds");
        let addr = listener.local_addr().expect("fake sidecar has an address");
        let inner = Arc::new(Mutex::new(FakeInner {
            requests: Vec::new(),
            replies: VecDeque::from(script),
            fallback,
            routes,
            hang: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));

        let (thread_inner, thread_stop) = (Arc::clone(&inner), Arc::clone(&stop));
        listener.set_nonblocking(true).expect("nonblocking accept");
        let serve = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        let (reply, hang) = {
                            let mut state = thread_inner.lock().unwrap();
                            if let Some(request) = &request {
                                state.requests.push(request.clone());
                            }
                            let routed = request
                                .as_ref()
                                .and_then(|r| state.routes.iter().find(|(p, _)| *p == r.path))
                                .map(|(_, reply)| reply.clone());
                            let reply = routed.unwrap_or_else(|| {
                                state
                                    .replies
                                    .pop_front()
                                    .unwrap_or_else(|| state.fallback.clone())
                            });
                            (reply, state.hang)
                        };
                        if request.is_none() {
                            continue;
                        }
                        if hang {
                            let until = Instant::now() + Duration::from_secs(2);
                            while Instant::now() < until && !thread_stop.load(Ordering::Relaxed) {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            continue;
                        }
                        let _ = stream.write_all(&reply);
                        let _ = stream.flush();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        FakeSidecar {
            addr,
            inner,
            stop,
            serve: Some(serve),
        }
    }

    pub(crate) fn always(reply: Vec<u8>) -> FakeSidecar {
        FakeSidecar::scripted(Vec::new(), reply)
    }

    pub(crate) fn silent() -> FakeSidecar {
        let fake = FakeSidecar::always(super::responses::ok(b"{}"));
        fake.inner.lock().unwrap().hang = true;
        fake
    }
}

impl Drop for FakeSidecar {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.serve.take() {
            let _ = handle.join();
        }
    }
}
