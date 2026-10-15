use auto_di::singleton;
use auto_socket::{auto_socket, on};
use axum::Router;
use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef},
};
use tokio::net::TcpListener;

struct ChatSocket;

#[singleton]
#[auto_socket("/chat")]
impl ChatSocket {
    fn new() -> Self {
        Self
    }

    #[on("message")]
    async fn message(&self, socket: SocketRef, Data(message): Data<String>) {
        let _ = socket.emit("message", &message);
    }
}

#[on("ping")]
async fn ping(socket: SocketRef) {
    let _ = socket.emit("pong", &"ok");
}

#[tokio::main]
async fn main() {
    let (layer, io) = SocketIo::new_layer();
    auto_socket::register_global(&io)
        .await
        .expect("failed to register socket handlers");

    let app = Router::new().layer(layer);
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
