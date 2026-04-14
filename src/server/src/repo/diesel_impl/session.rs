use async_trait::async_trait;
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};

use crate::db::DbPool;
use crate::models::Session;
use crate::repo::schema::session;
use crate::repo::{RepoError, SessionRepo};

const CREATE_SESSION_MAX_RETRIES: usize = 3;

#[derive(Debug, Insertable)]
#[diesel(table_name = session)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct NewSession<'a> {
    id: &'a str,
}

pub struct DieselSessionRepo {
    pool: DbPool,
}

// Trait implementer of SessionRepo via Diesel.
impl DieselSessionRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepo for DieselSessionRepo {
    async fn create(&self) -> Result<Session, RepoError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Session, RepoError> {
            let mut conn = pool.get()?;
            for _ in 0..CREATE_SESSION_MAX_RETRIES {
                let id = Session::new_session_id();
                let new_row = NewSession { id: &id };
                match diesel::insert_into(session::table)
                    .values(&new_row)
                    .returning(Session::as_returning())
                    .get_result::<Session>(&mut conn)
                {
                    Ok(row) => return Ok(row),
                    Err(DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(RepoError::IdRetriesExhausted)
        })
        .await?
    }

    async fn get(&self, id: &str) -> Result<Option<Session>, RepoError> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || -> Result<Option<Session>, RepoError> {
            let mut conn = pool.get()?;
            let row = session::table
                .find(&id)
                .select(Session::as_select())
                .first::<Session>(&mut conn)
                .optional()?;
            Ok(row)
        })
        .await?
    }

    async fn list(&self) -> Result<Vec<Session>, RepoError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Session>, RepoError> {
            let mut conn = pool.get()?;
            let rows = session::table
                .select(Session::as_select())
                .load::<Session>(&mut conn)?;
            Ok(rows)
        })
        .await?
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || -> Result<bool, RepoError> {
            let mut conn = pool.get()?;
            let removed =
                diesel::delete(session::table.filter(session::id.eq(&id))).execute(&mut conn)?;
            Ok(removed > 0)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::collections::HashSet;

    async fn repo(test_name: &str) -> DieselSessionRepo {
        let pool = db::test_support::create_pool_in_memory(test_name)
            .await
            .unwrap();
        DieselSessionRepo::new(pool)
    }

    #[tokio::test]
    async fn create_session_returns_populated_row() {
        let sessions = repo("diesel_create_session").await;
        let s = sessions.create().await.unwrap();
        let parts: Vec<&str> = s.id.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| !p.is_empty()));
    }

    #[tokio::test]
    async fn get_missing_session_returns_none() {
        let sessions = repo("diesel_get_missing").await;
        assert!(sessions.get("does-not-exist").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_returns_all_created_sessions() {
        let sessions = repo("diesel_list_sessions").await;
        let a = sessions.create().await.unwrap();
        let b = sessions.create().await.unwrap();
        let c = sessions.create().await.unwrap();
        let all = sessions.list().await.unwrap();
        assert_eq!(all.len(), 3);
        let ids: HashSet<_> = all.into_iter().map(|s| s.id).collect();
        assert!(ids.contains(&a.id));
        assert!(ids.contains(&b.id));
        assert!(ids.contains(&c.id));
    }

    #[tokio::test]
    async fn delete_session_returns_true_then_false() {
        let sessions = repo("diesel_delete_session").await;
        let s = sessions.create().await.unwrap();
        assert!(sessions.delete(&s.id).await.unwrap());
        assert!(sessions.get(&s.id).await.unwrap().is_none());
        assert!(!sessions.delete(&s.id).await.unwrap());
    }
}
