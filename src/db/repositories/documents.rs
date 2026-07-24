use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::error::AppResult;

/// Maximum characters per FTS5 chunk. Paragraph-aware splitting hard-caps here.
const MAX_CHUNK_CHARS: usize = 1000;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct DocumentRecord {
    pub id: i64,
    pub user_id: String,
    pub topic_ids: Vec<i64>,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Insert a reusable document and its FTS5 chunks, then attach it to the
/// explicitly supplied topics.
pub async fn insert_document(
    pool: &SqlitePool,
    user_id: &str,
    topic_ids: &[i64],
    source_type: &str,
    title: &str,
    url: Option<&str>,
    content: &str,
) -> AppResult<i64> {
    let mut tx = pool.begin().await?;

    let document_id = sqlx::query(
        "INSERT INTO user_documents (user_id, source_type, title, url, content)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(source_type)
    .bind(title)
    .bind(url)
    .bind(content)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    for topic_id in topic_ids {
        sqlx::query("INSERT INTO document_topics (document_id, topic_id) VALUES (?, ?)")
            .bind(document_id)
            .bind(topic_id)
            .execute(&mut *tx)
            .await?;
    }

    for chunk in chunk_text(content) {
        sqlx::query(
            "INSERT INTO document_chunks (document_id, user_id, chunk)
             VALUES (?, ?, ?)",
        )
        .bind(document_id)
        .bind(user_id)
        .bind(chunk)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(document_id)
}

pub async fn list_documents(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: Option<i64>,
) -> AppResult<Vec<DocumentRecord>> {
    let rows = if let Some(topic_id) = topic_id {
        sqlx::query(
            "SELECT id, user_id, source_type, title, url, content, created_at
             FROM user_documents
             WHERE user_id = ? AND EXISTS (
                 SELECT 1 FROM document_topics
                 WHERE document_id = user_documents.id AND topic_id = ?
             )
             ORDER BY created_at DESC, id DESC",
        )
        .bind(user_id)
        .bind(topic_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, user_id, source_type, title, url, content, created_at
             FROM user_documents
             WHERE user_id = ?
             ORDER BY created_at DESC, id DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?
    };

    let mut documents = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.get("id");
        let topic_ids = sqlx::query_scalar::<_, i64>(
            "SELECT assignments.topic_id
             FROM document_topics assignments
             JOIN topics top ON top.id = assignments.topic_id AND top.user_id = ?
             WHERE assignments.document_id = ?
             ORDER BY assignments.topic_id",
        )
        .bind(user_id)
        .bind(id)
        .fetch_all(pool)
        .await?;
        documents.push(DocumentRecord {
            id,
            user_id: row.get("user_id"),
            topic_ids,
            source_type: row.get("source_type"),
            title: row.get("title"),
            url: row.get("url"),
            content: row.get("content"),
            created_at: row.get("created_at"),
        });
    }
    Ok(documents)
}

pub async fn get_document_by_id(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
) -> AppResult<Option<DocumentRecord>> {
    let row = sqlx::query(
        "SELECT id, user_id, source_type, title, url, content, created_at
         FROM user_documents
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let topic_ids = sqlx::query_scalar::<_, i64>(
        "SELECT assignments.topic_id
         FROM document_topics assignments
         JOIN topics top ON top.id = assignments.topic_id AND top.user_id = ?
         WHERE assignments.document_id = ?
         ORDER BY assignments.topic_id",
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Some(DocumentRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        topic_ids,
        source_type: row.get("source_type"),
        title: row.get("title"),
        url: row.get("url"),
        content: row.get("content"),
        created_at: row.get("created_at"),
    }))
}

pub async fn delete_document(pool: &SqlitePool, user_id: &str, id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM document_topics
         WHERE document_id = ? AND EXISTS (
             SELECT 1 FROM user_documents WHERE id = ? AND user_id = ?
         )",
    )
    .bind(id)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM document_chunks WHERE document_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_documents WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn attach_document_topic(
    pool: &SqlitePool,
    user_id: &str,
    document_id: i64,
    topic_id: i64,
) -> AppResult<()> {
    ensure_document_and_topic_owned(pool, user_id, document_id, topic_id).await?;
    sqlx::query("INSERT OR IGNORE INTO document_topics (document_id, topic_id) VALUES (?, ?)")
        .bind(document_id)
        .bind(topic_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn detach_document_topic(
    pool: &SqlitePool,
    user_id: &str,
    document_id: i64,
    topic_id: i64,
) -> AppResult<()> {
    ensure_document_and_topic_owned(pool, user_id, document_id, topic_id).await?;
    sqlx::query("DELETE FROM document_topics WHERE document_id = ? AND topic_id = ?")
        .bind(document_id)
        .bind(topic_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn ensure_document_and_topic_owned(
    pool: &SqlitePool,
    user_id: &str,
    document_id: i64,
    topic_id: i64,
) -> AppResult<()> {
    let owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM user_documents docs
         JOIN topics top ON top.user_id = docs.user_id
         WHERE docs.id = ? AND docs.user_id = ? AND top.id = ? AND top.user_id = ?",
    )
    .bind(document_id)
    .bind(user_id)
    .bind(topic_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    if owned != 1 {
        return Err(crate::error::AppError::NotFound(
            "Document or topic not found".to_string(),
        ));
    }
    Ok(())
}

/// Retrieve up to `limit` matching chunks via FTS5 BM25 ranking from sources
/// explicitly assigned to `topic_id`.
pub async fn retrieve_chunks(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    query: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    let sanitized = sanitize_fts_query(query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT chunks.chunk
         FROM document_chunks chunks
         JOIN document_topics assignments
           ON assignments.document_id = chunks.document_id
          AND assignments.topic_id = ?
         JOIN topics assigned_topics
           ON assigned_topics.id = assignments.topic_id AND assigned_topics.user_id = ?
         JOIN user_documents docs
           ON docs.id = chunks.document_id AND docs.user_id = ?
         WHERE chunks.user_id = ? AND document_chunks MATCH ?
         ORDER BY rank
         LIMIT ?",
    )
    .bind(topic_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(&sanitized)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("chunk"))
        .collect())
}

/// Split text into paragraph-aware chunks, hard-capped at `MAX_CHUNK_CHARS`.
fn chunk_text(content: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    for paragraph in content.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.chars().count() <= MAX_CHUNK_CHARS {
            chunks.push(paragraph.to_string());
        } else {
            // Hard-split overlong paragraphs on character boundaries.
            let mut current = String::new();
            for ch in paragraph.chars() {
                if current.chars().count() >= MAX_CHUNK_CHARS {
                    chunks.push(std::mem::take(&mut current));
                }
                current.push(ch);
            }
            if !current.is_empty() {
                chunks.push(current);
            }
        }
    }
    chunks
}

/// Sanitize a free-text query for FTS5: lowercase, keep alphanumerics, drop FTS5
/// operators, OR-join the remaining terms. Empty when no usable terms remain.
fn sanitize_fts_query(query: &str) -> String {
    let terms: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_string)
        .collect();
    terms.join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::{chunk_text, sanitize_fts_query};

    #[test]
    fn sanitize_strips_operators_and_or_joins() {
        assert_eq!(
            sanitize_fts_query("borrow checker (Rust)"),
            "borrow OR checker OR rust"
        );
        assert_eq!(sanitize_fts_query("  \"NEAR\" AND* "), "near OR and");
    }

    #[test]
    fn sanitize_empty_query_is_empty() {
        assert_eq!(sanitize_fts_query("   "), "");
        assert_eq!(sanitize_fts_query("!@#$"), "");
    }

    #[test]
    fn chunk_splits_on_blank_lines() {
        let chunks = chunk_text("first para\n\nsecond para");
        assert_eq!(chunks, vec!["first para", "second para"]);
    }

    #[test]
    fn chunk_hard_caps_long_paragraphs() {
        let long = "a".repeat(2500);
        let chunks = chunk_text(&long);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.chars().count() <= 1000));
    }
}
