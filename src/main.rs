//! A Katana (reverse)proxifier, to easily spawn a new Katana
//! for CI purposes mainly.
//!
//! This proxifier uses docker to spin up a new instance of Katana
//! and then manage it internally using the name provided by the user.
//! This version is fully on-memory, and will drop every managed service
//! if killed.
use axum::{
    body::Body,
    extract::FromRef,
    routing::{get, post},
    Router, Server,
};
use hyper::client::HttpConnector;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead};
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod db;
use db::{ProxifierDb, SqlxDb};

mod docker_manager;
use docker_manager::DockerManager;

mod extractors;
mod handlers;

type HttpClient = hyper::client::Client<HttpConnector, Body>;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlxDb,
    pub docker: DockerManager,
    pub http: HttpClient,
}

impl FromRef<AppState> for SqlxDb {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl FromRef<AppState> for HttpClient {
    fn from_ref(state: &AppState) -> Self {
        state.http.clone()
    }
}

impl FromRef<AppState> for DockerManager {
    fn from_ref(state: &AppState) -> Self {
        state.docker.clone()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging()?;

    let docker_image = env::var("KATANA_CI_IMAGE").expect("KATANA_CI_IMAGE is not set");

    sqlx::any::install_default_drivers();

    let mut db = SqlxDb::new_any("sqlite::memory:").await?;

    sqlx::migrate!("./migrations")
        .run(db.get_pool_ref())
        .await?;

    load_users_from_env(&mut db).await;

    let docker = DockerManager::new(&docker_image);
    let http: HttpClient = hyper::Client::builder().build(HttpConnector::new());

    let state = AppState {
        db: db.clone(),
        http,
        docker,
    };

    let dev_cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    // build our application with a route
    let app = Router::new()
        .route("/start", get(handlers::start_katana))
        .route("/:name/stop", get(handlers::stop_katana))
        .route("/:name/logs", get(handlers::logs_katana))
        .route("/:name/katana", post(handlers::proxy_request_katana))
        .with_state(state)
        .layer(dev_cors);

    let ip = "127.0.0.1:5050";
    info!("{}", format!("📡 waiting for requests on http://{ip}..."));
    Server::bind(&ip.parse().unwrap())
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

fn init_logging() -> Result<(), Box<dyn Error>> {
    const DEFAULT_LOG_FILTER: &str = "info,katana=trace";

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env().or(EnvFilter::try_new(DEFAULT_LOG_FILTER))?,
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    Ok(())
}

async fn load_users_from_env(db: &mut SqlxDb) {
    let file_path = match env::var("KATANA_CI_USERS_FILE") {
        Ok(path) => path,
        Err(_) => {
            warn!("KATANA_CI_USERS_FILE not set, skipping default users");
            return;
        }
    };

    let file = match File::open(&file_path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Failed to open file: {}", err);
            std::process::exit(1);
        }
    };

    for line in io::BufReader::new(file).lines() {
        match line {
            Ok(contents) => {
                let parts: Vec<&str> = contents.split(',').collect();

                if parts.len() != 2 {
                    eprintln!("File should contain two comma-separated strings.");
                    std::process::exit(1);
                }

                let name = parts[0].trim();
                let api_key = parts[1].trim();

                match db.user_add(name, Some(api_key.to_string())).await {
                    Ok(_) => debug!("Default user {} added", name),
                    Err(e) => error!("Can't add default user {name}: {e}"),
                }
            }
            Err(err) => {
                eprintln!("Failed to read line: {}", err);
                std::process::exit(1);
            }
        }
    }
}
