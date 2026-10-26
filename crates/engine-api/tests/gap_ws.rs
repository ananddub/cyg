//! The realtime `/ws` surface, end to end — every `#[on("...")]` event tag the
//! socket declares, driven by real websocket clients against the real plane
//! (Postgres 16 in a Testcontainer, the same harness `rest_e2e.rs` uses).
//!
//! `ws_socket.rs` covers `ping`, the lifecycle hooks and the auth refusal; this
//! file covers the remaining 26 tags plane by plane: matchmaker, match, party,
//! chat, presence, notifications. The read side of the match REST surface
//! (`GET /v2/match`, `/v2/matchmaker/stats`) is asserted here too, because the
//! registries these sockets populate are exactly what those routes print.

use std::collections::HashMap;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;

use harness::*;

use tokio_tungstenite::tungstenite::Message as Raw;

/// One websocket client: unique `cid` namespace, a pending queue so frames
/// meant for a later assertion are never swallowed.
struct WsClient {
    name: String,
    user: String,
    uid: Uuid,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    n: u32,
    pending: Vec<Value>,
}

impl WsClient {
    async fn connect(port: u16, token: &str, name: &str, uid: Uuid, user: &str) -> WsClient {
        let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        let mut client = WsClient {
            name: name.into(),
            user: user.into(),
            uid,
            ws,
            n: 0,
            pending: Vec::new(),
        };
        client.liveness().await;
        client
    }

    /// Liveness probe: proves the `#[connect]` hook ran before any event flies.
    async fn liveness(&mut self) {
        let cid = self.next_cid();
        self.send(json!({"cid": cid, "type": "ping"}).to_string())
            .await;
        let pong = self.take_reply(&cid, Some("pong"), "ping liveness").await;
        assert_eq!(pong["type"], "pong");
    }

    fn next_cid(&mut self) -> String {
        self.n += 1;
        format!("{}{}", self.name, self.n)
    }

    async fn send(&mut self, text: String) {
        self.ws
            .send(Raw::Text(text.into()))
            .await
            .expect("frame sent");
    }

    /// Send an event with a payload merged into the envelope; returns the cid.
    async fn send_event(&mut self, tag: &str, payload: Value) -> String {
        let cid = self.next_cid();
        let mut frame = json!({"cid": cid, "type": tag});
        if let Value::Object(map) = payload {
            for (key, value) in map {
                frame[key] = value;
            }
        }
        self.send(frame.to_string()).await;
        cid
    }

    /// Send without caring about any reply (the frame is still read later by
    /// whatever assertion follows).
    async fn fire(&mut self, tag: &str, payload: Value) {
        self.send_event(tag, payload).await;
    }

    async fn read_frame(&mut self) -> Option<Value> {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(10), self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: a frame within 10 s", self.name))?;
            let Ok(msg) = msg else {
                panic!("{}: socket closed mid-read", self.name);
            };
            if let Raw::Text(text) = msg {
                return Some(serde_json::from_str(text.as_str()).expect("frame is JSON"));
            }
        }
    }

    fn find(&self, want: &dyn Fn(&Value) -> bool) -> Option<usize> {
        self.pending.iter().position(want)
    }

    async fn take_where(&mut self, want: &dyn Fn(&Value) -> bool, describe: &str) -> Value {
        if let Some(index) = self.find(want) {
            return self.pending.remove(index);
        }
        loop {
            let frame = match self.read_frame().await {
                Some(frame) => frame,
                None => panic!("{}: socket closed while waiting for {describe}", self.name),
            };
            if want(&frame) {
                return frame;
            }
            self.pending.push(frame);
        }
    }

    /// The reply correlated with `cid`, optionally of a specific type.
    async fn take_reply(&mut self, cid: &str, want_type: Option<&str>, describe: &str) -> Value {
        let want = want_type.map(str::to_owned);
        let cid = cid.to_owned();
        self.take_where(
            &|frame| {
                frame["cid"] == cid.as_str() && want.as_deref().is_none_or(|t| frame["type"] == t)
            },
            &format!("{describe} (cid {cid})"),
        )
        .await
    }

    /// A frame of this type, cid-agnostic: broadcasts included.
    async fn take_event(&mut self, want_type: &str) -> Value {
        let want = want_type.to_owned();
        self.take_where(&|frame| frame["type"] == want, &format!("event {want}"))
            .await
    }

    /// The `type:"error"` frame answering this cid; returns its message.
    async fn take_ws_error(&mut self, cid: &str, describe: &str) -> String {
        let frame = self
            .take_where(
                &|f| f["type"] == "error" && f["cid"] == cid,
                &format!("{describe} (error on cid {cid})"),
            )
            .await;
        frame["message"].as_str().unwrap_or_default().to_owned()
    }

    /// Consume everything queued plus whatever arrives in this window, so the
    /// next assertion starts from a clean stream.
    async fn drain(&mut self, window: Duration) {
        loop {
            let next = tokio::time::timeout(window, self.ws.next()).await;
            match next {
                Ok(Some(Ok(Raw::Text(text)))) => {
                    if let Ok(frame) = serde_json::from_str::<Value>(text.as_str()) {
                        self.pending.push(frame);
                    }
                }
                _ => break,
            }
        }
    }

    /// Assert no frame arrives inside this window (documented silent paths).
    async fn expect_quiet_for(&mut self, window: Duration, describe: &str) {
        if let Ok(next) = tokio::time::timeout(window, self.ws.next()).await {
            match next {
                Some(Ok(Raw::Text(text))) => {
                    let frame: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                    panic!(
                        "{}: unexpected frame {frame} while expecting {describe}",
                        self.name
                    );
                }
                Some(Ok(_)) => {}
                Some(Err(err)) => panic!("{}: socket error: {err}", self.name),
                None => panic!("{}: socket closed while expecting {describe}", self.name),
            }
        }
    }

    async fn close(mut self) {
        let _ = self.ws.close(None).await;
        while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await
        {
            if msg.is_err() || matches!(msg, Ok(Raw::Close(_))) {
                break;
            }
        }
    }
}

