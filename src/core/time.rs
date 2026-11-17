use chrono::{DateTime, Duration, Utc};

/// The start of a reproducible time window: `days` before `end`.
///
/// The window is pinned to an explicit `end` rather than the wall clock, which is
/// what makes a run reproducible.
pub fn window_start(end: DateTime<Utc>, days: i64) -> DateTime<Utc> {
    end - Duration::days(days)
}
