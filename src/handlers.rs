use axum::{
    body::Body,
    extract::{FromRef, Path, Query, State},
    http::{uri::Uri, Request, StatusCode},
    response::{IntoResponse, Response},
};

use serde::Deserialize;
use tracing::error;

use crate::db::{DbError, InstanceInfo, ProxifierDb, SqlxDb};
use crate::docker_manager::{DockerError, DockerManager, KatanaDockerOptions};
use crate::extractors::AuthenticatedUser;
use crate::{AppState, HttpClient};

impl From<DbError> for hyper::StatusCode {
    fn from(e: DbError) -> Self {
        error!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl From<DbError> for (hyper::StatusCode, String) {
    fn from(e: DbError) -> Self {
        error!("{}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

impl From<DockerError> for hyper::StatusCode {
    fn from(e: DockerError) -> Self {
        error!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl From<DockerError> for (hyper::StatusCode, String) {
    fn from(e: DockerError) -> Self {
        error!("{}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

#[derive(Deserialize)]
pub struct KatanaStartQueryParams {
    pub block_time: Option<u32>,
    pub no_mining: Option<bool>,
}

pub async fn start_katana(
    State(state): State<AppState>,
    Query(params): Query<KatanaStartQueryParams>,
    user: AuthenticatedUser,
) -> Result<String, StatusCode> {
    let mut db = SqlxDb::from_ref(&state);
    let docker = DockerManager::from_ref(&state);

    let port = db.get_free_port().await.expect("Impossible to get a port");

    let container_id = docker
        .create(&KatanaDockerOptions {
            block_time: params.block_time,
            no_mining: params.no_mining,
            port: port as u32,
        })
        .await?;

    docker.start(&container_id).await?;

    let name = crate::db::get_random_name();

    db.instance_add(&InstanceInfo {
        container_id,
        api_key: user.api_key.clone(),
        name: name.clone(),
        proxied_port: port,
    })
    .await?;

    Ok(name)
}

pub async fn stop_katana(
    State(state): State<AppState>,
    Path(name): Path<String>,
    _user: AuthenticatedUser,
) -> Result<Response, StatusCode> {
    let mut db = SqlxDb::from_ref(&state);
    let docker = DockerManager::from_ref(&state);

    let instance = db.instance_from_name(&name).await?;
    if instance.is_none() {
        return Ok((StatusCode::BAD_REQUEST, "Invalid name").into_response());
    }

    let instance = instance.unwrap();

    let force = true;
    docker.remove(&instance.container_id, force).await?;

    db.instance_rm(&instance.name).await?;

    Ok(().into_response())
}

pub async fn proxy_request_katana(
    State(state): State<AppState>,
    Path(name): Path<String>,
    mut req: Request<Body>,
) -> Result<Response, StatusCode> {
    let db = SqlxDb::from_ref(&state);
    let http = HttpClient::from_ref(&state);
    //let docker = DockerManager::from_ref(&state);

    let instance = db.instance_from_name(&name).await?;
    if instance.is_none() {
        return Ok((StatusCode::BAD_REQUEST, "Invalid name").into_response());
    }

    let instance = instance.unwrap();

    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);

    let uri = format!("http://127.0.0.1:{}{}", instance.proxied_port, path_query);

    *req.uri_mut() = Uri::try_from(uri).unwrap();

    Ok(http
        .request(req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .into_response())
}

#[derive(Deserialize)]
pub struct KatanaLogsQueryParams {
    pub n: Option<String>,
}

pub async fn logs_katana(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<KatanaLogsQueryParams>,
    _user: AuthenticatedUser,
) -> Result<String, (StatusCode, String)> {
    let db = SqlxDb::from_ref(&state);
    let docker = DockerManager::from_ref(&state);

    let n = params.n.unwrap_or("25".to_string());

    let instance = db.instance_from_name(&name).await?;
    if instance.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Invalid name".to_string()));
    }

    let instance = instance.unwrap();

    Ok(docker.logs(&instance.container_id, n).await?)
}
