use super::*;

pub struct WsClient {
    pub name: String,
    pub user: String,
    pub uid: Uuid,
    pub ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    pub n: u32,
    pub pending: Vec<Value>,
}