/// A minted session token for an existing player identity.
fn mint(uid: Uuid, username: &str) -> String {
    engine_api::infra::jwt::mint_token_pair(uid, username, HashMap::new()).0
}

fn uid_of(player: &Player) -> Uuid {
    Uuid::parse_str(&player.uid).expect("player uid is a uuid")
}

/// Session ids travel as simple hex; re-spell them the way a client holding a
/// `Uuid` would.
fn normalized_sid(sid: &str) -> String {
    Uuid::parse_str(sid)
        .map(|uuid| uuid.to_string())
        .unwrap_or_else(|_| sid.to_owned())
}

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

async fn match_plane(a: &mut WsClient, b: &mut WsClient) -> String {
    eprintln!("ws-e2e: match plane");

    // Creation is answered with the id on the request cid.
    let cid = a
        .send_event(
            "match_create",
            json!({"name": "room-e2e-a", "open": true, "tick_rate": 30, "label": "room-e2e-a"}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("match_create_ack"), "match create")
        .await;
    let match_id = ack["match_id"].as_str().expect("match id").to_owned();

    // Creating a match does not seat its creator: the resident joins by id, and its
    // reply carries no other presences because the match is still empty.
    let cid_seat = a
        .send_event("match_join", json!({"match_id": match_id.clone()}))
        .await;
    let seated = a
        .take_reply(&cid_seat, Some("match_join"), "seat the creator")
        .await;
    assert_eq!(seated["match_id"].as_str(), Some(match_id.as_str()));
    assert!(
        seated.get("presences").is_none(),
        "an empty match answers the first join with no presences: {seated}"
    );

    // Join by match_id: self reply, presence event for the resident.
    let cid_join = b
        .send_event("match_join", json!({"match_id": match_id.clone()}))
        .await;
    let joined = b
        .take_reply(&cid_join, Some("match_join"), "join by id")
        .await;
    assert_eq!(joined["match_id"].as_str(), Some(match_id.as_str()));
    assert_eq!(
        joined["presences"]
            .as_array()
            .map(|rows| rows.len())
            .unwrap_or_default(),
        1,
        "the joiner is told who was already in the match: {joined}"
    );
    let presence = a.take_event("match_presence_event").await;
    assert_eq!(presence["match_id"].as_str(), Some(match_id.as_str()));
    assert!(
        !presence["joins"]
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .is_empty(),
        "the resident hears about the join: {presence}"
    );

    // Broadcast data: the peer receives it, the sender gets no ack.
    a.fire(
        "match_data_send",
        json!({"match_id": match_id.clone(), "op_code": 3, "data": "hello-b"}),
    )
    .await;
    let data = b.take_event("match_data").await;
    assert_eq!(data["op_code"], 3);
    assert_eq!(data["data"], "hello-b");
    assert_eq!(data["match_id"].as_str(), Some(match_id.as_str()));
    a.expect_quiet_for(Duration::from_millis(300), "no ack to the data sender")
        .await;

    // WebRTC signal relay: full payload to the peer, empty echo to the sender.
    let cid_sig = a
        .send_event(
            "match_signal",
            json!({"match_id": match_id.clone(), "data": "{\"offer\":\"sdp\"}"}),
        )
        .await;
    let echoed = a
        .take_reply(&cid_sig, Some("match_signal"), "signal echo")
        .await;
    let relayed = b.take_event("match_signal").await;
    assert_eq!(relayed["data"], "{\"offer\":\"sdp\"}");
    assert!(
        echoed["data"].as_str().unwrap_or("").is_empty(),
        "the sender's echo is empty: {echoed}"
    );

    // Leave: self ack, presence event for the peer, data refused afterwards.
    let cid_leave = a
        .send_event("match_leave", json!({"match_id": match_id.clone()}))
        .await;
    let left = a.take_reply(&cid_leave, Some("match_leave"), "leave").await;
    assert_eq!(left["match_id"].as_str(), Some(match_id.as_str()));
    let gone = b.take_event("match_presence_event").await;
    assert!(
        !gone["leaves"]
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .is_empty(),
        "the peer hears the leave: {gone}"
    );
    let cid_after = a
        .send_event(
            "match_data_send",
            json!({"match_id": match_id.clone(), "op_code": 1, "data": "late"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid_after, "data after leave").await,
        "must join the match before sending data"
    );

    // Error shapes.
    a.drain(Duration::from_millis(100)).await;
    let cid_unknown = b
        .send_event(
            "match_data_send",
            json!({"match_id": Uuid::new_v4().to_string(), "op_code": 1, "data": "x"}),
        )
        .await;
    assert_eq!(
        b.take_ws_error(&cid_unknown, "data on unknown match").await,
        "match not found"
    );
    let cid_bad_token = b
        .send_event("match_join", json!({"token": "deadbeefdeadbeef"}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_bad_token, "join with an unknown token")
            .await,
        "match token invalid or expired"
    );
    let cid_no_selector = b.send_event("match_join", json!({})).await;
    assert_eq!(
        b.take_ws_error(&cid_no_selector, "join with no selector")
            .await,
        "match_join needs match_id, token or label"
    );
    let cid_bad_id = b
        .send_event("match_leave", json!({"match_id": "not-a-uuid"}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_bad_id, "leave with a malformed id")
            .await,
        "match not found"
    );

    // Join by an unknown label auto-creates an open match under it.
    let label = format!("e2e label {}", letters_suffix());
    let cid_label = b
        .send_event("match_join", json!({"label": label.clone()}))
        .await;
    let by_label = b
        .take_reply(&cid_label, Some("match_join"), "join by label")
        .await;
    assert!(
        by_label["match_id"].is_string(),
        "a label join resolves to a match: {by_label}"
    );
    match_id
}

async fn matchmaker_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: matchmaker plane");
    let props = json!({"mode": "e2ews"});
    let query = json!("+properties.mode:e2ews");
    let cid_a = a
        .send_event(
            "matchmaker_add",
            json!({"min_count": 2, "max_count": 2, "query": query.clone(), "string_properties": props.clone()}),
        )
        .await;
    let ticket = a
        .take_reply(&cid_a, Some("matchmaker_ticket"), "ticket a")
        .await;
    let ticket_a = ticket["ticket"].as_str().expect("ticket a").to_owned();
    assert!(!ticket_a.is_empty());

    // Out-of-range bounds are refused on the request cid.
    let cid_min = a
        .send_event("matchmaker_add", json!({"min_count": 1, "max_count": 9}))
        .await;
    let message = a.take_ws_error(&cid_min, "ticket out of bounds").await;
    assert!(
        message.contains("min_count and max_count"),
        "unexpected bounds refusal: {message}"
    );

    // The 5-second pool tick pairs compatible tickets and pushes the match
    // with a join token to each side.
    let cid_b = b
        .send_event(
            "matchmaker_add",
            json!({"min_count": 2, "max_count": 2, "query": query, "string_properties": props}),
        )
        .await;
    b.take_reply(&cid_b, Some("matchmaker_ticket"), "ticket b")
        .await;
    let matched_a = a.take_event("matchmaker_matched").await;
    let matched_b = b.take_event("matchmaker_matched").await;
    let match_id = matched_a["match_id"].as_str().expect("matched match id");
    assert_eq!(matched_b["match_id"].as_str(), Some(match_id));

    let token_a = matched_a["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            matched_a["users"]
                .as_array()
                .map(|rows| rows.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|u| u["presence"]["user_id"].as_str() == Some(&a.uid.simple().to_string()))
                .find_map(|u| u["token"].as_str())
        })
        .expect("matchmaker_matched carries a join token")
        .to_owned();
    let token_b = matched_b["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            matched_b["users"]
                .as_array()
                .map(|rows| rows.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|u| u["presence"]["user_id"].as_str() == Some(&b.uid.simple().to_string()))
                .find_map(|u| u["token"].as_str())
        })
        .expect("the second side carries its own token")
        .to_owned();

    // Each side joins the ephemeral match on its own token.
    let cid_join = a.send_event("match_join", json!({"token": token_a})).await;
    let joined = a
        .take_reply(&cid_join, Some("match_join"), "join by token")
        .await;
    assert_eq!(joined["match_id"].as_str(), Some(match_id));
    let cid_join = b.send_event("match_join", json!({"token": token_b})).await;
    b.take_reply(&cid_join, Some("match_join"), "join b by token")
        .await;
    a.drain(Duration::from_millis(200)).await;
    b.drain(Duration::from_millis(200)).await;

    // Leaving the token-only match releases both sides (it is ephemeral).
    a.fire("match_leave", json!({"match_id": match_id.to_owned()}))
        .await;
    b.fire("match_leave", json!({"match_id": match_id.to_owned()}))
        .await;
    a.drain(Duration::from_millis(300)).await;
    b.drain(Duration::from_millis(300)).await;

    // A fresh ticket can be queued and pulled back out of the pool.
    let cid_add = a
        .send_event(
            "matchmaker_add",
            json!({"min_count": 8, "max_count": 8, "query": "+properties.mode:e2ews-solo", "string_properties": {"mode": "e2ews-solo"}}),
        )
        .await;
    let ticket = a
        .take_reply(&cid_add, Some("matchmaker_ticket"), "solo ticket")
        .await;
    let ticket_solo = ticket["ticket"].as_str().expect("ticket").to_owned();
    let cid_remove = a
        .send_event("matchmaker_remove", json!({"ticket": ticket_solo.clone()}))
        .await;
    let done = a
        .take_reply(&cid_remove, Some("matchmaker_done"), "ticket removed")
        .await;
    assert_eq!(done["type"], "matchmaker_done");
    let cid_again = a
        .send_event("matchmaker_remove", json!({"ticket": ticket_solo}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid_again, "removing a gone ticket").await,
        "matchmaker ticket not found"
    );
    let cid_bad = a
        .send_event("matchmaker_remove", json!({"ticket": "nope"}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid_bad, "removing a malformed ticket")
            .await,
        "matchmaker ticket not found"
    );
}

