use std::sync::Arc;

use crate::state::{LIVE_REFRESHERS, subscription_refresh_gates};

pub(crate) async fn refresh_subscription(
    endpoint: &str,
    args: serde_json::Value,
    identity: Option<crate::LiveIdentity>,
) {
    let Some(refreshers) = LIVE_REFRESHERS.get() else {
        return;
    };
    for entry in refreshers.iter().filter(|entry| entry.endpoint == endpoint) {
        let gate = {
            let gates = subscription_refresh_gates();
            if let Some(gate) = gates.read_sync(endpoint, |_, g| g.clone()) {
                gate
            } else {
                match gates.entry_sync(endpoint.to_owned()) {
                    scc::hash_map::Entry::Occupied(occ) => occ.get().clone(),
                    scc::hash_map::Entry::Vacant(vac) => {
                        let g = Arc::new(tokio::sync::Mutex::new(()));
                        vac.insert_entry(g.clone());
                        g
                    }
                }
            }
        };
        let _guard = gate.lock().await;
        (entry.refresh)(args.clone(), identity.clone()).await;
    }
}
