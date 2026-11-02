use syn::{LitStr, Signature};

use super::harvest::Harvested;
use crate::model::LiveStrategy;

pub(super) fn resolve_live(
    signature: &Signature,
    harvested: &mut Harvested,
) -> syn::Result<LiveStrategy> {
    let is_live = harvested.is_live;
    if is_live && harvested.live_event.is_none() {
        harvested.live_event = Some(LitStr::new(
            &signature.ident.to_string(),
            signature.ident.span(),
        ));
    }
    if is_live && harvested.live_client_name.is_none() {
        harvested.live_client_name = Some(LitStr::new(
            &signature.ident.to_string(),
            signature.ident.span(),
        ));
    }
    let has_live_tables = harvested.live_table.is_some() || !harvested.live_tables.is_empty();
    let live_strategy = harvested.live_strategy.unwrap_or(if has_live_tables {
        LiveStrategy::Sqlite
    } else {
        LiveStrategy::Publish
    });
    if live_strategy == LiveStrategy::Sqlite && !has_live_tables {
        return Err(syn::Error::new_spanned(
            signature,
            "strategy = sqlite requires table or tables",
        ));
    }
    if live_strategy != LiveStrategy::Sqlite && has_live_tables {
        return Err(syn::Error::new_spanned(
            signature,
            "table/tables can only be used with strategy = sqlite",
        ));
    }
    if harvested.live_capacity.is_some() && live_strategy != LiveStrategy::Stream {
        return Err(syn::Error::new_spanned(
            signature,
            "capacity is only valid with strategy = stream",
        ));
    }
    if harvested.live_replay.is_some() && live_strategy != LiveStrategy::Stream {
        return Err(syn::Error::new_spanned(
            signature,
            "replay is only valid with strategy = stream",
        ));
    }
    if let Some(replay) = &harvested.live_replay {
        if replay.base10_parse::<usize>()? > 10_000 {
            return Err(syn::Error::new_spanned(
                replay,
                "stream replay cannot exceed 10000 events",
            ));
        }
    }
    Ok(live_strategy)
}
