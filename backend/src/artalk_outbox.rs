use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};
use tracing::{error, info, warn};

use crate::state::AppState;

const BATCH_SIZE: i64 = 20;

#[derive(Debug, FromRow)]
struct Job {
    id: i64,
    aggregate_key: String,
    operation: String,
    payload: serde_json::Value,
    attempts: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SetCommentingPayload {
    page_key: String,
    page_title: String,
    allowed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeletePagePayload {
    page_key: String,
}

pub async fn enqueue_set_commenting(
    transaction: &mut Transaction<'_, Postgres>,
    page_key: &str,
    page_title: &str,
    allowed: bool,
) -> Result<(), sqlx::Error> {
    enqueue(
        transaction,
        page_key,
        "set_commenting",
        serde_json::json!({
            "page_key": page_key,
            "page_title": page_title,
            "allowed": allowed
        }),
    )
    .await
}

pub async fn enqueue_delete_page(
    transaction: &mut Transaction<'_, Postgres>,
    page_key: &str,
) -> Result<(), sqlx::Error> {
    enqueue(
        transaction,
        page_key,
        "delete_page",
        serde_json::json!({ "page_key": page_key }),
    )
    .await
}

async fn enqueue(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate_key: &str,
    operation: &str,
    payload: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO artalk_outbox (aggregate_key, operation, payload)
         VALUES ($1, $2, $3)
         ON CONFLICT (aggregate_key) DO UPDATE
         SET operation = EXCLUDED.operation,
             payload = EXCLUDED.payload,
             attempts = 0,
             next_attempt_at = now(),
             locked_at = NULL,
             last_error = NULL,
             updated_at = now()",
    )
    .bind(aggregate_key)
    .bind(operation)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = process_due(&state).await {
            error!(%error, "Artalk outbox pass failed");
        }
    }
}

async fn process_due(state: &AppState) -> Result<(), sqlx::Error> {
    // Claim work in a single short statement. No PostgreSQL lock is held while
    // the Artalk HTTP call runs; duplicate delivery after a crash is safe because
    // both supported operations are idempotent.
    let jobs = sqlx::query_as::<_, Job>(
        "WITH due AS (
             SELECT id
             FROM artalk_outbox
             WHERE next_attempt_at <= now()
               AND (locked_at IS NULL OR locked_at < now() - interval '5 minutes')
             ORDER BY next_attempt_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT $1
         )
         UPDATE artalk_outbox job
         SET locked_at = now(), updated_at = now()
         FROM due
         WHERE job.id = due.id
         RETURNING job.id, job.aggregate_key, job.operation, job.payload, job.attempts",
    )
    .bind(BATCH_SIZE)
    .fetch_all(state.pool())
    .await?;

    for job in jobs {
        let delivered = deliver(state, &job).await;
        match delivered {
            Ok(()) => {
                sqlx::query("DELETE FROM artalk_outbox WHERE id = $1")
                    .bind(job.id)
                    .execute(state.pool())
                    .await?;
                info!(
                    page_key = job.aggregate_key,
                    operation = job.operation,
                    "Artalk outbox delivered"
                );
            }
            Err(delivery_error) => {
                let attempts = job.attempts.saturating_add(1);
                let delay_seconds =
                    (5_i64.saturating_mul(2_i64.saturating_pow(attempts.min(9) as u32))).min(3600);
                sqlx::query(
                    "UPDATE artalk_outbox
                     SET attempts = $1,
                         next_attempt_at = now() + make_interval(secs => $2),
                         locked_at = NULL,
                         last_error = left($3, 2000),
                         updated_at = now()
                     WHERE id = $4",
                )
                .bind(attempts)
                .bind(delay_seconds as f64)
                .bind(delivery_error.to_string())
                .bind(job.id)
                .execute(state.pool())
                .await?;
                warn!(
                    page_key = job.aggregate_key,
                    operation = job.operation,
                    attempts,
                    error = %delivery_error,
                    "Artalk outbox scheduled for retry"
                );
            }
        }
    }
    Ok(())
}

async fn deliver(state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.operation.as_str() {
        "set_commenting" => {
            let payload: SetCommentingPayload = serde_json::from_value(job.payload.clone())?;
            state
                .artalk()
                .set_page_commenting(&payload.page_key, &payload.page_title, payload.allowed)
                .await?;
        }
        "delete_page" => {
            let payload: DeletePagePayload = serde_json::from_value(job.payload.clone())?;
            state
                .artalk()
                .delete_pages([payload.page_key.as_str()])
                .await?;
        }
        operation => anyhow::bail!("unsupported Artalk outbox operation {operation}"),
    }
    Ok(())
}
