use bytes::{Bytes, BytesMut};
use engine_net::framing::{OpCode, encode_frame, iter_frames, take_frame};
use proptest::prelude::*;

fn opcode_strategy() -> impl Strategy<Value = OpCode> {
    prop_oneof![
        Just(OpCode::Ping),
        Just(OpCode::Pong),
        Just(OpCode::PlayerInput),
        Just(OpCode::StateDelta),
        Just(OpCode::Custom),
    ]
}

fn payload_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..600)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn any_chunk_split_reassembles_byte_exact(
        opcode in opcode_strategy(),
        payload in payload_strategy(),
        chunk_size in 1usize..64,
    ) {
        let mut wire = BytesMut::new();
        encode_frame(opcode, &payload, &mut wire);

        let mut acc = BytesMut::new();
        let mut decoded: Vec<(OpCode, Bytes)> = Vec::new();
        for chunk in wire.chunks(chunk_size) {
            acc.extend_from_slice(chunk);
            while let Some(frame) = take_frame(&mut acc) {
                decoded.push(frame);
            }
        }
        prop_assert_eq!(decoded.len(), 1, "exactly one frame was encoded");
        prop_assert_eq!(decoded[0].0, opcode);
        prop_assert_eq!(decoded[0].1.to_vec(), payload.clone(), "frame payload must be byte-exact");
    }

    #[test]
    fn concatenated_frames_decode_exactly(
        frames in prop::collection::vec((opcode_strategy(), payload_strategy()), 0..12),
    ) {
        let mut wire = BytesMut::new();
        for (op, payload) in &frames {
            encode_frame(*op, payload, &mut wire);
        }
        let decoded: Vec<(OpCode, Vec<u8>)> = iter_frames(&wire)
            .map(|(op, payload)| (op, payload.to_vec()))
            .collect();
        let expected: Vec<(OpCode, Vec<u8>)> =
            frames.iter().map(|(op, p)| (*op, p.clone())).collect();
        prop_assert_eq!(decoded, expected, "iter_frames must roundtrip every frame");
    }

    #[test]
    fn garbage_never_panics_the_decoder(garbage in prop::collection::vec(any::<u8>(), 0..300)) {

        let mut buf = BytesMut::from(&garbage[..]);
        let mut guard = 0;
        while take_frame(&mut buf).is_some() {
            guard += 1;
            prop_assume!(guard < 1000, "decoder loop must terminate on garbage");
        }

        let _ = iter_frames(&garbage).count();
    }
}
