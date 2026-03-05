use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ChatSessionError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Chat session not found")]
    NotFound,
    #[error("Project not found")]
    ProjectNotFound,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ChatSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Option<String>,
    pub pinned: bool,
    pub executor: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ChatSessionWithStatus {
    #[serde(flatten)]
    #[ts(flatten)]
    pub chat_session: ChatSession,
    pub is_running: bool,
    pub elapsed_seconds: Option<i64>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateChatSession {
    pub project_id: Uuid,
    pub title: Option<String>,
    pub executor: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateChatSession {
    pub title: Option<String>,
    pub pinned: Option<bool>,
}

impl ChatSession {
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ChatSession,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      pinned AS "pinned!: bool",
                      executor,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM chat_sessions
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_project_id(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            ChatSession,
            r#"SELECT id AS "id!: Uuid",
                      project_id AS "project_id!: Uuid",
                      title,
                      pinned AS "pinned!: bool",
                      executor,
                      created_at AS "created_at!: DateTime<Utc>",
                      updated_at AS "updated_at!: DateTime<Utc>"
               FROM chat_sessions
               WHERE project_id = $1
               ORDER BY pinned DESC, updated_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_project_id_with_status(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<ChatSessionWithStatus>, sqlx::Error> {
        let records = sqlx::query!(
            r#"SELECT
                cs.id AS "id!: Uuid",
                cs.project_id AS "project_id!: Uuid",
                cs.title,
                cs.pinned AS "pinned!: bool",
                cs.executor,
                cs.created_at AS "created_at!: DateTime<Utc>",
                cs.updated_at AS "updated_at!: DateTime<Utc>",
                CASE WHEN EXISTS (
                    SELECT 1 FROM chat_execution_processes cep
                    WHERE cep.chat_session_id = cs.id AND cep.status = 'running'
                    LIMIT 1
                ) THEN 1 ELSE 0 END AS "is_running!: i64",
                (
                    SELECT CAST((julianday('now') - julianday(cep.started_at)) * 86400 AS INTEGER)
                    FROM chat_execution_processes cep
                    WHERE cep.chat_session_id = cs.id AND cep.status = 'running'
                    ORDER BY cep.started_at DESC
                    LIMIT 1
                ) AS "elapsed_seconds: i64"
            FROM chat_sessions cs
            WHERE cs.project_id = $1
            ORDER BY cs.pinned DESC, cs.updated_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|r| ChatSessionWithStatus {
                chat_session: ChatSession {
                    id: r.id,
                    project_id: r.project_id,
                    title: r.title,
                    pinned: r.pinned,
                    executor: r.executor,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                is_running: r.is_running != 0,
                elapsed_seconds: r.elapsed_seconds,
            })
            .collect())
    }

    pub async fn create(pool: &SqlitePool, data: &CreateChatSession) -> Result<Self, ChatSessionError> {
        let id = Uuid::new_v4();
        Ok(sqlx::query_as!(
            ChatSession,
            r#"INSERT INTO chat_sessions (id, project_id, title, executor)
               VALUES ($1, $2, $3, $4)
               RETURNING id AS "id!: Uuid",
                         project_id AS "project_id!: Uuid",
                         title,
                         pinned AS "pinned!: bool",
                         executor,
                         created_at AS "created_at!: DateTime<Utc>",
                         updated_at AS "updated_at!: DateTime<Utc>""#,
            id,
            data.project_id,
            data.title,
            data.executor
        )
        .fetch_one(pool)
        .await?)
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateChatSession,
    ) -> Result<Self, ChatSessionError> {
        // Update title if provided (empty string clears it)
        let title_value = data.title.as_ref().filter(|s| !s.is_empty());
        let title_provided = data.title.is_some();

        sqlx::query!(
            r#"UPDATE chat_sessions SET
                title = CASE WHEN $1 THEN $2 ELSE title END,
                pinned = COALESCE($3, pinned),
                updated_at = datetime('now', 'subsec')
            WHERE id = $4"#,
            title_provided,
            title_value,
            data.pinned,
            id
        )
        .execute(pool)
        .await?;

        Self::find_by_id(pool, id)
            .await?
            .ok_or(ChatSessionError::NotFound)
    }

    pub async fn update_executor(
        pool: &SqlitePool,
        id: Uuid,
        executor: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE chat_sessions SET executor = $1, updated_at = datetime('now', 'subsec') WHERE id = $2"#,
            executor,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn touch(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE chat_sessions SET updated_at = datetime('now', 'subsec') WHERE id = ?",
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM chat_sessions WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn set_title_from_prompt(
        pool: &SqlitePool,
        id: Uuid,
        prompt: &str,
    ) -> Result<(), sqlx::Error> {
        let title = truncate_to_title(prompt, 60);
        sqlx::query!(
            "UPDATE chat_sessions SET title = $1, updated_at = datetime('now', 'subsec') WHERE id = $2 AND title IS NULL",
            title,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

fn truncate_to_title(prompt: &str, max_len: usize) -> String {
    let trimmed = prompt.trim();
    // Take first line only
    let first_line = trimmed.lines().next().unwrap_or(trimmed);
    if first_line.chars().count() <= max_len {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(max_len).collect();
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &truncated[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }
}
