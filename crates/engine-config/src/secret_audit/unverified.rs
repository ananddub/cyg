use crate::control::ControlConfig;
use super::{flag_is_off, unverified_reason, Check, Env};

pub(super) fn audit(control: &ControlConfig, env: Env, out: &mut Vec<Check>) {
    for (name, configured) in [
        ("OX_IAP_ALLOW_UNVERIFIED", control.iap.allow_unverified),
        (
            "OX_SOCIAL_ALLOW_UNVERIFIED",
            control.social.allow_unverified,
        ),
    ] {
        match flag_is_off(env(name).as_deref()) {
            Err(value) => out.push(Check::fail(format!(
                "{name}={value:?} is neither on nor off; the parser reads it as unset, so what was meant is unknowable — write 0 or delete the line"
            ))),
            Ok(off) => out.push(if off && !configured {
                Check::pass(format!("{name} is off"))
            } else {
                Check::fail(format!(
                    "{} is on: {}",
                    name,
                    unverified_reason(name)
                ))
            }),
        }
    }
}
