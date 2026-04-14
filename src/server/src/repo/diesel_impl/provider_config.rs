use async_trait::async_trait;
use diesel::prelude::*;
use diesel::upsert::excluded;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::provider::types::{ProviderConfig, ProviderId};
use crate::repo::schema::session_provider_config;
use crate::repo::{ProviderConfigRepo, RepoError};

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = session_provider_config)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct ProviderConfigRow {
    session_id: String,
    provider_id: String,
    config_json: String,
}

impl TryFrom<ProviderConfigRow> for (ProviderId, ProviderConfig) {
    type Error = RepoError;

    fn try_from(row: ProviderConfigRow) -> Result<Self, Self::Error> {
        let provider_id = row
            .provider_id
            .parse::<ProviderId>()
            .map_err(|e| RepoError::ProviderId(e.to_string()))?;
        let config = serde_json::from_str::<ProviderConfig>(&row.config_json)?;
        Ok((provider_id, config))
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
            let new_row = ProviderConfigRow {
                session_id,
                provider_id: provider_id.to_string(),
                config_json,
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
    ) -> Result<Option<ProviderConfig>, RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        let provider_id_str = provider_id.as_str();
        tokio::task::spawn_blocking(move || -> Result<Option<ProviderConfig>, RepoError> {
            let mut conn = pool.get()?;
            let row = session_provider_config::table
                .filter(session_provider_config::session_id.eq(&session_id))
                .filter(session_provider_config::provider_id.eq(provider_id_str))
                .select(ProviderConfigRow::as_select())
                .first::<ProviderConfigRow>(&mut conn)
                .optional()?;
            row.map(<(ProviderId, ProviderConfig)>::try_from)
                .transpose()
                .map(|opt| opt.map(|(_, config)| config))
        })
        .await?
    }

    async fn list_for_session(
        &self,
        session_id: &str,
    ) -> Result<HashMap<ProviderId, ProviderConfig>, RepoError> {
        let pool = self.pool.clone();
        let session_id = session_id.to_owned();
        tokio::task::spawn_blocking(
            move || -> Result<HashMap<ProviderId, ProviderConfig>, RepoError> {
                let mut conn = pool.get()?;
                let rows = session_provider_config::table
                    .filter(session_provider_config::session_id.eq(&session_id))
                    .select(ProviderConfigRow::as_select())
                    .load::<ProviderConfigRow>(&mut conn)?;
                rows.into_iter()
                    .map(<(ProviderId, ProviderConfig)>::try_from)
                    .collect()
            },
        )
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
    use crate::repo::diesel_impl::session::DieselSessionRepo;
    use crate::repo::SessionRepo;

    async fn repos(test_name: &str) -> (DieselSessionRepo, DieselProviderConfigRepo) {
        let pool = db::test_support::create_pool_in_memory(test_name)
            .await
            .unwrap();
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
        assert_basic_auth(&row, "bob", "s3cret");
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
        assert!(a_rows.contains_key(&ProviderId::Dam));
        assert!(a_rows.contains_key(&ProviderId::Joysound));

        let b_rows = configs.list_for_session(&b.id).await.unwrap();
        assert_eq!(b_rows.len(), 1);
        assert!(b_rows.contains_key(&ProviderId::Dam));
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
        assert!(remaining.contains_key(&ProviderId::Joysound));
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
