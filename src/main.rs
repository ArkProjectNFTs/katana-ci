//! A Katana (reverse)proxifier, to easily spawn a new Katana
//! for CI purposes mainly.
//!
//! This proxifier uses docker to spin up a new instance of Katana
//! and then manage it internally using the name provided by the user.
//! This version is fully on-memory, and will drop every managed service
//! if killed.
use shiplift::{ContainerOptions, Docker};
use std::collections::HashMap;
use std::error::Error;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, trace};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod db;
use db::HashMapDb;

mod docker_manager;
use docker_manager::DockerManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging()?;

    let mut db = HashMapDb::new();
    let docker = DockerManager::new("katana");

    let id = docker.create(8899).await?;
    info!("id {}", id);

    sleep(Duration::from_millis(2000)).await;

    docker.start(&id).await?;

    sleep(Duration::from_millis(2000)).await;

    let logs = docker.logs(&id).await?;
    debug!("{}", logs);

    sleep(Duration::from_millis(10000)).await;

    sleep(Duration::from_millis(2000)).await;

    let logs = docker.logs(&id).await?;
    debug!("{}", logs);

    docker.remove(&id).await?;


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
