//! Local time in the formats the log, the window and the log viewer use.

use chrono::{DateTime, Local, TimeZone};

/// The current time, captured once and formatted the ways a call needs.
#[derive(Debug, Clone)]
pub struct Now {
    /// UNIX seconds.
    pub epoch: i64,
    /// `%Y-%m-%dT%H:%M:%S%:z` (the log's `time`).
    pub iso: String,
    /// `%Y%m%dT%H%M%S` (the first part of a call id).
    pub id_stamp: String,
    /// `%H:%M:%S`.
    pub hms: String,
    /// `%Y-%m-%d %H:%M:%S` (the window's time).
    pub ymd_hms: String,
}

fn local(epoch: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(epoch, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap())
}

/// Formats an epoch the ways a call needs.
pub fn at(epoch: i64) -> Now {
    let t = local(epoch);
    Now {
        epoch,
        iso: t.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        id_stamp: t.format("%Y%m%dT%H%M%S").to_string(),
        hms: t.format("%H:%M:%S").to_string(),
        ymd_hms: t.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

/// The current time.
pub fn now() -> Now {
    at(epoch_now())
}

/// UNIX seconds now.
pub fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The local date (`%Y-%m-%d`), time (`%H:%M:%S`) and month/day (`%m/%d`) of an epoch, for the log viewer.
pub fn local_parts(epoch: i64) -> (String, String, String) {
    let t = local(epoch);
    (
        t.format("%Y-%m-%d").to_string(),
        t.format("%H:%M:%S").to_string(),
        t.format("%m/%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // chrono reads TZ when it is set; the tests that depend on it run in one test so that they do not race.
    #[test]
    fn formats_in_the_local_zone() {
        // SAFETY: tests in this crate do not read TZ concurrently except here.
        unsafe { std::env::set_var("TZ", "UTC0") };
        let (date, time, md) = local_parts(1_800_000_000);
        assert_eq!(
            (date.as_str(), time.as_str(), md.as_str()),
            ("2027-01-15", "08:00:00", "01/15")
        );
        let n = at(1_800_000_000);
        assert_eq!(n.iso, "2027-01-15T08:00:00+00:00");
        assert_eq!(n.id_stamp, "20270115T080000");
        assert_eq!(n.ymd_hms, "2027-01-15 08:00:00");
        assert_eq!(n.hms, "08:00:00");
    }
}
