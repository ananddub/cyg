//! Label matching against the contract, and the SCTP parameters behind it.

use str0m::channel::{ChannelConfig, Reliability};

use super::{LABEL_RELIABLE, LABEL_UNRELIABLE, LabelVerdict, Lane, classify_inbound, classify_label, contract_mismatch, Inbound};

#[test]
fn exact_labels_are_the_contract_and_other_labels_are_ignored() {
    assert_eq!(classify_label(LABEL_UNRELIABLE), LabelVerdict::Contract(Lane::Unreliable));
    assert_eq!(classify_label(LABEL_RELIABLE), LabelVerdict::Contract(Lane::Reliable));

    // Never a silent fallback to the reliable lane.
    assert_eq!(classify_label("telemetry"), LabelVerdict::Ignored);
    assert_eq!(classify_label("game-chat"), LabelVerdict::Ignored);
    assert_eq!(classify_label(""), LabelVerdict::Ignored);
    assert_eq!(classify_label("GAME-RELIABLE"), LabelVerdict::Ignored);
    assert_eq!(classify_label("game-reliable-v2"), LabelVerdict::Ignored);

    // Documented legacy aliases, including the bare lane words older bundles used.
    assert_eq!(classify_label("unreliable"), LabelVerdict::LegacyAlias(Lane::Unreliable));
    assert_eq!(classify_label("reliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
    assert_eq!(classify_label("client-unreliable"), LabelVerdict::LegacyAlias(Lane::Unreliable));
    assert_eq!(classify_label("client_reliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
    // "unreliable" ends with "reliable": the ordering trap, caught.
    assert_ne!(classify_label("client-unreliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
}

#[test]
fn sctp_parameters_must_match_the_lane() {
    let unreliable_cfg = ChannelConfig {
        label: LABEL_UNRELIABLE.to_string(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        negotiated: None,
        protocol: String::new(),
    };
    let reliable_cfg = ChannelConfig {
        label: LABEL_RELIABLE.to_string(),
        ordered: true,
        reliability: Reliability::Reliable,
        negotiated: None,
        protocol: String::new(),
    };
    assert_eq!(contract_mismatch(Lane::Unreliable, &unreliable_cfg), None);
    assert_eq!(contract_mismatch(Lane::Reliable, &reliable_cfg), None);

    // A browser bundle that opened 'game-unreliable' with defaults is not an unreliable lane, and the transport must say so.
    assert!(contract_mismatch(Lane::Unreliable, &reliable_cfg).is_some());
    assert!(contract_mismatch(Lane::Reliable, &unreliable_cfg).is_some());
}

#[test]
fn sdk_app_messages_classify_as_app() {
    // SDK sendBytes envelope: [0x05][type_len u8][type][payload]
    let frame = [0x05u8, 5, b'i', b'n', b'p', b'u', b't', 1, 2, 3];
    match classify_inbound(Some(Lane::Reliable), &frame) {
        Inbound::App { data } => {
            assert_eq!(data, frame.to_vec(), "full envelope host ko jaata hai");
        }
        _ => panic!("0x05 envelope must classify as App"),
    }
    // truncated envelope (len field ke bina) bhi App hai — host strip karke
    // khud decide karega
    let short = [0x05u8, 5];
    assert!(matches!(
        classify_inbound(Some(Lane::Reliable), &short),
        Inbound::App { .. }
    ));
}
