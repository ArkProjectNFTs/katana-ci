//! Database abstraction to manage Katana instances.
//!
use async_trait::async_trait;
//use regex::Regex;
use sqlx::{sqlite::SqliteConnectOptions, Error as SqlxError, FromRow, SqlitePool};
use std::str::FromStr;
use tracing::trace;
use uuid::Uuid;

/// Errors for DB operations.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("An error occurred: {0}")]
    Generic(String),
    #[error("Entity already in the database: {0}")]
    AlreadyExists(String),
    #[error("SQLx error: {0}")]
    Sqlx(SqlxError),
}

/// Katana instance info, used to track
/// the spawned instances in docker containers.
/// The `name` of an instance must follow the `is_valid_instance_name` rules.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstanceInfo {
    pub container_id: String,
    pub proxied_port: u16,
    #[sqlx(rename = "instance_name")]
    pub name: String,
    pub api_key: String,
}

/// User's info.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserInfo {
    #[sqlx(rename = "user_name")]
    pub name: String,
    pub api_key: String,
}

pub fn get_random_name() -> String {
    let uid = Uuid::new_v4().to_string();
    uid.rsplit('-').next().unwrap().to_string()
}

///
// fn is_valid_instance_name(name: &str) -> bool {
//     let re = Regex::new(r"^[a-z0-9]+$").unwrap();
//     re.is_match(name)
// }

#[async_trait]
pub trait ProxifierDb: Send + Sync {
    async fn user_add(&mut self, name: &str, api_key: Option<String>) -> Result<UserInfo, DbError>;
    async fn user_from_api_key(&self, api_key: &str) -> Result<Option<UserInfo>, DbError>;
    async fn instance_from_name(&self, name: &str) -> Result<Option<InstanceInfo>, DbError>;
    async fn instance_add(&mut self, info: &InstanceInfo) -> Result<(), DbError>;
    async fn instance_rm(&mut self, name: &str) -> Result<(), DbError>;
    async fn is_port_in_use(&self, port: u16) -> Result<bool, DbError>;
}

impl From<SqlxError> for DbError {
    fn from(e: SqlxError) -> Self {
        DbError::Sqlx(e)
    }
}

/// Default implementation with SQLx.
#[derive(Debug, Clone)]
pub struct SqlxDb {
    pool: SqlitePool,
}

impl SqlxDb {
    pub fn get_pool_ref(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn new_any(_db_url: &str) -> Result<Self, DbError> {
        Ok(Self {
            pool: SqlitePool::connect_with(SqliteConnectOptions::from_str("sqlite:data.db")?)
                .await?,
        })
    }

    pub async fn get_free_port(&self) -> Option<u16> {
        trace!("checking for free port");

        loop {
            let port = rand::random::<u16>();
            if port > 10000 && port < 65000 {
                match self.is_port_in_use(port).await {
                    Ok(in_use) => {
                        if in_use {
                            trace!("port {port} in use");
                            continue;
                        } else {
                            trace!("free port found {port}");
                            return Some(port);
                        }
                    }
                    Err(_e) => return None,
                };
            }
        }
    }

    async fn get_instance_by_name(&self, name: &str) -> Result<Option<InstanceInfo>, DbError> {
        let q = "SELECT * FROM instance_info WHERE instance_name = ?;";

        match sqlx::query(q)
            .bind(name.to_string())
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => {
                if rows.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(InstanceInfo::from_row(&rows[0])?))
                }
            }
            Err(e) => Err(DbError::Sqlx(e)),
        }
    }

    async fn get_user_by_apikey(&self, api_key: &str) -> Result<Option<UserInfo>, DbError> {
        let q = "SELECT * FROM user_info WHERE api_key = ?;";

        match sqlx::query(q)
            .bind(api_key.to_string())
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => {
                if rows.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(UserInfo::from_row(&rows[0])?))
                }
            }
            Err(e) => Err(DbError::Sqlx(e)),
        }
    }
}

#[async_trait]
impl ProxifierDb for SqlxDb {
    async fn user_add(&mut self, name: &str, api_key: Option<String>) -> Result<UserInfo, DbError> {
        trace!("adding new user {name} with api_key {:?}", api_key);

        let name = name.to_string();
        let api_key = if let Some(k) = api_key {
            if (self.user_from_api_key(&k).await?).is_some() {
                return Err(DbError::AlreadyExists(format!("api-key {}", k)));
            } else {
                k
            }
        } else {
            Uuid::new_v4().to_string()
        };

        let info = UserInfo { name, api_key };

        let q = "INSERT INTO user_info (user_name, api_key) VALUES (?, ?);";

        let _r = sqlx::query(q)
            .bind(info.name.clone())
            .bind(info.api_key.clone())
            .execute(&self.pool)
            .await?;

        Ok(info)
    }

