use super::*;
use crate::messages::Via;

fn input(seq: u16) -> InputMsg {
    InputMsg {
        player_id: 1,
        seq,
        payload: [0; 20],
        arrival_ns: 0,
        via: Via::NativeUdp,
    }
}

#[test]
fn push_input_accepts_until_full() {
    let bus = Bus::new(2, 2);
    bus.push_input(input(1));
    bus.push_input(input(2));
    assert_eq!(bus.inputs_rx.len(), 2);
}

#[test]
fn push_input_full_drops_the_oldest_and_keeps_the_newest() {
    let bus = Bus::new(2, 2);
    bus.push_input(input(1));
    bus.push_input(input(2));
    bus.push_input(input(3));
    assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 2);
    assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 3);
    assert!(bus.inputs_rx.try_recv().is_err());
}

#[test]
fn push_input_when_the_sim_is_gone_is_a_silent_noop() {
    let bus = Bus::new(2, 2);
    let tx = bus.inputs_tx.clone();
    let orphaned = Arc::try_unwrap(bus).ok().unwrap();
    drop(orphaned);
    assert!(matches!(
        tx.try_send(input(1)),
        Err(crossbeam_channel::TrySendError::Disconnected(_))
    ));
}