async fn party_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: party plane");

    // Create: the full party object comes back on the request cid.
    let cid = a
        .send_event("party_create", json!({"max_size": 3, "open": true}))
        .await;
    let created = a.take_reply(&cid, Some("party"), "party create").await;
    let party_id = created["party"]["party_id"]
        .as_str()
        .expect("party id")
        .to_owned();
    let leader_sid = normalized_sid(
        created["party"]["leader"]["session_id"]
            .as_str()
            .expect("leader session id"),
    );

    // A second member joins by id; the resident hears the roster event.
    a.drain(Duration::from_millis(100)).await;
    let cid_join = b
        .send_event("party_join", json!({"party_id": party_id.clone()}))
        .await;
    let joined = b.take_reply(&cid_join, Some("party"), "party join").await;
    assert_eq!(
        joined["party"]["party_id"].as_str(),
        Some(party_id.as_str())
    );
    let member_event = a.take_event("party_member_join").await;
    assert_eq!(member_event["party_id"].as_str(), Some(party_id.as_str()));
    let joiner_sid = normalized_sid(
        member_event["joins"][0]["session_id"]
            .as_str()
            .expect("the joiner's session id"),
    );

    // Party data: peers hear it, the sender does not echo.
    b.fire(
        "party_data_send",
        json!({"party_id": party_id.clone(), "op_code": 5, "data": "hi"}),
    )
    .await;
    let data = a.take_event("party_data").await;
    assert_eq!(data["op_code"], 5);
    assert_eq!(data["data"], "hi");

    // Promotion: leader-only, and every member hears it, caller included.
    let cid_promote = b
        .send_event(
            "party_promote",
            json!({"party_id": party_id.clone(), "presence": {"user_id": a.uid.to_string(), "session_id": leader_sid.clone(), "username": a.user}}),
        )
        .await;
    assert_eq!(
        b.take_ws_error(&cid_promote, "promotion by a non-leader")
            .await,
        "only party leader can promote"
    );
    b.drain(Duration::from_millis(50)).await;
    a.fire(
        "party_promote",
        json!({"party_id": party_id.clone(), "presence": {"user_id": b.uid.to_string(), "session_id": joiner_sid.clone(), "username": b.user}}),
    )
    .await;
    a.drain(Duration::from_millis(100)).await;
    let promoted_b = b.take_event("party_promote").await;
    assert_eq!(promoted_b["party_id"].as_str(), Some(party_id.as_str()));
    assert_eq!(
        normalized_sid(
            promoted_b["presence"]["session_id"]
                .as_str()
                .expect("promoted session id")
        ),
        joiner_sid,
        "the new leader is announced"
    );

    // Update is leader-only and succeeds in silence.
    let cid_update = a
        .send_event(
            "party_update",
            json!({"party_id": party_id.clone(), "max_size": 2}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid_update, "update by the demoted ex-leader")
            .await,
        "only party leader can update party"
    );
    b.fire(
        "party_update",
        json!({"party_id": party_id.clone(), "open": true}),
    )
    .await;
    b.expect_quiet_for(Duration::from_millis(300), "a silent successful update")
        .await;
    a.drain(Duration::from_millis(100)).await;

    // Close: both members hear it with the metadata, and the party is gone.
    b.fire(
        "party_close",
        json!({"party_id": party_id.clone(), "metadata": {"bye": true}}),
    )
    .await;
    let closed_a = a.take_event("party_close").await;
    let closed_b = b.take_event("party_close").await;
    assert_eq!(closed_a["metadata"]["bye"], true);
    assert_eq!(closed_b["type"], "party_close");

    // After the close, data sends and ticket removals are documented silences.
    b.fire(
        "party_data_send",
        json!({"party_id": party_id.clone(), "data": "after close"}),
    )
    .await;
    b.drain(Duration::from_millis(200)).await;
    a.drain(Duration::from_millis(200)).await;

    // A ticket add outside a party is refused as an error on the cid.
    let cid = a
        .send_event(
            "party_matchmaker_add",
            json!({"min_count": 2, "max_count": 2}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "party ticket with no party").await,
        "not a member of a party"
    );
    let cid = a
        .send_event(
            "party_matchmaker_remove",
            json!({"party_id": Uuid::new_v4().to_string(), "ticket": Uuid::new_v4().simple().to_string()}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "party ticket remove outside a party")
            .await,
        "not a member of this party"
    );

    // A private party refuses joiners.
    let cid = a
        .send_event("party_create", json!({"max_size": 2, "open": false}))
        .await;
    let created = a
        .take_reply(&cid, Some("party"), "private party create")
        .await;
    let private_id = created["party"]["party_id"]
        .as_str()
        .expect("private party id")
        .to_owned();
    a.drain(Duration::from_millis(100)).await;
    let cid_join = b
        .send_event("party_join", json!({"party_id": private_id.clone()}))
        .await;
    assert_eq!(
        b.take_ws_error(&cid_join, "joining a private party").await,
        "party is private"
    );
    let cid_close = a
        .send_event("party_close", json!({"party_id": private_id}))
        .await;
    a.take_event("party_close").await;
    a.drain(Duration::from_millis(100)).await;
    let _ = cid_close;

    // Leaving a party that does not resolve is silent (documented no-reply).
    b.fire(
        "party_leave",
        json!({"party_id": Uuid::new_v4().to_string()}),
    )
    .await;
    b.drain(Duration::from_millis(200)).await;
    a.drain(Duration::from_millis(100)).await;
    b.drain(Duration::from_millis(100)).await;
}

