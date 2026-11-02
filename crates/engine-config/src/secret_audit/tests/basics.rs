use super::*;

#[test]
fn a_configured_deployment_passes_the_audit() {
    let checks = audit_strong(&[]);
    assert_eq!(problems(&checks), 0, "{checks:?}");
}

#[test]
fn every_documented_default_is_named_in_the_report() {
    let checks = audit_secrets(&ControlConfig::default(), &Rotation::default(), &env(&[]));
    let failed = failures(&checks);
    for name in [
        "session.signing_key",
        "session.refresh_signing_key",
        "session.server_key",
        "control.seat_secret",
        "control.console.signing_key",
        "control.console.password",
    ] {
        assert!(
            failed.iter().any(|d| d.starts_with(name)),
            "{name} must be reported: {failed:?}"
        );
    }
}

#[test]
fn doctor_fails_when_a_credential_is_accepted_unverified() {
    for (name, why) in [
        ("OX_IAP_ALLOW_UNVERIFIED", "currency"),
        ("OX_SOCIAL_ALLOW_UNVERIFIED", "identity"),
    ] {
        for value in ["1", "true"] {
            let checks = audit_strong(&[(name, value)]);
            assert_eq!(problems(&checks), 1, "{name}={value}");
            assert!(
                failures(&checks)[0].contains(name) && failures(&checks)[0].contains(why),
                "{name}={value} must say why: {:?}",
                failures(&checks)
            );
        }
    }
    let mut configured = strong();
    configured.iap.allow_unverified = true;
    configured.social.allow_unverified = true;
    assert_eq!(
        problems(&audit_secrets(&configured, &Rotation::default(), &env(&[]))),
        2,
        "the config spelling fails exactly like the env"
    );
}
