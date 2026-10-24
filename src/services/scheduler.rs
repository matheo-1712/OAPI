//! Daily maintenance scheduling.
//!
//! [`next_run`] computes when the next daily run happens, in Europe/Paris local time.

use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Paris;

/// Outcome of one maintenance task run.
#[derive(Debug, Default, PartialEq)]
#[allow(dead_code)] // consumed by the background loop (added later)
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
#[allow(dead_code)] // used by the background loop (added later)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveTime, Utc};

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn next_run_is_today_or_tomorrow_in_paris_time() {
        let four = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
        // Winter (UTC+1): 04:00 Paris = 03:00 UTC
        assert_eq!(next_run(utc("2026-01-10T01:00:00Z"), four), utc("2026-01-10T03:00:00Z"));
        assert_eq!(next_run(utc("2026-01-10T03:00:00Z"), four), utc("2026-01-11T03:00:00Z"));
        // Summer (UTC+2): 04:00 Paris = 02:00 UTC
        assert_eq!(next_run(utc("2026-07-10T01:00:00Z"), four), utc("2026-07-10T02:00:00Z"));
    }

    #[test]
    fn next_run_skips_dst_gap() {
        // 29 March 2026: 02:00 -> 03:00 in Paris; 02:30 does not exist
        let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        assert_eq!(next_run(utc("2026-03-29T00:00:00Z"), t), utc("2026-03-29T01:30:00Z"));
    }

    #[test]
    fn next_run_runs_once_on_ambiguous_hour() {
        // 25 October 2026: 02:30 Paris exists twice; only the first counts
        let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let first = next_run(utc("2026-10-24T23:00:00Z"), t);
        assert_eq!(first, utc("2026-10-25T00:30:00Z"));
        assert_eq!(next_run(first, t), utc("2026-10-26T01:30:00Z"));
    }
}
