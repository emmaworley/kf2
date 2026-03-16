use async_trait::async_trait;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::upsert::excluded;

use super::schema::{session, session_provider_config};
use super::{ProviderConfigRepo, RepoError, SessionRepo};
use crate::db::DbPool;
use crate::provider::types::{ProviderConfig, ProviderId};
use crate::provider_config::ProviderConfigRow;
use crate::session::{Session, generate_session_id};

const CREATE_SESSION_MAX_RETRIES: usize = 3;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = session)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct SessionRow {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = session)]
struct NewSessionRow<'a> {
    id: &'a str,
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = session_provider_config)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct ProviderConfigRowDb {
    session_id: String,
    provider_id: String,
    config_json: String,
}

impl TryFrom<ProviderConfigRowDb> for ProviderConfigRow {
    type Error = RepoError;

    fn try_from(row: ProviderConfigRowDb) -> Result<Self, Self::Error> {
        let provider_id = row
            .provider_id
            .parse::<ProviderId>()
            .map_err(|e| RepoError::ProviderId(e.to_string()))?;
        let config = serde_json::from_str::<ProviderConfig>(&row.config_json)?;
        Ok(Self {
            session_id: row.session_id,
            provider_id,
            config,
        })
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = session_provider_config)]
struct NewProviderConfigRow<'a> {
    session_id: &'a str,
    provider_id: &'a str,
    config_json: &'a str,
}

