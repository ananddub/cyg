use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

use engine_api::db::{Datastore, KvBudget};

#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

#[derive(Default)]
struct Message(String);

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value}");
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        }
    }
}

impl<S: Subscriber> Layer<S> for Lines {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut message = Message::default();
        event.record(&mut message);
        self.0.lock().unwrap().push(message.0);
    }
}

#[tokio::test]
async fn surrealkv_applies_the_configured_budget() {
    let lines = Lines::default();
    let guard = tracing_subscriber::registry()
        .with(lines.clone())
        .set_default();

    let root = std::env::temp_dir().join(format!("cyg-kv-budget-{}", uuid::Uuid::new_v4()));
    let endpoint = format!("surrealkv://{}", root.display());
    let db = Datastore::open(
        &endpoint,
        "budget",
        "budget",
        None,
        None,
        KvBudget {
            memtable_mib: Some(8),
            cache_mib: Some(8),
        },
    )
    .await
    .expect("a sized surrealkv opens");

    db.exec("CREATE person:one SET name = 'a';")
        .await
        .expect("write");
    let names: Vec<String> = db
        .query("SELECT VALUE name FROM person;")
        .await
        .expect("read");
    assert_eq!(names, vec!["a".to_string()]);

    drop(guard);
    let log = lines.0.lock().unwrap().join("\n");
    assert!(
        log.contains("Setting max memtable size: 8388608"),
        "the memtable budget must reach the engine: {log}"
    );
    assert!(
        log.contains("Setting block cache capacity: 8388608"),
        "the cache budget must reach the engine: {log}"
    );

    drop(db);
    let _ = std::fs::remove_dir_all(root);
}