async fn chat_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: chat plane (room channel, ephemeral)");

    // A malformed channel id is refused; a room channel opens for anyone.
    let cid = a
        .send_event("channel_join", json!({"channel_id": "bogus"}))
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "joining a malformed channel").await,
        "Invalid channel ID."
    );
    let cid = a
        .send_event("channel_join", json!({"channel_id": "2...lobby"}))
        .await;
    let joined_a = a.take_reply(&cid, Some("channel"), "channel join a").await;
    assert_eq!(joined_a["channel"]["id"], "2...lobby");
    assert_eq!(joined_a["channel"]["room_name"], "lobby");

    // Second member: sees the roster, the resident sees the arrival.
    let cid = b
        .send_event("channel_join", json!({"channel_id": "2...lobby"}))
        .await;
    let joined_b = b.take_reply(&cid, Some("channel"), "channel join b").await;
    assert_eq!(
        joined_b["presences"].as_array().map(Vec::len),
        Some(1),
        "the roster already holds a: {joined_b}"
    );
    let arrival = a.take_event("channel_presence_event").await;
    assert_eq!(arrival["channel_id"], "2...lobby");
    assert_eq!(arrival["joins"].as_array().map(Vec::len), Some(1));

    // Ephemeral message: ack to the sender, broadcast to the member.
    let cid = b
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": {"body": "ping"}, "ephemeral": true}),
        )
        .await;
    let ack = b
        .take_reply(&cid, Some("channel_message_ack"), "ephemeral message")
        .await;
    assert_eq!(ack["code"], 0);
    let message = a.take_event("channel_message").await;
    assert_eq!(message["code"], 0);
    assert!(
        message["content"]
            .as_str()
            .unwrap_or_default()
            .contains("ping"),
        "content travels stringified: {message}"
    );

    // Stream data: unreliable sends echo only to peers.
    a.fire(
        "stream_data",
        json!({"stream_id": "2...lobby", "data": {"input": "w"}, "reliable": false}),
    )
    .await;
    let stream = b.take_event("stream_data").await;
    assert_eq!(stream["stream_id"], "2...lobby");
    assert_eq!(stream["data"], json!({"input": "w"}));
    let cid = b
        .send_event(
            "stream_data",
            json!({"stream_id": "2...lobby", "data": {"input": "j"}, "reliable": true}),
        )
        .await;
    let echo = b
        .take_reply(&cid, Some("stream_data"), "reliable stream echo")
        .await;
    assert_eq!(echo["reliable"], true);
    a.take_event("stream_data").await;

    // Leave is always acked; the resident hears it; unknown channels too.
    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...lobby"}))
        .await;
    let left = a
        .take_reply(&cid, Some("channel_leave"), "channel leave")
        .await;
    assert_eq!(left["channel_id"], "2...lobby");
    let departure = b.take_event("channel_presence_event").await;
    assert_eq!(departure["leaves"].as_array().map(Vec::len), Some(1));
    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...ghost"}))
        .await;
    a.take_reply(&cid, Some("channel_leave"), "leaving an unknown channel")
        .await;

    // An ephemeral room stays reachable while a member holds it (parity: membership
    // is enforced for persistent channels only), so a departed sender is still acked
    // and the fan-out skips it — the message lands on the remaining member.
    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": "x", "ephemeral": true}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("channel_message_ack"), "send after leaving")
        .await;
    assert_eq!(ack["code"], 0);
    let overheard = b.take_event("channel_message").await;
    assert_eq!(overheard["username"], a.user.as_str());
    a.expect_quiet_for(
        Duration::from_millis(200),
        "the sender never hears its own message",
    )
    .await;

    // The last member leaves, the room is dropped, and the send is refused.
    let cid = b
        .send_event("channel_leave", json!({"channel_id": "2...lobby"}))
        .await;
    b.take_reply(&cid, Some("channel_leave"), "last member leaves")
        .await;
    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby", "content": "x", "ephemeral": true}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "message on a room nobody holds")
            .await,
        "must join the channel before sending messages"
    );
    // So is data on a stream nobody joined.
    let cid = a
        .send_event(
            "stream_data",
            json!({"stream_id": "2...ghost", "data": {"input": "x"}}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "stream data without joining").await,
        "must join the stream before sending data"
    );
}

