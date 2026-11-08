use crate::transport::ws::http::request::parse_greeting;

#[test]
fn options_query_is_percent_decoded() {
    let raw =
        b"GET /abc?sessionId=s1&options=%7B%22token%22%3A%22a+b%22%7D HTTP/1.1\r\nHost: x\r\n\r\n";
    let greeting = parse_greeting(raw);
    assert_eq!(greeting.url_session_id.as_deref(), Some("s1"));
    assert_eq!(greeting.url_options.as_deref(), Some(r#"{"token":"a b"}"#));
}

#[test]
fn missing_options_stays_none() {
    let raw = b"GET /abc?sessionId=s1 HTTP/1.1\r\nHost: x\r\n\r\n";
    assert!(parse_greeting(raw).url_options.is_none());
}
