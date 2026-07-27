use std::time::Duration;

use sqlx::{FromRow, Postgres, Transaction};
use tracing::{error, info, warn};

use crate::state::AppState;

const BATCH_SIZE: i64 = 20;

#[derive(Debug, FromRow)]
struct CleanupJob {
    id: i64,
    object_key: String,
    attempts: i32,
}

pub async fn enqueue(
    transaction: &mut Transaction<'_, Postgres>,
    object_key: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO storage_gc_jobs (object_key, reason)
         VALUES ($1, $2)
         ON CONFLICT (object_key) DO UPDATE
         SET reason = EXCLUDED.reason,
             next_attempt_at = LEAST(storage_gc_jobs.next_attempt_at, now()),
             locked_at = NULL",
    )
    .bind(object_key)
    .bind(reason)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = process_due(&state).await {
            error!(%error, "storage garbage-collection pass failed");
        }
    }
}

async fn process_due(state: &AppState) -> Result<(), sqlx::Error> {
    let jobs = sqlx::query_as::<_, CleanupJob>(
        "WITH due AS (
             SELECT id
             FROM storage_gc_jobs
             WHERE next_attempt_at <= now()
               AND (locked_at IS NULL OR locked_at < now() - interval '5 minutes')
             ORDER BY next_attempt_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT $1
         )
         UPDATE storage_gc_jobs job
         SET locked_at = now()
         FROM due
         WHERE job.id = due.id
         RETURNING job.id, job.object_key, job.attempts",
    )
    .bind(BATCH_SIZE)
    .fetch_all(state.pool())
    .await?;

    for job in jobs {
        match state
            .object_storage()
            .delete_public_object(state.storage_http_client(), &job.object_key)
            .await
        {
            Ok(()) => {
                sqlx::query("DELETE FROM storage_gc_jobs WHERE id = $1")
                    .bind(job.id)
                    .execute(state.pool())
                    .await?;
                info!(object_key = job.object_key, "storage garbage collected");
            }
            Err(delete_error) => {
                let next_attempt = job.attempts.saturating_add(1);
                let delay_seconds = (30_i64
                    .saturating_mul(2_i64.saturating_pow(next_attempt.min(7) as u32)))
                .min(3600);
                sqlx::query(
                    "UPDATE storage_gc_jobs
                     SET attempts = $1,
                         next_attempt_at = now() + make_interval(secs => $2),
                         locked_at = NULL,
                         last_error = left($3, 2000)
                     WHERE id = $4",
                )
                .bind(next_attempt)
                .bind(delay_seconds as f64)
                .bind(delete_error.to_string())
                .bind(job.id)
                .execute(state.pool())
                .await?;
                warn!(
                    object_key = job.object_key,
                    attempts = next_attempt,
                    %delete_error,
                    "storage garbage collection scheduled for retry"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn retry_delay_is_bounded() {
        let delays = (1_i32..=12)
            .map(|attempt| {
                (30_i64.saturating_mul(2_i64.saturating_pow(attempt.min(7) as u32))).min(3600)
            })
            .collect::<Vec<_>>();
        assert_eq!(delays[0], 60);
        assert_eq!(delays.last(), Some(&3600));
    }
}
