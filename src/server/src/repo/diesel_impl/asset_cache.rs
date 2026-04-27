use async_trait::async_trait;
use diesel::prelude::*;
use diesel::upsert::excluded;

use crate::db::DbPool;
use crate::repo::schema::asset_cache;
use crate::repo::{AssetCacheEntry, AssetCacheRepo, RepoError};

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = asset_cache)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct AssetCacheRow {
    asset_id: String,
    relative_path: String,
}

impl From<AssetCacheRow> for AssetCacheEntry {
    fn from(row: AssetCacheRow) -> Self {
        Self {
            asset_id: row.asset_id,
            relative_path: row.relative_path,
        }
    }
}

impl From<AssetCacheEntry> for AssetCacheRow {
    fn from(entry: AssetCacheEntry) -> Self {
        Self {
            asset_id: entry.asset_id,
            relative_path: entry.relative_path,
        }
    }
}

pub struct DieselAssetCacheRepo {
    pool: DbPool,
}

impl DieselAssetCacheRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetCacheRepo for DieselAssetCacheRepo {
    async fn get(&self, asset_id: &str) -> Result<Option<AssetCacheEntry>, RepoError> {
        let pool = self.pool.clone();
        let asset_id = asset_id.to_owned();
        tokio::task::spawn_blocking(move || -> Result<Option<AssetCacheEntry>, RepoError> {
            let mut conn = pool.get()?;
            let row = asset_cache::table
                .find(&asset_id)
                .select(AssetCacheRow::as_select())
                .first::<AssetCacheRow>(&mut conn)
                .optional()?;
            Ok(row.map(AssetCacheEntry::from))
        })
        .await?
    }

    async fn upsert(&self, entry: AssetCacheEntry) -> Result<(), RepoError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<(), RepoError> {
            let mut conn = pool.get()?;
            let row = AssetCacheRow::from(entry);
            diesel::insert_into(asset_cache::table)
                .values(&row)
                .on_conflict(asset_cache::asset_id)
                .do_update()
                .set((
                    asset_cache::relative_path.eq(excluded(asset_cache::relative_path)),
                    asset_cache::updated_at.eq(diesel::dsl::now),
                ))
                .execute(&mut conn)?;
            Ok(())
        })
        .await?
    }

    async fn delete(&self, asset_id: &str) -> Result<bool, RepoError> {
        let pool = self.pool.clone();
        let asset_id = asset_id.to_owned();
        tokio::task::spawn_blocking(move || -> Result<bool, RepoError> {
            let mut conn = pool.get()?;
            let removed =
                diesel::delete(asset_cache::table.filter(asset_cache::asset_id.eq(&asset_id)))
                    .execute(&mut conn)?;
            Ok(removed > 0)
        })
        .await?
    }

    async fn list(&self) -> Result<Vec<AssetCacheEntry>, RepoError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<AssetCacheEntry>, RepoError> {
            let mut conn = pool.get()?;
            let rows = asset_cache::table
                .select(AssetCacheRow::as_select())
                .order(asset_cache::asset_id.asc())
                .load::<AssetCacheRow>(&mut conn)?;
            Ok(rows.into_iter().map(AssetCacheEntry::from).collect())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn repo(test_name: &str) -> DieselAssetCacheRepo {
        let pool = db::test_support::create_pool_in_memory(test_name)
            .await
            .unwrap();
        DieselAssetCacheRepo::new(pool)
    }

    fn entry(asset_id: &str, relative_path: &str) -> AssetCacheEntry {
        AssetCacheEntry {
            asset_id: asset_id.into(),
            relative_path: relative_path.into(),
        }
    }

    #[tokio::test]
    async fn get_returns_none_for_missing_asset() {
        let r = repo("asset_cache_get_missing").await;
        assert!(r.get("urn:dam:contentsId:1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let r = repo("asset_cache_round_trip").await;
        r.upsert(entry(
            "urn:dam:contentsId:1",
            "urn_dam_contentsId_1/playlist.m3u8",
        ))
        .await
        .unwrap();
        let got = r.get("urn:dam:contentsId:1").await.unwrap().unwrap();
        assert_eq!(got.asset_id, "urn:dam:contentsId:1");
        assert_eq!(got.relative_path, "urn_dam_contentsId_1/playlist.m3u8");
    }

    #[tokio::test]
    async fn upsert_overwrites_existing_relative_path() {
        let r = repo("asset_cache_upsert_overwrite").await;
        r.upsert(entry("urn:dam:contentsId:1", "old/playlist.m3u8"))
            .await
            .unwrap();
        r.upsert(entry("urn:dam:contentsId:1", "new/playlist.m3u8"))
            .await
            .unwrap();
        let got = r.get("urn:dam:contentsId:1").await.unwrap().unwrap();
        assert_eq!(got.relative_path, "new/playlist.m3u8");
    }

    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let r = repo("asset_cache_delete").await;
        r.upsert(entry("urn:dam:contentsId:1", "x/playlist.m3u8"))
            .await
            .unwrap();
        assert!(r.delete("urn:dam:contentsId:1").await.unwrap());
        assert!(r.get("urn:dam:contentsId:1").await.unwrap().is_none());
        assert!(!r.delete("urn:dam:contentsId:1").await.unwrap());
    }

    #[tokio::test]
    async fn list_returns_all_in_id_order() {
        let r = repo("asset_cache_list").await;
        r.upsert(entry("urn:dam:contentsId:2", "two/playlist.m3u8"))
            .await
            .unwrap();
        r.upsert(entry("urn:dam:contentsId:1", "one/playlist.m3u8"))
            .await
            .unwrap();
        r.upsert(entry("urn:dam:contentsId:3", "three/playlist.m3u8"))
            .await
            .unwrap();
        let all = r.list().await.unwrap();
        assert_eq!(
            all.iter().map(|e| &e.asset_id).collect::<Vec<_>>(),
            vec![
                "urn:dam:contentsId:1",
                "urn:dam:contentsId:2",
                "urn:dam:contentsId:3",
            ]
        );
    }
}
