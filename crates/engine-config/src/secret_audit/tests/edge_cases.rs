use super::*;

#[test]
fn a_flag_value_that_is_neither_on_nor_off_fails_instead_of_being_ignored() {
    for value in ["maybe", "", "01"] {
        let checks = audit_strong(&[("CYG_IAP_ALLOW_UNVERIFIED", value)]);
        assert_eq!(problems(&checks), 1, "{value:?}");
        assert!(
            failures(&checks)[0].contains("neither on nor off"),
            "{value:?}: {:?}",
            failures(&checks)
        );
    }
    for value in ["0", "false", "off", "no"] {
        assert_eq!(
            problems(&audit_strong(&[("CYG_IAP_ALLOW_UNVERIFIED", value)])),
            0,
            "{value:?} is an explicit off"
        );
    }
}

#[test]
fn the_development_escape_hatch_skips_strength_but_not_flags() {
    let checks = audit_secrets(
        &ControlConfig::default(),
        &Rotation::default(),
        &env(&[
            ("CYG_ALLOW_DEFAULT_SECRETS", "1"),
            ("CYG_IAP_ALLOW_UNVERIFIED", "1"),
        ]),
    );
    assert_eq!(problems(&checks), 1, "{checks:?}");
    assert!(
        checks
            .iter()
            .all(|c| !c.detail.contains("session.signing_key")),
        "the guard is off, so strength is not this audit's job"
    );
}

#[test]
fn a_retired_secret_has_to_be_as_unguessable_as_the_key_it_replaced() {
    let rotation = Rotation {
        previous_signing_key: "old".into(),
        previous_seat_secret: "a-retired-seat-secret-long-enough".into(),
        ..Default::default()
    };
    let checks = audit_secrets(&strong(), &rotation, &env(&[]));
    assert_eq!(problems(&checks), 1, "{checks:?}");
    assert!(failures(&checks)[0].contains("CYG_PREVIOUS_SIGNING_KEY"));
    assert!(
        checks
            .iter()
            .any(|c| c.ok && c.detail.contains("CYG_PREVIOUS_SEAT_SECRET"))
    );
}

#[test]
fn a_remote_database_needs_a_password_worth_the_transport() {
    let mut c = strong();
    c.session.database_endpoint = "ws://db:8000/rpc".into();
    assert_eq!(
        problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
        1
    );
    c.session.database_pass = "s3cret".into();
    assert_eq!(
        problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
        1
    );
    c.session.database_pass = "a-long-enough-database-pass".into();
    assert_eq!(
        problems(&audit_secrets(&c, &Rotation::default(), &env(&[]))),
        0
    );
}