async fn presence_plane(a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: presence plane");

    // Follow by id resolves against the database (both players exist there).
    let cid = a
        .send_event("status_follow", json!({"user_ids": [b.uid.to_string()]}))
        .await;
    let followed = a
        .take_reply(&cid, Some("status_followed"), "follow by id")
        .await;
    assert_eq!(
        followed["user_ids"].as_array().map(Vec::len),
        Some(1),
        "the followed id comes back: {followed}"
    );

    // The update itself is answered to nobody: only followers hear it.
    b.fire("status_update", json!({"status": "in-game"})).await;
    let event = a.take_event("status_presence_event").await;
    assert_eq!(event["joins"].as_array().map(Vec::len), Some(1));
    assert_eq!(event["joins"][0]["status"], "in-game");
    b.expect_quiet_for(Duration::from_millis(200), "no reply to a status update")
        .await;

    // Follow by username resolves the name through the database. The reply lists
    // exactly this request's matches, so an unknown name answers empty — the
    // earlier follow stays registered, as the next update proves.
    let cid = a
        .send_event("status_follow", json!({"usernames": ["nobody-e2e"]}))
        .await;
    let followed = a
        .take_reply(&cid, Some("status_followed"), "follow an unknown name")
        .await;
    assert_eq!(
        followed["user_ids"].as_array().map(Vec::len),
        Some(0),
        "nothing resolves: {followed}"
    );
    b.fire("status_update", json!({"status": "queued"})).await;
    let held = a.take_event("status_presence_event").await;
    assert_eq!(held["joins"][0]["status"], "queued");
    assert!(
        held.get("cid").is_none(),
        "a fan-out carries no request cid: {held}"
    );

    // Unfollow answers with the emptied list.
    let cid = a
        .send_event("status_unfollow", json!({"user_ids": [b.uid.to_string()]}))
        .await;
    let unfollowed = a
        .take_reply(&cid, Some("status_followed"), "unfollow")
        .await;
    assert_eq!(unfollowed["user_ids"].as_array().map(Vec::len), Some(0));
    b.fire("status_update", json!({"status": "idle"})).await;
    a.expect_quiet_for(Duration::from_millis(400), "no events after unfollow")
        .await;
}

