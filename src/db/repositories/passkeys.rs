use sqlx::SqlitePool;
use webauthn_rs::prelude::*;

use crate::error::AppResult;

pub async fn list(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<Passkey>> {
    let rows = sqlx::query_as::<_, (String,)>("SELECT passkey FROM passkeys WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    let mut passkeys = Vec::new();
    for row in rows {
        let passkey: Passkey = serde_json::from_str(&row.0)?;
        passkeys.push(passkey);
    }
    Ok(passkeys)
}

pub async fn save(pool: &SqlitePool, user_id: &str, passkey: &Passkey) -> AppResult<()> {
    let passkey_json = serde_json::to_string(passkey)?;
    let passkey_id = passkey.cred_id().to_vec();

    sqlx::query("INSERT INTO passkeys (passkey_id, user_id, passkey) VALUES (?, ?, ?)")
        .bind(passkey_id)
        .bind(user_id)
        .bind(passkey_json)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn delete(pool: &SqlitePool, user_id: &str, passkey_id: &[u8]) -> AppResult<()> {
    sqlx::query("DELETE FROM passkeys WHERE user_id = ? AND passkey_id = ?")
        .bind(user_id)
        .bind(passkey_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn find_by_id(pool: &SqlitePool, passkey_id: &[u8]) -> AppResult<Option<(String, Passkey)>> {
    let row = sqlx::query_as::<_, (String, String)>("SELECT user_id, passkey FROM passkeys WHERE passkey_id = ?")
        .bind(passkey_id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        let passkey: Passkey = serde_json::from_str(&row.1)?;
        Ok(Some((row.0, passkey)))
    } else {
        Ok(None)
    }
}