    async fn user_from_api_key(&self, api_key: &str) -> Result<Option<UserInfo>, DbError> {
        trace!("getting user from api_key {api_key}");
        self.get_user_by_apikey(api_key).await
    }

    async fn instance_from_name(&self, name: &str) -> Result<Option<InstanceInfo>, DbError> {
        trace!("getting instance from name {name}");
        self.get_instance_by_name(name).await
    }

    async fn instance_add(&mut self, info: &InstanceInfo) -> Result<(), DbError> {
        trace!("adding instance {:?}", info);

        if (self.instance_from_name(&info.name).await?).is_some() {
            return Err(DbError::AlreadyExists(format!(
                "Instance {} already exists",
                info.name
            )));
        }

        let q = "INSERT INTO instance_info (container_id, proxied_port, instance_name, api_key) VALUES (?, ?, ?, ?);";

        let _r = sqlx::query(q)
            .bind(info.container_id.clone())
            .bind(info.proxied_port)
            .bind(info.name.clone())
            .bind(info.api_key.clone())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn instance_rm(&mut self, name: &str) -> Result<(), DbError> {
        trace!("removing instance {name}");

        if (self.instance_from_name(name).await?).is_some() {
            let q = "DELETE FROM instance_info WHERE instance_name = ?;";
            sqlx::query(q)
                .bind(name.to_string())
                .fetch_all(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn is_port_in_use(&self, port: u16) -> Result<bool, DbError> {
        trace!("checking port {port}");

        let q = "SELECT * FROM instance_info WHERE proxied_port = ?;";

        Ok(!sqlx::query(q)
            .bind(port.to_string())
            .fetch_all(&self.pool)
            .await?
            .is_empty())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[tokio::test]
//     async fn test_hashmap_db_user_add() {
//         let mut db = HashMapDb::new();
//         let u = db.user_add("user1", None).await.unwrap();

//         assert_eq!(u.name, "user1");
//     }

//     #[tokio::test]
//     async fn test_hashmap_db_user_from_api_key() {
//         let mut db = HashMapDb::new();

//         let u = db.user_from_api_key("abcd").await.unwrap();
//         assert_eq!(u, None);

//         db.user_add("user1", Some("my-key".to_string()))
//             .await
//             .unwrap();

//         let u = db.user_from_api_key("my-key").await.unwrap();
//         assert_eq!(
//             u,
//             Some(UserInfo {
//                 name: "user1".to_string(),
//                 api_key: "my-key".to_string(),
//             })
//         );
//     }

//     #[tokio::test]
//     async fn test_hashmap_db_instance_add() {
//         let mut db = HashMapDb::new();
//         let i = InstanceInfo {
//             container_id: "1".to_string(),
//             api_key: "my-key".to_string(),
//             name: "test1".to_string(),
//             proxied_port: 1234,
//         };

//         db.instance_add(&i).await.unwrap();
//     }

//     #[tokio::test]
//     async fn test_hashmap_db_instance_from_name() {
//         let mut db = HashMapDb::new();

//         let db_i = db.instance_from_name("test1").await.unwrap();
//         assert_eq!(db_i, None);

//         let i = InstanceInfo {
//             container_id: "1".to_string(),
//             api_key: "my-key".to_string(),
//             name: "test1".to_string(),
//             proxied_port: 1234,
//         };

//         db.instance_add(&i).await.unwrap();

//         let db_i = db.instance_from_name("test1").await.unwrap();
//         assert_eq!(db_i, Some(i));
//     }

//     #[tokio::test]
//     async fn test_hashmap_db_instance_rm() {
//         let mut db = HashMapDb::new();

//         db.instance_rm("test1").await.unwrap();

//         let i = InstanceInfo {
//             container_id: "1".to_string(),
//             api_key: "my-key".to_string(),
//             name: "test1".to_string(),
//             proxied_port: 1234,
//         };

//         db.instance_add(&i).await.unwrap();

//         let db_i = db.instance_from_name("test1").await.unwrap();
//         assert_eq!(db_i, Some(i));

//         db.instance_rm("test1").await.unwrap();

//         let db_i = db.instance_from_name("test1").await.unwrap();
//         assert_eq!(db_i, None);
//     }

//     #[tokio::test]
//     async fn test_hashmap_db_is_port_in_use() {
//         let mut db = HashMapDb::new();

//         assert_eq!(db.is_port_in_use(1234).await.unwrap(), false);

//         let i = InstanceInfo {
//             container_id: "1".to_string(),
//             api_key: "my-key".to_string(),
//             name: "test1".to_string(),
//             proxied_port: 1234,
//         };

//         db.instance_add(&i).await.unwrap();

//         assert_eq!(db.is_port_in_use(1234).await.unwrap(), true);
//     }
// }