pub struct DieselSessionRepo {
    pool: DbPool,
}

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
                let id = generate_session_id();
                let new_row = NewSessionRow { id: &id };
                match diesel::insert_into(session::table)
                    .values(&new_row)
                    .returning(SessionRow::as_returning())
                    .get_result::<SessionRow>(&mut conn)
                {
                    Ok(row) => return Ok(row.into()),
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
                .select(SessionRow::as_select())
                .first::<SessionRow>(&mut conn)
                .optional()?;
            Ok(row.map(Into::into))
        })
        .await?
    }

    async fn list(&self) -> Result<Vec<Session>, RepoError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Session>, RepoError> {
            let mut conn = pool.get()?;
            let rows = session::table
                .select(SessionRow::as_select())
                .load::<SessionRow>(&mut conn)?;
            Ok(rows.into_iter().map(Into::into).collect())
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

pub struct DieselProviderConfigRepo {
    pool: DbPool,
}

impl DieselProviderConfigRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProviderConfigRepo for DieselProviderConfigRepo {
    async fn upsert(
        &self,
        session_id: &str,
        provider_id: ProviderId,
        config: ProviderConfig,
    ) -> Result<(), RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        let config_json = serde_json::to_string(&config)?;
        tokio::task::spawn_blocking(move || -> Result<(), RepoError> {
            let mut conn = pool.get()?;
            let new_row = NewProviderConfigRow {
                session_id: &session_id,
                provider_id: provider_id.as_str(),
                config_json: &config_json,
            };
            diesel::insert_into(session_provider_config::table)
                .values(&new_row)
                .on_conflict((
                    session_provider_config::session_id,
                    session_provider_config::provider_id,
                ))
                .do_update()
                .set((
                    session_provider_config::config_json
                        .eq(excluded(session_provider_config::config_json)),
                    session_provider_config::updated_at.eq(diesel::dsl::now),
                ))
                .execute(&mut conn)?;
            Ok(())
        })
        .await?
    }

    async fn get(
        &self,
        session_id: &str,
        provider_id: ProviderId,
    ) -> Result<Option<ProviderConfigRow>, RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        let provider_id_str = provider_id.as_str();
        tokio::task::spawn_blocking(move || -> Result<Option<ProviderConfigRow>, RepoError> {
            let mut conn = pool.get()?;
            let row = session_provider_config::table
                .filter(session_provider_config::session_id.eq(&session_id))
                .filter(session_provider_config::provider_id.eq(provider_id_str))
                .select(ProviderConfigRowDb::as_select())
                .first::<ProviderConfigRowDb>(&mut conn)
                .optional()?;
            row.map(ProviderConfigRow::try_from).transpose()
        })
        .await?
    }

    async fn list_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ProviderConfigRow>, RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        tokio::task::spawn_blocking(move || -> Result<Vec<ProviderConfigRow>, RepoError> {
            let mut conn = pool.get()?;
            let rows = session_provider_config::table
                .filter(session_provider_config::session_id.eq(&session_id))
                .select(ProviderConfigRowDb::as_select())
                .load::<ProviderConfigRowDb>(&mut conn)?;
            rows.into_iter().map(ProviderConfigRow::try_from).collect()
        })
        .await?
    }

    async fn delete(&self, session_id: &str, provider_id: ProviderId) -> Result<bool, RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        let provider_id_str = provider_id.as_str();
        tokio::task::spawn_blocking(move || -> Result<bool, RepoError> {
            let mut conn = pool.get()?;
            let removed = diesel::delete(
                session_provider_config::table
                    .filter(session_provider_config::session_id.eq(&session_id))
                    .filter(session_provider_config::provider_id.eq(provider_id_str)),
            )
            .execute(&mut conn)?;
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

    async fn repos(test_name: &str) -> (DieselSessionRepo, DieselProviderConfigRepo) {
        let pool = db::create_pool_in_memory(test_name).await.unwrap();
        (
            DieselSessionRepo::new(pool.clone()),
            DieselProviderConfigRepo::new(pool),
        )
    }

    fn sample_config() -> ProviderConfig {
        ProviderConfig::BasicAuth {
            username: "alice".into(),
            password: "hunter2".into(),
        }
    }

    fn assert_basic_auth(
        config: &ProviderConfig,
        expected_username: &str,
        expected_password: &str,
    ) {
        let ProviderConfig::BasicAuth { username, password } = config;
        assert_eq!(username, expected_username);
        assert_eq!(password, expected_password);
    }

    #[tokio::test]
    async fn create_session_returns_populated_row() {
        let (sessions, _) = repos("diesel_create_session").await;
        let s = sessions.create().await.unwrap();
        let parts: Vec<&str> = s.id.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| !p.is_empty()));
    }

    #[tokio::test]
    async fn get_missing_session_returns_none() {
        let (sessions, _) = repos("diesel_get_missing").await;
        assert!(sessions.get("does-not-exist").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_returns_all_created_sessions() {
        let (sessions, _) = repos("diesel_list_sessions").await;
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
        let (sessions, _) = repos("diesel_delete_session").await;
        let s = sessions.create().await.unwrap();
        assert!(sessions.delete(&s.id).await.unwrap());
        assert!(sessions.get(&s.id).await.unwrap().is_none());
        assert!(!sessions.delete(&s.id).await.unwrap());
    }

    #[tokio::test]
    async fn upsert_updates_existing_row() {
        let (sessions, configs) = repos("diesel_upsert_update").await;
        let s = sessions.create().await.unwrap();
        configs
            .upsert(&s.id, ProviderId::Dam, sample_config())
            .await
            .unwrap();
        configs
            .upsert(
                &s.id,
                ProviderId::Dam,
                ProviderConfig::BasicAuth {
                    username: "bob".into(),
                    password: "s3cret".into(),
                },
            )
            .await
            .unwrap();
        let row = configs.get(&s.id, ProviderId::Dam).await.unwrap().unwrap();
        assert_basic_auth(&row.config, "bob", "s3cret");
    }

    #[tokio::test]
    async fn list_for_session_is_scoped() {
        let (sessions, configs) = repos("diesel_list_scoped").await;
        let a = sessions.create().await.unwrap();
        let b = sessions.create().await.unwrap();
        configs
            .upsert(&a.id, ProviderId::Dam, sample_config())
            .await
            .unwrap();
        configs
            .upsert(&a.id, ProviderId::Joysound, sample_config())
            .await
            .unwrap();
        configs
            .upsert(&b.id, ProviderId::Dam, sample_config())
            .await
            .unwrap();

        let a_rows = configs.list_for_session(&a.id).await.unwrap();
        assert_eq!(a_rows.len(), 2);
        let providers: Vec<ProviderId> = a_rows.iter().map(|r| r.provider_id).collect();
        assert!(providers.contains(&ProviderId::Dam));
        assert!(providers.contains(&ProviderId::Joysound));

        let b_rows = configs.list_for_session(&b.id).await.unwrap();
        assert_eq!(b_rows.len(), 1);
        assert_eq!(b_rows[0].provider_id, ProviderId::Dam);
    }

    #[tokio::test]
    async fn delete_provider_config_is_targeted() {
        let (sessions, configs) = repos("diesel_delete_targeted").await;
        let s = sessions.create().await.unwrap();
        configs
            .upsert(&s.id, ProviderId::Dam, sample_config())
            .await
            .unwrap();
        configs
            .upsert(&s.id, ProviderId::Joysound, sample_config())
            .await
            .unwrap();

        assert!(configs.delete(&s.id, ProviderId::Dam).await.unwrap());

        let remaining = configs.list_for_session(&s.id).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].provider_id, ProviderId::Joysound);
        assert!(!configs.delete(&s.id, ProviderId::Dam).await.unwrap());
    }

    #[tokio::test]
    async fn cascade_delete_when_session_removed() {
        let (sessions, configs) = repos("diesel_cascade").await;
        let s = sessions.create().await.unwrap();
        configs
            .upsert(&s.id, ProviderId::Dam, sample_config())
            .await
            .unwrap();

        sessions.delete(&s.id).await.unwrap();

        let remaining = configs.list_for_session(&s.id).await.unwrap();
        assert!(
            remaining.is_empty(),
            "cascade delete should have removed provider config rows; got {remaining:?}"
        );
    }
}