async fn notifications_plane(env: &Harness, a: &mut WsClient, player: &Player) {
    eprintln!("ws-e2e: notifications plane");
    let note_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO notification (id, user_id, subject, content, code, sender_id) \
         VALUES ($1, $2, $3, '{}'::jsonb, 7, $4)",
    )
    .bind(note_id)
    .bind(uid_of(player))
    .bind("ws-e2e notification")
    .bind(Uuid::new_v4())
    .execute(&env.pool)
    .await
    .expect("seed a notification");

    let cid = a.send_event("notifications", json!({"limit": 50})).await;
    let list = a.take_reply(&cid, None, "notifications list").await;
    assert_eq!(
        list["type"], "notifications_list",
        "the reply tag differs from the request tag"
    );
    let simple = note_id.simple().to_string();
    let hit = list["notifications"]
        .as_array()
        .map(|rows| rows.as_slice())
        .unwrap_or(&[])
        .iter()
        .find(|n| {
            n["id"]
                .as_str()
                .unwrap_or_default()
                .replace('-', "")
                .eq_ignore_ascii_case(&simple)
        })
        .unwrap_or_else(|| panic!("the seeded notification is listed: {list}"));
    assert_eq!(hit["subject"], "ws-e2e notification");
    assert_eq!(hit["code"], 7);
}

