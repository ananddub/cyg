use super::*;

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
        let state = engine_api::ApiState::new(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            &game,
            &control,
            Bus::new(64, 64),
            port,
        )
        .await;
        let pool = state
            .db
            .clone()
            .expect("the mem:// datastore opens at state construction");

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
pub fn bind(params: &[(&str, Value)]) -> BTreeMap<String, Value> {
    params
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}
pub async fn scalar<T: DeserializeOwned>(
    db: &Datastore,
    surql: &str,
    params: &[(&str, Value)],
) -> Option<T> {
    let bound = bind(params);
    db.query_params::<T>(surql, &bound)
        .await
        .unwrap_or_else(|err| panic!("{surql}: {err}"))
        .into_iter()
        .next()
}
pub async fn row(db: &Datastore, surql: &str, params: &[(&str, Value)]) -> Option<Value> {
    scalar::<Value>(db, surql, params).await
}
pub async fn count(
    db: &Datastore,
    table: &str,
    filter: Option<&str>,
    params: &[(&str, Value)],
) -> i64 {
    let surql = match filter {
        Some(filter) => format!("RETURN array::len((SELECT id FROM {table} WHERE {filter}))"),
        None => format!("RETURN array::len((SELECT id FROM {table}))"),
    };
    scalar::<i64>(db, &surql, params).await.unwrap_or(0)
}
pub async fn exec(db: &Datastore, surql: &str, params: &[(&str, Value)]) {
    let bound = bind(params);
    db.exec_params(surql, &bound)
        .await
        .unwrap_or_else(|err| panic!("{surql}: {err}"));
}
pub async fn seed_board(pool: &Datastore, id: &str) {
    seed_board_opts(pool, id, 1, false, true, 0, 0, false).await;
}
