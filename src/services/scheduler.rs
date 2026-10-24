//! Daily maintenance scheduling.
//!
//! [`next_run`] computes when the next daily run happens, in Europe/Paris local time.

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Paris;

use crate::config::SchedulerConfig;
use crate::services::{badge_service, purge_service};
use crate::utils::pocketbase::PocketbaseClient;

/// Boxed future returned by a daily task.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A daily maintenance task.
pub type TaskFn = for<'a> fn(&'a PocketbaseClient) -> BoxFuture<'a, Result<TaskSummary, String>>;

/// Outcome of one maintenance task run.
#[derive(Debug, Default, PartialEq)]
pub struct TaskSummary {
    /// Number of items the task tried to handle.
    pub processed: usize,
    /// Number of items handled successfully.
    pub succeeded: usize,
    /// Record IDs of the items that failed.
    pub failed: Vec<String>,
}

/// First instant strictly after `now` at which the Paris wall clock reads `run_at`.
///
/// A time inside the spring-forward gap moves to the next existing local hour;
/// an ambiguous time (autumn fall-back) resolves to its first occurrence.
pub fn next_run(now: DateTime<Utc>, run_at: NaiveTime) -> DateTime<Utc> {
    let mut date = now.with_timezone(&Paris).date_naive();
    loop {
        let mut local = date.and_time(run_at);
        let candidate = loop {
            match Paris.from_local_datetime(&local) {
                LocalResult::Single(t) => break t,
                LocalResult::Ambiguous(first, _) => break first,
                LocalResult::None => local += Duration::hours(1),
            }
        };
        let candidate = candidate.with_timezone(&Utc);
        if candidate > now {
            return candidate;
        }
        date = date.succ_opt().expect("date overflow");
    }
}

fn purge_task(pb: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
    Box::pin(purge_service::purge_deleted_accounts(pb))
}

fn badges_task(pb: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
    Box::pin(badge_service::refresh_all(pb))
}

/// Daily tasks, in execution order.
pub const DAILY_TASKS: [(&str, TaskFn); 2] = [("purge", purge_task), ("badges", badges_task)];

/// Runs each task in order, logging its outcome; a failure does not stop the next task.
pub async fn run_tasks(
    pb: &PocketbaseClient,
    tasks: &[(&str, TaskFn)],
) -> Vec<(String, Result<TaskSummary, String>)> {
    let mut results = Vec::with_capacity(tasks.len());
    for (name, task) in tasks {
        let result = task(pb).await;
        match &result {
            Ok(summary) => tracing::info!(
                task = *name,
                processed = summary.processed,
                succeeded = summary.succeeded,
                failed = summary.failed.len(),
                failed_ids = ?summary.failed,
                "Daily task finished"
            ),
            Err(error) => tracing::error!(task = *name, %error, "Daily task failed"),
        }
        results.push((name.to_string(), result));
    }
    results
}

/// One daily pass: authenticates against PocketBase, then runs [`DAILY_TASKS`].
async fn run_once() {
    let mut pb = PocketbaseClient::new();
    if let Err(error) = pb.login().await {
        tracing::error!(%error, "Daily tasks skipped: PocketBase login failed");
        return;
    }
    run_tasks(&pb, &DAILY_TASKS).await;
}

/// Starts the background loop that runs the daily tasks at the configured time.
///
/// Does nothing when the scheduler is disabled. A run that fails or panics is logged
/// and retried the next day; a run missed while OAPI is stopped is not caught up.
pub fn spawn(config: &SchedulerConfig) {
    if !config.enabled {
        tracing::info!("Daily tasks scheduler is disabled");
        return;
    }
    let run_at = match config.run_time() {
        Ok(time) => time,
        Err(error) => {
            tracing::error!(%error, "Daily tasks scheduler not started");
            return;
        }
    };
    if crate::config::Config::global().server.env != "production" {
        tracing::warn!(
            "Daily tasks scheduler is enabled outside production and will purge accounts on the configured PocketBase at {}; set scheduler.enabled: false in config.yaml to disable",
            config.run_at
        );
    }
    tokio::spawn(async move {
        loop {
            let next = next_run(Utc::now(), run_at);
            tracing::info!("Next daily tasks run at {}", next.with_timezone(&Paris));
            let wait = (next - Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;
            if let Err(error) = tokio::spawn(run_once()).await {
                tracing::error!(%error, "Daily tasks run aborted");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveTime, Utc};

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn next_run_is_today_or_tomorrow_in_paris_time() {
        let four = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
        // Winter (UTC+1): 04:00 Paris = 03:00 UTC
        assert_eq!(
            next_run(utc("2026-01-10T01:00:00Z"), four),
            utc("2026-01-10T03:00:00Z")
        );
        assert_eq!(
            next_run(utc("2026-01-10T03:00:00Z"), four),
            utc("2026-01-11T03:00:00Z")
        );
        // Summer (UTC+2): 04:00 Paris = 02:00 UTC
        assert_eq!(
            next_run(utc("2026-07-10T01:00:00Z"), four),
            utc("2026-07-10T02:00:00Z")
        );
    }

    #[test]
    fn next_run_skips_dst_gap() {
        // 29 March 2026: 02:00 -> 03:00 in Paris; 02:30 does not exist
        let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        assert_eq!(
            next_run(utc("2026-03-29T00:00:00Z"), t),
            utc("2026-03-29T01:30:00Z")
        );
    }

    #[test]
    fn next_run_runs_once_on_ambiguous_hour() {
        // 25 October 2026: 02:30 Paris exists twice; only the first counts
        let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let first = next_run(utc("2026-10-24T23:00:00Z"), t);
        assert_eq!(first, utc("2026-10-25T00:30:00Z"));
        assert_eq!(next_run(first, t), utc("2026-10-26T01:30:00Z"));
    }

    #[tokio::test]
    async fn run_tasks_continues_after_failure() {
        let pb = PocketbaseClient::for_test("http://127.0.0.1:9".into(), "t");
        fn fail(_: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
            Box::pin(async { Err("boom".into()) })
        }
        fn ok(_: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
            Box::pin(async {
                Ok(TaskSummary {
                    processed: 1,
                    succeeded: 1,
                    failed: vec![],
                })
            })
        }
        let results = run_tasks(&pb, &[("purge", fail), ("badges", ok)]).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("purge".into(), Err("boom".into())));
        assert!(results[1].1.is_ok());
    }

    #[test]
    fn daily_tasks_run_purge_before_badges() {
        assert_eq!(DAILY_TASKS.map(|(name, _)| name), ["purge", "badges"]);
    }
}
