//! The breaker and bulkhead are judged by what the destination is asked, not by
//! what this module believes: every test counts connections on a real socket.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use reqwest::Client;

/// A destination that answers every request with `status` and counts them.
fn canned(status: u16) -> (String, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a fake vendor");
    let addr = listener.local_addr().expect("fake vendor address");
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let response = format!(
                "HTTP/1.1 {status} Vendor\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}"
            );
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        }
    });
    (format!("http://{addr}/"), hits)
}

fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("test client")
}

fn unique(name: &str) -> String {
    format!("{name}-{}", uuid::Uuid::new_v4())
}

#[tokio::test]
async fn a_dead_destination_stops_being_asked() {
    let (url, hits) = canned(500);
    let client = client();
    let destination = unique("dead");
    let trips = liveops::config().http_breaker_trips as usize;

    for _ in 0..trips {
        let _ = send(&destination, client.get(&url)).await;
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        trips,
        "each failure is a real attempt"
    );

    // The breaker is open now: further calls are refused in memory.
    for _ in 0..3 {
        match send(&destination, client.get(&url)).await {
            Err(HttpError::Open { .. }) => {}
            other => panic!("expected an open breaker, got {other:?}"),
        }
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        trips,
        "an open breaker never opens a socket"
    );

    let state = snapshot()
        .into_iter()
        .find(|(_, line)| {
            line.starts_with(&format!(
                "cyg_http_breaker_state{{destination=\"{destination}\"}}"
            ))
        })
        .map(|(_, line)| line);
    assert!(
        state.is_some_and(|line| line.ends_with('2')),
        "the gauge reports open"
    );
}

#[tokio::test]
async fn a_provider_that_answers_badly_is_still_alive() {
    let (url, hits) = canned(400);
    let client = client();
    let destination = unique("refusing");

    for _ in 0..(liveops::config().http_breaker_trips * 2) {
        let response = send(&destination, client.get(&url))
            .await
            .expect("a 400 is an answer");
        assert_eq!(response.status().as_u16(), 400);
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        (liveops::config().http_breaker_trips * 2) as usize,
        "a provider that answers is always reachable"
    );
}

#[tokio::test]
async fn a_half_open_breaker_sends_exactly_one_probe() {
    let (url, hits) = canned(500);
    let client = client();
    let destination = unique("half");
    let trips = liveops::config().http_breaker_trips as usize;

    for _ in 0..trips {
        let _ = send(&destination, client.get(&url)).await;
    }
    // Rewind the cooldown instead of sleeping for it.
    {
        let mut breakers = plane().breakers.lock().expect("breaker lock");
        let breaker = breakers.get_mut(&destination).expect("a breaker exists");
        breaker.open_until = Some(Instant::now() - Duration::from_millis(1));
    }

    // The first call is the probe and is allowed through (and fails).
    let first = send(&destination, client.get(&url)).await;
    assert!(first.is_ok(), "the probe reaches the destination");
    assert_eq!(hits.load(Ordering::SeqCst), trips + 1);
    // It failed, so the breaker is open again with nothing else let out.
    let second = send(&destination, client.get(&url)).await;
    assert!(
        matches!(second, Err(HttpError::Open { .. })),
        "a failed probe re-opens the breaker"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        trips + 1,
        "no second probe on this cooldown"
    );
}

#[tokio::test]
async fn a_recovered_destination_closes_the_breaker() {
    let (url, _hits) = canned(500);
    let client = client();
    let destination = unique("recovering");
    let trips = liveops::config().http_breaker_trips as usize;
    for _ in 0..trips {
        let _ = send(&destination, client.get(&url)).await;
    }
    {
        let mut breakers = plane().breakers.lock().expect("breaker lock");
        let breaker = breakers.get_mut(&destination).expect("a breaker exists");
        breaker.open_until = Some(Instant::now() - Duration::from_millis(1));
    }
    // Rewrite this destination to be healthy by recording success as if the
    // probe had come back 200.
    record(&destination, Outcome::Alive);
    let state = plane()
        .breakers
        .lock()
        .expect("breaker lock")
        .get(&destination)
        .map(|breaker| breaker.state(Instant::now()));
    assert_eq!(state, Some(BreakerState::Closed));
}
