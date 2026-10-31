use crate::infra::seats::*;

#[test]
fn base64_round_trips_every_length() {
    for len in 1..12 {
        let data: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        assert_eq!(b64url_decode(&b64url(&data)).unwrap(), data, "len {len}");
    }
}

#[test]
fn base64_matches_rfc4648_urlsafe_vectors() {
    assert_eq!(b64url(b""), "");
    assert_eq!(b64url(b"f"), "Zg");
    assert_eq!(b64url(b"fo"), "Zm8");
    assert_eq!(b64url(b"foo"), "Zm9v");
    assert_eq!(b64url(b"foob"), "Zm9vYg");
    assert_eq!(b64url(b"fooba"), "Zm9vYmE");
    assert_eq!(b64url(b"foobar"), "Zm9vYmFy");
    assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
}

#[test]
fn decoder_rejects_padded_empty_and_trailing_input() {
    assert_eq!(b64url_decode("Zm9v"), Some(b"foo".to_vec()));
    assert!(b64url_decode("").is_none());
    assert!(b64url_decode("Zm9v=").is_none());
    assert!(b64url_decode("Q").is_none());
    assert!(b64url_decode("Zm9+").is_none());
}
