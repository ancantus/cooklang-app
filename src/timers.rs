//! Recipe timers: model, wall-clock state, duration parsing, and persistence.
//!
//! Kept free of Dioxus so the logic can be unit-tested on the host with
//! `cargo test --no-default-features`.
//!
//! Timers count down against wall-clock deadlines (unix milliseconds) rather
//! than a ticking counter. The Android WebView suspends timers while the app
//! is backgrounded, but a deadline-based countdown is unaffected: when the app
//! comes back, the remaining time is simply recalculated. State is persisted to
//! disk on every change because Android may kill the process at any time.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cooklang::{Quantity, Value};
use serde::{Deserialize, Serialize};

/// One started timer, identified by its recipe and position in the recipe.
///
/// A deadline in the past means the timer has finished; `None` means paused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerEntry {
    /// Stable id: `"<recipe path>#<timer index>"`.
    pub id: String,
    /// Title of the recipe the timer was started from.
    pub recipe_title: String,
    /// Authored timer name, or "Timer" when unnamed.
    pub label: String,
    /// Full duration in seconds; the target when a timer is restarted.
    pub total_secs: u64,
    /// Wall-clock end time in unix milliseconds while counting down.
    pub deadline_ms: Option<u128>,
    /// Seconds left when the timer was paused; unused while running.
    pub paused_remaining_secs: u64,
    /// Lower endpoint for a range timer's early completion chime.
    #[serde(default)]
    pub range_early_at_secs: Option<u64>,
    /// Wall-clock time of the lower-endpoint chime while the timer is running.
    #[serde(default)]
    pub range_early_deadline_ms: Option<u128>,
    /// Remaining lower-endpoint time while the timer is paused.
    #[serde(default)]
    pub paused_range_remaining_secs: Option<u64>,
    /// Whether the range's lower endpoint has already chimed.
    #[serde(default)]
    pub range_early_notified: bool,
    /// Whether the timer's completion has been announced; announced timers are
    /// discarded from persistence the next time the app starts.
    #[serde(default)]
    pub notified: bool,
}

impl TimerEntry {
    /// A new entry with a fresh countdown running from `now_ms`.
    pub fn start_at(
        id: &str,
        recipe_title: &str,
        label: &str,
        total_secs: u64,
        now_ms: u128,
    ) -> Self {
        Self::start_with_range_at(id, recipe_title, label, total_secs, None, now_ms)
    }

    /// Start a timer that can also chime at the lower endpoint of a range.
    pub fn start_with_range_at(
        id: &str,
        recipe_title: &str,
        label: &str,
        total_secs: u64,
        range_early_at_secs: Option<u64>,
        now_ms: u128,
    ) -> Self {
        Self {
            id: id.to_owned(),
            recipe_title: recipe_title.to_owned(),
            label: label.to_owned(),
            total_secs,
            deadline_ms: Some(now_ms + u128::from(total_secs) * 1000),
            paused_remaining_secs: 0,
            range_early_at_secs,
            range_early_deadline_ms: range_early_at_secs
                .map(|secs| now_ms + u128::from(secs) * 1000),
            paused_range_remaining_secs: None,
            range_early_notified: false,
            notified: false,
        }
    }

