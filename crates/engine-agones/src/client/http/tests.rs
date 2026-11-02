use super::{Parse, try_parse};
use crate::error::SidecarError;

#[test]
fn parses_content_length_body() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    match try_parse(raw, false) {
        Parse::Complete(r) => assert_eq!(r.body, b"hello"),
        other => panic!("expected a complete reply, got {other:?}"),
    }

    let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhelloTRAILER";
    match try_parse(raw, false) {
        Parse::Complete(r) => assert_eq!(r.body, b"hello"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn parses_close_delimited_body() {
    let raw = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"state\":\"Ready\"}";
    match try_parse(raw, false) {
        Parse::Partial => {}
        other => panic!("without a length the body is not finished yet: {other:?}"),
    }
    match try_parse(raw, true) {
        Parse::Complete(r) => assert_eq!(r.body_lossy(), "{\"state\":\"Ready\"}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn parses_chunked_body() {
    let raw =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nfoo\r\n6\r\nbarbaz\r\n0\r\n\r\n";
    match try_parse(raw, false) {
        Parse::Complete(r) => assert_eq!(r.body_lossy(), "foobarbaz"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_chunk_size_that_does_not_match_its_data_is_malformed() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nbarbaz\r\n0\r\n\r\n";
    match try_parse(raw, false) {
        Parse::Failed(SidecarError::Malformed { reason }) => {
            assert!(reason.contains("CRLF"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_non_hexadecimal_chunk_size_is_malformed() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nfoo\r\n";
    assert!(matches!(
        try_parse(raw, false),
        Parse::Failed(SidecarError::Malformed { .. })
    ));
}

#[test]
fn chunked_with_missing_terminator_is_not_complete() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nfoo";
    assert!(matches!(try_parse(raw, false), Parse::Partial));
    assert!(matches!(
        try_parse(raw, true),
        Parse::Failed(SidecarError::Malformed { .. })
    ));
}

#[test]
fn truncated_content_length_is_malformed_not_panic() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{\"st";
    match try_parse(raw, true) {
        Parse::Failed(SidecarError::Malformed { reason }) => {
            assert!(reason.contains("promised 100"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn no_header_block_at_eof_is_malformed() {
    let raw = b"HTTP/1.1 200 OK\r\nCont";
    assert!(matches!(try_parse(raw, false), Parse::Partial));
    assert!(matches!(
        try_parse(raw, true),
        Parse::Failed(SidecarError::Malformed { .. })
    ));
}

#[test]
fn non_utf8_headers_still_parse() {
    let raw = b"HTTP/1.1 500 \xfe\xff\r\nContent-Length: 1\r\n\r\nx";
    match try_parse(raw, false) {
        Parse::Complete(r) => assert_eq!(r.status, 500),
        other => panic!("{other:?}"),
    }
}

#[test]
fn empty_or_garbage_status_line_is_malformed() {
    let raw = b"not http at all\r\n\r\n";
    assert!(matches!(
        try_parse(raw, true),
        Parse::Failed(SidecarError::Malformed { .. })
    ));
}

#[test]
fn no_content_length_means_until_eof_and_204_is_immediate() {
    let raw = b"HTTP/1.1 204 No Content\r\n\r\n";
    assert!(matches!(try_parse(raw, false), Parse::Complete(r) if r.body.is_empty()));
}

#[test]
fn conflicting_content_lengths_are_malformed() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 9\r\n\r\nhello";
    assert!(matches!(
        try_parse(raw, false),
        Parse::Failed(SidecarError::Malformed { .. })
    ));
}
