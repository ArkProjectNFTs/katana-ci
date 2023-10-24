//! Database abstraction to manage Katana instances.
//!
use async_trait::async_trait;
use regex::Regex;
use std::collections::HashMap;
use uuid::Uuid;

/// Errors for DB operations.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("An error occurred: {0}")]
    Generic(String),
    #[error("Entity already in the database: {0}")]
    AlreadyExists(String),
}

/// Katana instance info, used to track
/// the spawned instances in docker containers.
/// The `name` of an instance must follow the `is_valid_instance_name` rules.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceInfo {
    pub container_id: String,
    pub proxied_port: u16,
    pub name: String,
    pub api_key: String,
}

/// User's info.
#[derive(Debug, Clone, PartialEq)]
pub struct UserInfo {
    pub name: String,
    pub api_key: String,
}

///
fn is_valid_instance_name(name: &str) -> bool {
    let re = Regex::new(r"^[a-z0-9]+$").unwrap();
    re.is_match(name)
}

#[async_trait]
pub trait ProxifierDb: Send + Sync {
    async fn user_add(&mut self, name: &str, api_key: Option<String>) -> Result<UserInfo, DbError>;
    async fn user_from_api_key(&self, api_key: &str) -> Result<Option<UserInfo>, DbError>;
    async fn instance_from_name(&self, name: &str) -> Result<Option<InstanceInfo>, DbError>;
    async fn instance_add(&mut self, info: &InstanceInfo) -> Result<(), DbError>;
    async fn instance_rm(&mut self, name: &str) -> Result<(), DbError>;
    async fn is_port_in_use(&self, port: u16) -> Result<bool, DbError>;
}

/// Default implementation fully in memory of the ProxifierDb.
pub enum AnyEntry {
    User(UserInfo),
    Instance(InstanceInfo),
}

pub struct HashMapDb {
    map: HashMap<String, AnyEntry>,
}

impl HashMapDb {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn get_user_key(&self, api_key: &str) -> String {
        format!("USER#{}", api_key)
    }

    fn get_instance_key(&self, name: &str) -> String {
        format!("INSTANCE#{}", name)
    }
}

#[async_trait]
impl ProxifierDb for HashMapDb {
    async fn user_add(&mut self, name: &str, api_key: Option<String>) -> Result<UserInfo, DbError> {
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

        let user_key = self.get_user_key(&api_key);

        let info = UserInfo { name, api_key };

        self.map.insert(user_key, AnyEntry::User(info.clone()));
        Ok(info)
    }

    async fn user_from_api_key(&self, api_key: &str) -> Result<Option<UserInfo>, DbError> {
        let user_key = self.get_user_key(&api_key);
        if let Some(AnyEntry::User(info)) = self.map.get(&user_key) {
            Ok(Some(info.clone()))
        } else {
            Ok(None)
        }
    }

    async fn instance_from_name(&self, name: &str) -> Result<Option<InstanceInfo>, DbError> {
        let instance_key = self.get_instance_key(&name);
        if let Some(AnyEntry::Instance(info)) = self.map.get(&instance_key) {
            Ok(Some(info.clone()))
        } else {
            Ok(None)
        }
    }

    async fn instance_add(&mut self, info: &InstanceInfo) -> Result<(), DbError> {
        if (self.instance_from_name(&info.name).await?).is_some() {
            return Err(DbError::AlreadyExists(format!(
                "Instance {} already exists",
                info.name
            )));
        }

        let instance_key = self.get_instance_key(&info.name);

        self.map
            .insert(instance_key, AnyEntry::Instance(info.clone()));
        Ok(())
    }

    async fn instance_rm(&mut self, name: &str) -> Result<(), DbError> {
        if (self.instance_from_name(name).await?).is_some() {
            let instance_key = self.get_instance_key(name);
            self.map.remove(&instance_key);
        }

        Ok(())
    }

    async fn is_port_in_use(&self, port: u16) -> Result<bool, DbError> {
        for (key, value) in self.map.iter() {
            if !key.starts_with("INSTANCE#") {
                continue;
            }

            if let AnyEntry::Instance(info) = value {
                if info.proxied_port == port {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hashmap_db_user_add() {
        let mut db = HashMapDb::new();
        let u = db.user_add("user1", None).await.unwrap();

        assert_eq!(u.name, "user1");
    }

    #[tokio::test]
    async fn test_hashmap_db_user_from_api_key() {
        let mut db = HashMapDb::new();

        let u = db.user_from_api_key("abcd").await.unwrap();
        assert_eq!(u, None);

        db.user_add("user1", Some("my-key".to_string()))
            .await
            .unwrap();

        let u = db.user_from_api_key("my-key").await.unwrap();
        assert_eq!(
            u,
            Some(UserInfo {
                name: "user1".to_string(),
                api_key: "my-key".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn test_hashmap_db_instance_add() {
        let mut db = HashMapDb::new();
        let i = InstanceInfo {
            container_id: "1".to_string(),
            api_key: "my-key".to_string(),
            name: "test1".to_string(),
            proxied_port: 1234,
        };

        db.instance_add(&i).await.unwrap();
    }

    #[tokio::test]
    async fn test_hashmap_db_instance_from_name() {
        let mut db = HashMapDb::new();

        let db_i = db.instance_from_name("test1").await.unwrap();
        assert_eq!(db_i, None);

        let i = InstanceInfo {
            container_id: "1".to_string(),
            api_key: "my-key".to_string(),
            name: "test1".to_string(),
            proxied_port: 1234,
        };

        db.instance_add(&i).await.unwrap();

        let db_i = db.instance_from_name("test1").await.unwrap();
        assert_eq!(db_i, Some(i));
    }

    #[tokio::test]
    async fn test_hashmap_db_instance_rm() {
        let mut db = HashMapDb::new();

        db.instance_rm("test1").await.unwrap();

        let i = InstanceInfo {
            container_id: "1".to_string(),
            api_key: "my-key".to_string(),
            name: "test1".to_string(),
            proxied_port: 1234,
        };

        db.instance_add(&i).await.unwrap();

        let db_i = db.instance_from_name("test1").await.unwrap();
        assert_eq!(db_i, Some(i));

        db.instance_rm("test1").await.unwrap();

        let db_i = db.instance_from_name("test1").await.unwrap();
        assert_eq!(db_i, None);
    }

    #[tokio::test]
    async fn test_hashmap_db_is_port_in_use() {
        let mut db = HashMapDb::new();

        assert_eq!(db.is_port_in_use(1234).await.unwrap(), false);

        let i = InstanceInfo {
            container_id: "1".to_string(),
            api_key: "my-key".to_string(),
            name: "test1".to_string(),
            proxied_port: 1234,
        };

        db.instance_add(&i).await.unwrap();

        assert_eq!(db.is_port_in_use(1234).await.unwrap(), true);
    }
}
