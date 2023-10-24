use axum::{
    async_trait,
    extract::rejection::TypedHeaderRejectionReason,
    extract::{FromRef, FromRequestParts},
    headers::{self, authorization::Bearer, Authorization},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    RequestPartsExt, TypedHeader,
};

use tracing::error;

use crate::db::{DbError, ProxifierDb, SqlxDb};

/// Errors during authentication.
#[derive(Debug, thiserror::Error)]
pub enum AuthenticationError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Database error: {0}")]
    DbError(DbError),
}

impl IntoResponse for AuthenticationError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized(s) => {
                error!("{s}");
                StatusCode::UNAUTHORIZED.into_response()
            }
            Self::DbError(e) => {
                error!("{e}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

#[derive(Debug)]
pub struct AuthenticatedUser {
    pub api_key: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    SqlxDb: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthenticationError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // To complicated to have a bad request without info.
        // Return unauthorized for now.
        let bearer = extract_authorization_bearer(parts)
            .await
            .ok_or(AuthenticationError::Unauthorized("no bearer".to_string()))?;

        let api_key = bearer.token().to_string();

        let db = SqlxDb::from_ref(state);

        match db
            .user_from_api_key(&api_key)
            .await
            .map_err(AuthenticationError::DbError)?
        {
            Some(_u) => Ok(AuthenticatedUser { api_key }),
            None => Err(AuthenticationError::Unauthorized(format!(
                "API-KEY {api_key}"
            ))),
        }
    }
}

/// Extract authorization bearer from headers.
async fn extract_authorization_bearer(
    parts: &mut Parts,
) -> Option<TypedHeader<Authorization<Bearer>>> {
    match parts
        .extract::<TypedHeader<headers::Authorization<Bearer>>>()
        .await
    {
        Ok(bearer) => Some(bearer),
        Err(e) => match *e.name() {
            header::AUTHORIZATION => match e.reason() {
                TypedHeaderRejectionReason::Missing => None,
                _ => {
                    error!("unexpected error getting Authorization header(s): {}", e);
                    None
                }
            },
            _ => {
                error!("unexpected error getting authorization: {}", e);
                None
            }
        },
    }
}

struct DatabaseConnection(sqlx::AnyPool);

#[async_trait]
impl<S> FromRequestParts<S> for DatabaseConnection
where
    sqlx::AnyPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = sqlx::AnyPool::from_ref(state);

        let _conn = pool
            .acquire()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("conn error {e}")))?;

        Ok(Self(pool))
    }
}