    /// Seconds remaining right now; 0 once the deadline has passed.
    pub fn remaining_secs(&self, now_ms: u128) -> u64 {
        match self.deadline_ms {
            Some(deadline) if deadline > now_ms => {
                u64::try_from((deadline - now_ms).div_ceil(1000)).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Whether the countdown is actively running (deadline in the future).
    pub fn is_counting(&self, now_ms: u128) -> bool {
        self.deadline_ms.is_some_and(|deadline| deadline > now_ms)
    }

    /// Whether the timer is paused.
    pub fn is_paused(&self) -> bool {
        self.deadline_ms.is_none()
    }

    /// Whether the countdown has elapsed.
    pub fn is_finished(&self, now_ms: u128) -> bool {
        self.deadline_ms.is_some_and(|deadline| deadline <= now_ms)
    }

    /// Whether the lower-endpoint chime is due and has not been announced.
    pub fn is_range_early_due(&self, now_ms: u128) -> bool {
        self.range_early_at_secs.is_some()
            && !self.range_early_notified
            && (self
                .range_early_deadline_ms
                .is_some_and(|deadline| deadline <= now_ms)
                || self.paused_range_remaining_secs == Some(0))
    }
}

/// Wall-clock time in unix milliseconds; the reference for all deadlines.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0)
}

/// Resolve an authored timer quantity to a duration in seconds.
///
/// Returns `None` when the user should be asked for a duration instead: the
/// timer has no quantity at all (`~rest{}`), has a text value, or uses a unit
/// that is not a unit of time. A number without a unit is read as minutes.
/// A range starts the longer bound.
pub fn timer_duration_secs(quantity: Option<&Quantity>) -> Option<u64> {
    let quantity = quantity?;
    let value = match quantity.value() {
        Value::Number(number) => number.value(),
        Value::Range { end, .. } => end.value(),
        Value::Text(_) => return None,
    };
    quantity_value_secs(value, quantity.unit())
}

/// Resolve the lower endpoint used for a range timer's early chime.
pub fn timer_range_early_secs(quantity: Option<&Quantity>) -> Option<u64> {
    let quantity = quantity?;
    let Value::Range { start, end } = quantity.value() else {
        return None;
    };
    let start = quantity_value_secs(start.value(), quantity.unit())?;
    let end = quantity_value_secs(end.value(), quantity.unit())?;
    (start < end).then_some(start)
}

fn quantity_value_secs(value: f64, unit: Option<&str>) -> Option<u64> {
    let minutes = match unit {
        Some(unit) => match unit.to_ascii_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => value / 60.0,
            "m" | "min" | "mins" | "minute" | "minutes" => value,
            "h" | "hr" | "hrs" | "hour" | "hours" => value * 60.0,
            "d" | "day" | "days" => value * 24.0 * 60.0,
            // Unknown units (grams, cups, …) need a user supplied duration.
            _ => return None,
        },
        // No unit: minutes is the cooking convention.
        None => value,
    };
    if !minutes.is_finite() {
        return None;
    }
    let secs = (minutes * 60.0).round();
    u64::try_from(secs as i64).ok().filter(|secs| *secs > 0)
}

/// Format seconds as `M:SS` or, from one hour up, `H:MM:SS`.
pub fn format_countdown(total_secs: u64) -> String {
    let secs = total_secs % 60;
    let minutes = (total_secs / 60) % 60;
    let hours = total_secs / 3600;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// Load the persisted active timers; an unreadable file counts as none.
pub fn load_timers() -> Vec<TimerEntry> {
    read_timers(&timers_file_path())
}

/// Persist the active timers, replacing the previous state atomically.
pub fn save_timers(timers: &[TimerEntry]) -> Result<(), String> {
    write_timers(&timers_file_path(), timers)
}

fn timers_file_path() -> PathBuf {
    crate::repository::app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("cooklang-client"))
        .join("timers.json")
}

fn read_timers(path: &Path) -> Vec<TimerEntry> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn write_timers(path: &Path, timers: &[TimerEntry]) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().expect("timers file has a parent"))
        .map_err(|error| error.to_string())?;
    let contents = serde_json::to_string(timers).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, contents).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooklang::quantity::Number;

    fn seconds(value: Value, unit: Option<&str>) -> Option<u64> {
        let quantity = Quantity::new(value, unit.map(str::to_owned));
        timer_duration_secs(Some(&quantity))
    }

    fn number(value: f64) -> Value {
        Value::Number(Number::Regular(value))
    }

    #[test]
    fn parses_authored_time_units() {
        assert_eq!(seconds(number(10.0), Some("min")), Some(600));
        assert_eq!(seconds(number(1.0), Some("hour")), Some(3600));
        assert_eq!(seconds(number(90.0), Some("sec")), Some(90));
        assert_eq!(seconds(number(1.5), Some("h")), Some(5400));
        assert_eq!(seconds(number(2.0), Some("days")), Some(172_800));
    }

    #[test]
    fn reads_unitless_numbers_as_minutes() {
        assert_eq!(seconds(number(10.0), None), Some(600));
    }

    #[test]
    fn uses_the_longer_bound_of_a_range() {
        assert_eq!(
            seconds(
                Value::Range {
                    start: Number::Regular(5.0),
                    end: Number::Regular(8.0),
                },
                Some("min"),
            ),
            Some(480),
        );
    }

    #[test]
    fn asks_the_user_for_ambiguous_timers() {
        // No quantity at all.
        assert_eq!(timer_duration_secs(None), None);
        // Text value.
        assert_eq!(seconds(Value::Text("a while".into()), None), None);
        // Non-time unit.
        assert_eq!(seconds(number(5.0), Some("g")), None);
        // Zero and negative durations are not useful.
        assert_eq!(seconds(number(0.0), Some("min")), None);
        assert_eq!(seconds(number(-2.0), Some("min")), None);
    }

    #[test]
    fn formats_countdowns() {
        assert_eq!(format_countdown(0), "0:00");
        assert_eq!(format_countdown(59), "0:59");
        assert_eq!(format_countdown(60), "1:00");
        assert_eq!(format_countdown(615), "10:15");
        assert_eq!(format_countdown(3661), "1:01:01");
    }

    #[test]
    fn tracks_state_from_the_deadline() {
        let entry = TimerEntry::start_at("p#0", "Bread", "Proof", 600, 1_000);
        assert_eq!(entry.remaining_secs(1_000), 600);
        assert!(entry.is_counting(1_000));
        assert!(!entry.is_finished(1_000));

        // Backgrounded past the deadline: still correct, just finished.
        assert_eq!(entry.remaining_secs(2_000_000), 0);
        assert!(entry.is_finished(2_000_000));

        let paused = TimerEntry {
            deadline_ms: None,
            paused_remaining_secs: 120,
            ..entry
        };
        assert!(paused.is_paused());
        assert_eq!(paused.remaining_secs(99_000), 0);
    }

    #[test]
    fn persists_and_restores_timers() {
        let path = std::env::temp_dir().join(format!(
            "cooklang-timers-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entries = vec![
            TimerEntry::start_at("bread.cook#0", "Bread", "Bake", 2700, 5_000),
            TimerEntry {
                deadline_ms: None,
                paused_remaining_secs: 45,
                ..TimerEntry::start_at("rice.cook#2", "Rice", "Steam", 300, 5_000)
            },
        ];
        write_timers(&path, &entries).expect("write");
        assert_eq!(read_timers(&path), entries);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn treats_a_corrupt_file_as_no_timers() {
        let path = std::env::temp_dir().join("cooklang-timers-corrupt-test");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(read_timers(&path).is_empty());
        let _ = std::fs::remove_file(path);
    }
}
