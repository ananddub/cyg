use super::*;

pub const CONSOLE_USER: &str = "e2e-admin";
pub const CONSOLE_PASSWORD: &str = "e2e-password";
pub const SERVER_KEY: &str = "e2e-server-key";
pub const GAME_NAME: &str = "e2e-game";
pub struct Harness {
    pub base: String,
    pub port: u16,
    pub client: Client,
    pub pool: Option<Arc<Datastore>>,
    pub blobs: Option<Arc<BlobStore>>,
    pub bus: Arc<Bus>,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub server: tokio::task::JoinHandle<()>,
}
impl Harness {
    pub async fn start() -> Harness {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind an ephemeral API port");
        let port = listener.local_addr().expect("listener address").port();

        let control = ControlConfig {
            api_port: port,
            seat_secret: "e2e-seat-secret".into(),
            rate_limit_rps: 1000,
            rate_limit_burst: 1000,
            session: SessionConfig {
                database_endpoint: "mem://".into(),
                signing_key: "e2e-signing-key".into(),
                refresh_signing_key: "e2e-refresh-signing-key".into(),
                server_key: SERVER_KEY.into(),
                token_expiry_secs: 3_600,
                refresh_expiry_secs: 7_200,
                ..SessionConfig::default()
            },
            console: ConsoleConfig {
                username: CONSOLE_USER.into(),
                password: CONSOLE_PASSWORD.into(),
                signing_key: "e2e-console-signing-key".into(),
                token_expiry_secs: 3_600,
            },
            iap: IapConfig {
                allow_unverified: true,
                ..IapConfig::default()
            },
            ..ControlConfig::default()
        };
        let game = GameConfig {
            name: GAME_NAME.into(),
            port: 7777,
            max_clients: 16,
            tick_rate: 20,
            ..GameConfig::default()
        };
        let bus = Bus::new(64, 64);
        let state = engine_api::ApiState::new(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            &game,
            &control,
            bus.clone(),
            port,
        )
        .await;
        let pool = state.db.clone();
        let blobs = state.blobs.clone();

        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            engine_api::serve(listener, state, async move {
                let _ = shutdown_rx.await;
            })
            .await;
        });

        let base = format!("http://127.0.0.1:{port}");
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("build the HTTP client");
        let env = Harness {
            base,
            port,
            client,
            pool,
            blobs,
            bus,
            shutdown,
            server,
        };
        env.wait_until_ready().await;
        env
    }

    async fn wait_until_ready(&self) {
        for _ in 0..200 {
            if let Ok(resp) = self
                .client
                .get(format!("{}/auto/health", self.base))
                .send()
                .await
                && resp.status().is_success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the API did not answer /auto/health within 10s");
    }

    pub fn db(&self) -> &Datastore {
        self.pool
            .as_ref()
            .expect("the harness datastore opened (database_endpoint = mem://)")
    }

    pub fn blobs(&self) -> &BlobStore {
        self.blobs
            .as_ref()
            .expect("the harness blob store opened (blobs defaults to memory:// under mem://)")
    }

    pub async fn stop(self) {
        let Harness {
            client,
            shutdown,
            server,
            ..
        } = self;
        drop(client);
        let _ = shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(10), server).await;
    }
}
pub async fn expect(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let actual = resp.status();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    body
}
pub async fn json_body(resp: reqwest::Response, status: StatusCode, what: &str) -> Value {
    let body = expect(resp, status, what).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("{what}: response is not JSON ({err}): {body}"))
}
pub async fn refusal(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let body = json_body(resp, status, what).await;
    body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{what}: refusal has no message: {body}"))
        .to_owned()
}
