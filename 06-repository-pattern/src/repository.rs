use sqlx::PgPool;

use crate::domain::{Email, UserName};
use crate::{NewUser, User};

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("email {0} is already taken")]
    EmailTaken(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
}

pub trait UserRepository: Clone + Send + Sync + 'static {
    fn create(&self, new_user: NewUser) -> impl Future<Output = Result<User, RepoError>> + Send;
    fn list(
        &self,
        name: Option<String>,
    ) -> impl Future<Output = Result<Vec<User>, RepoError>> + Send;
    fn find(&self, id: i64) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn update(
        &self,
        id: i64,
        new_user: NewUser,
    ) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn delete(&self, id: i64) -> impl Future<Output = Result<bool, RepoError>> + Send;
}

#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for PgUserRepository {
    async fn create(&self, new_user: NewUser) -> Result<User, RepoError> {
        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (name, email) VALUES ($1, $2)
            RETURNING id, name AS "name: UserName", email AS "email: Email""#,
            new_user.name.as_str(),
            new_user.email.as_str()
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|err| email_taken(err, &new_user.email))?;
        Ok(user)
    }

    async fn list(&self, name: Option<String>) -> Result<Vec<User>, RepoError> {
        let users = sqlx::query_as!(
            User,
            r#"SELECT id, name AS "name: UserName", email AS "email: Email" FROM users
            WHERE $1::text IS NULL OR name ILIKE '%' || $1 || '%'
            ORDER BY id"#,
            name
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }

    async fn find(&self, id: i64) -> Result<Option<User>, RepoError> {
        let user = sqlx::query_as!(
            User,
            r#"SELECT id, name AS "name: UserName", email AS "email: Email" FROM users
            WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    async fn update(&self, id: i64, new_user: NewUser) -> Result<Option<User>, RepoError> {
        let user = sqlx::query_as!(
            User,
            r#"UPDATE users SET name = $1, email = $2 WHERE id = $3
            RETURNING id, name AS "name: UserName", email AS "email: Email""#,
            new_user.name.as_str(),
            new_user.email.as_str(),
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| email_taken(err, &new_user.email))?;
        Ok(user)
    }

    async fn delete(&self, id: i64) -> Result<bool, RepoError> {
        let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn email_taken(err: sqlx::Error, email: &Email) -> RepoError {
    match err {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            RepoError::EmailTaken(email.as_str().to_string())
        }
        other => RepoError::from(other),
    }
}

#[cfg(test)]
pub mod in_memory {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    pub struct InMemoryUserRepository {
        users: Arc<Mutex<Vec<User>>>,
    }

    impl UserRepository for InMemoryUserRepository {
        async fn create(&self, new_user: NewUser) -> Result<User, RepoError> {
            let mut users = self.users.lock().unwrap();
            if users.iter().any(|user| user.email == new_user.email) {
                return Err(RepoError::EmailTaken(new_user.email.as_str().to_string()));
            }
            let id = users.last().map_or(1, |user| user.id + 1);
            let user = User {
                id,
                name: new_user.name,
                email: new_user.email,
            };
            users.push(user.clone());
            Ok(user)
        }

        async fn list(&self, name: Option<String>) -> Result<Vec<User>, RepoError> {
            let users = self.users.lock().unwrap();
            let name = name.map(|name| name.to_lowercase());
            Ok(users
                .iter()
                .filter(|user| {
                    name.as_ref()
                        .is_none_or(|name| user.name.as_str().to_lowercase().contains(name))
                })
                .cloned()
                .collect())
        }

        async fn find(&self, id: i64) -> Result<Option<User>, RepoError> {
            let users = self.users.lock().unwrap();
            Ok(users.iter().find(|user| user.id == id).cloned())
        }

        async fn update(&self, id: i64, new_user: NewUser) -> Result<Option<User>, RepoError> {
            let mut users = self.users.lock().unwrap();
            if users
                .iter()
                .any(|user| user.id != id && user.email == new_user.email)
            {
                return Err(RepoError::EmailTaken(new_user.email.as_str().to_string()));
            }
            let Some(user) = users.iter_mut().find(|user| user.id == id) else {
                return Ok(None);
            };
            user.name = new_user.name;
            user.email = new_user.email;
            Ok(Some(user.clone()))
        }

        async fn delete(&self, id: i64) -> Result<bool, RepoError> {
            let mut users = self.users.lock().unwrap();
            let before = users.len();
            users.retain(|user| user.id != id);
            Ok(users.len() < before)
        }
    }
}
