use auto_route::{controller, get};
use axum::extract::Path;
use tokio::net::TcpListener;

struct GreetingController;

#[controller("/greetings")]
impl GreetingController {
    fn new() -> Self {
        Self
    }

    #[get("/:name")]
    async fn greeting(&self, Path(name): Path<String>) -> String {
        format!("Hello, {name}!")
    }
}

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let app = auto_route::routes().await.expect("failed to build routes");
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
