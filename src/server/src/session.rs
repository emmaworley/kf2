use random_word::Lang;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn generate_session_id() -> String {
    let w1 = random_word::get(Lang::En);
    let w2 = random_word::get(Lang::En);
    let w3 = random_word::get(Lang::En);
    format!("{w1}-{w2}-{w3}")
}

pub async fn create_session(pool: &DbPool) -> Result<Session, Box<dyn std::error::Error>> {
    let conn = pool.get().await?;
    let session = conn
        .interact(|conn| {
            const MAX_RETRIES: usize = 3;
            for _ in 0..MAX_RETRIES {
                let id = generate_session_id();
                let result = conn.execute(
                    "INSERT INTO session (id) VALUES (?1)",
                    params![id],
                );
                match result {
                    Ok(_) => {
                        let mut stmt = conn.prepare(
                            "SELECT id, created_at, updated_at FROM session WHERE id = ?1",
                        )?;
                        let session = stmt.query_row(params![id], |row| {
                            Ok(Session {
                                id: row.get(0)?,
                                created_at: row.get(1)?,
                                updated_at: row.get(2)?,
                            })
                        })?;
                        return Ok(session);
                    }
                    Err(rusqlite::Error::SqliteFailure(err, _))
                        if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                Some("failed to generate unique session ID after retries".into()),
            ))
        })
        .await
        .map_err(|e| format!("interact error: {e}"))??;
    Ok(session)
}

pub async fn get_session(
    pool: &DbPool,
    id: String,
) -> Result<Option<Session>, Box<dyn std::error::Error>> {
    let conn = pool.get().await?;
    let session = conn
        .interact(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, created_at, updated_at FROM session WHERE id = ?1",
            )?;
            let result = stmt.query_row(params![id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            });
            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| format!("interact error: {e}"))??;
    Ok(session)
}

pub async fn list_sessions(pool: &DbPool) -> Result<Vec<Session>, Box<dyn std::error::Error>> {
    let conn = pool.get().await?;
    let sessions = conn
        .interact(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, created_at, updated_at FROM session")?;
            let rows = stmt.query_map([], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| format!("interact error: {e}"))??;
    Ok(sessions)
}

pub async fn delete_session(
    pool: &DbPool,
    id: String,
) -> Result<bool, Box<dyn std::error::Error>> {
    let conn = pool.get().await?;
    let deleted = conn
        .interact(move |conn| {
            let rows = conn.execute("DELETE FROM session WHERE id = ?1", params![id])?;
            Ok::<bool, rusqlite::Error>(rows > 0)
        })
        .await
        .map_err(|e| format!("interact error: {e}"))??;
    Ok(deleted)
}
