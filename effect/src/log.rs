//! A small log at ~/Library/Logs/Raster CRT.log for problems the host never shows, such as a
//! GPU failure or earlier frames the host wouldn't give. Each message is written once per
//! session, so a problem that repeats on every frame never fills the disk.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_MESSAGES: usize = 200;
const SECONDS_PER_DAY: u64 = 86_400;

static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/Raster CRT.log"))
}

/// DD-MM-YYYY HH:MM:SS UTC for a Unix time (days to civil date after Howard Hinnant).
fn stamp(unix: u64) -> String {
    let (days, secs) = ((unix / SECONDS_PER_DAY) as i64, unix % SECONDS_PER_DAY);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}-{month:02}-{year} {:02}:{:02}:{:02} UTC", secs / 3_600, secs / 60 % 60, secs % 60)
}

/// Writes `message` to the log the first time it is seen this session.
pub fn once(message: impl Into<String>) {
    let message = message.into();
    let Ok(mut guard) = SEEN.lock() else { return };
    let seen = guard.get_or_insert_with(HashSet::new);
    if seen.len() >= MAX_MESSAGES || !seen.insert(message.clone()) {
        return;
    }
    let Some(path) = path() else { return };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(file, "{} {message}", stamp(now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_read_day_first() {
        assert_eq!(stamp(0), "01-01-1970 00:00:00 UTC");
        assert_eq!(stamp(1_792_053_296), "15-10-2026 08:34:56 UTC");
        assert_eq!(stamp(951_782_400), "29-02-2000 00:00:00 UTC");
    }
}