async fn channel_messages_plane(env: &Harness, a: &mut WsClient, b: &mut WsClient) {
    eprintln!("ws-e2e: persistent channel messages (database-backed)");

    // Update before join is refused.
    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": Uuid::new_v4().to_string(), "content": "x"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "update before joining").await,
        "must join the channel before updating messages"
    );

    let cid = a
        .send_event("channel_join", json!({"channel_id": "2...lobby-db"}))
        .await;
    a.take_reply(&cid, Some("channel"), "join the db channel")
        .await;
    let cid = b
        .send_event("channel_join", json!({"channel_id": "2...lobby-db"}))
        .await;
    b.take_reply(&cid, Some("channel"), "join the db channel (b)")
        .await;

    // The persistent write lands in `message` and acks with a message id.
    let cid = a
        .send_event(
            "channel_message_send",
            json!({"channel_id": "2...lobby-db", "content": {"body": "stored"}}),
        )
        .await;
    let ack = a
        .take_reply(&cid, Some("channel_message_ack"), "persistent message ack")
        .await;
    assert_eq!(ack["code"], 0);
    let message_id = ack["message_id"].as_str().expect("message id").to_owned();
    // Room channels persist as (mode 2, empty subject, empty descriptor, label).
    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM message \
         WHERE stream_mode = 2 AND stream_subject = '00000000-0000-0000-0000-000000000000'::uuid \
           AND stream_descriptor = '00000000-0000-0000-0000-000000000000'::uuid \
           AND stream_label = $1",
    )
    .bind("lobby-db")
    .fetch_one(&env.pool)
    .await
    .expect("the message table is readable");
    assert!(
        stored >= 1,
        "the persistent message is stored: {message_id}"
    );

    // The peer hears the broadcast.
    let message = b.take_event("channel_message").await;
    assert_eq!(message["code"], 0);

    // Update and remove travel on the same rails, with code 1 and 2.
    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": message_id.clone(), "content": {"body": "edited"}}),
        )
        .await;
    let updated = a
        .take_reply(&cid, Some("channel_message_ack"), "update ack")
        .await;
    assert_eq!(updated["code"], 1);
    b.take_event("channel_message").await;

    let cid = a
        .send_event(
            "channel_message_remove",
            json!({"channel_id": "2...lobby-db", "message_id": message_id}),
        )
        .await;
    let removed = a
        .take_reply(&cid, Some("channel_message_ack"), "remove ack")
        .await;
    assert_eq!(removed["code"], 2);

    // A malformed message id is refused with its own wording.
    let cid = a
        .send_event(
            "channel_message_update",
            json!({"channel_id": "2...lobby-db", "message_id": "not-a-uuid", "content": "x"}),
        )
        .await;
    assert_eq!(
        a.take_ws_error(&cid, "update with a malformed id").await,
        "Invalid message ID."
    );

    let cid = a
        .send_event("channel_leave", json!({"channel_id": "2...lobby-db"}))
        .await;
    a.take_reply(&cid, Some("channel_leave"), "leave the db channel")
        .await;
    let cid = b
        .send_event("channel_leave", json!({"channel_id": "2...lobby-db"}))
        .await;
    b.take_reply(&cid, Some("channel_leave"), "leave the db channel (b)")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn realtime_socket_every_event_end_to_end() {
    let env = Harness::start().await;

    // Distinct identities so member/self fan-out can be told apart.
    let alice = device_player(&env.client, &env.base, "ws-al").await;
    let bob = device_player(&env.client, &env.base, "ws-bb").await;
    let carol = device_player(&env.client, &env.base, "ws-cr").await;

    let mut a = WsClient::connect(
        env.port,
        &mint(uid_of(&alice), &alice.username),
        "a",
        uid_of(&alice),
        &alice.username,
    )
    .await;
    let mut b = WsClient::connect(
        env.port,
        &mint(uid_of(&bob), &bob.username),
        "b",
        uid_of(&bob),
        &bob.username,
    )
    .await;

    let created_id = match_plane(&mut a, &mut b).await;

    // The REST read side prints exactly these registries.
    let list = json_body(
        env.client
            .get(format!("{}/v2/match", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match lists relay matches",
    )
    .await;
    let ids: Vec<&str> = list["matches"]
        .as_array()
        .expect("the registry is populated after the match plane")
        .iter()
        .filter_map(|m| m["match_id"].as_str())
        .collect();
    assert!(
        ids.contains(&created_id.as_str()),
        "the created match is listed: {ids:?}"
    );
    let label_only = json_body(
        env.client
            .get(format!("{}/v2/match?min_size=1", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/match filters by size",
    )
    .await;
    assert!(
        !label_only["matches"].as_array().unwrap().is_empty(),
        "at least the label match still has a member"
    );
    refusal(
        env.client
            .get(format!("{}/v2/match?limit=0", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "GET /v2/match validates limit",
    )
    .await;
    let stats = json_body(
        env.client
            .get(format!("{}/v2/matchmaker/stats", env.base))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/matchmaker/stats",
    )
    .await;
    assert_eq!(stats["ticket_count"], 0, "no tickets are queued yet");

    matchmaker_plane(&mut a, &mut b).await;
    party_plane(&mut a, &mut b).await;
    chat_plane(&mut a, &mut b).await;
    presence_plane(&mut a, &mut b).await;
    channel_messages_plane(&env, &mut a, &mut b).await;

    // Carol gets her own socket for the database-backed notification list.
    let mut c = WsClient::connect(
        env.port,
        &mint(uid_of(&carol), &carol.username),
        "c",
        uid_of(&carol),
        &carol.username,
    )
    .await;
    notifications_plane(&env, &mut c, &carol).await;

    c.close().await;
    b.close().await;
    a.close().await;
    env.stop().await;
    eprintln!("ws-e2e: every /ws event tag passed");
}
