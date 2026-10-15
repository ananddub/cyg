use crate::{
    LiveRefreshDescriptor,
    state::{LIVE_REFRESHERS, ResolvedLiveRefresher},
};
use auto_di::{Container, DiError};

pub(crate) async fn install_refreshers(container: &Container) -> Result<(), DiError> {
    let mut refreshers = Vec::new();
    for descriptor in inventory::iter::<LiveRefreshDescriptor> {
        refreshers.push(ResolvedLiveRefresher {
            endpoint: descriptor.endpoint,
            refresh: (descriptor.factory)(container).await?,
        });
    }
    let _ = LIVE_REFRESHERS.set(refreshers);
    Ok(())
}
