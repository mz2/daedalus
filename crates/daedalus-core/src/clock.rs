//! Wall-clock helper. Isolated so it is the single place time enters the core.

use std::time::{SystemTime, UNIX_EPOCH};

use daedalus_proto::Timestamp;

/// Current time as a [`Timestamp`] (Unix epoch milliseconds).
#[must_use]
pub fn now() -> Timestamp {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Timestamp::from_millis(ms)
}
