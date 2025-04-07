//! Cross platform time utilities.

pub use std::time::SystemTime as Time;
pub use time::UtcDateTime;

#[cfg(not(feature = "web"))]
pub use std::time::{Duration, Instant};
#[cfg(feature = "web")]
pub use web_time::{Duration, Instant};

/// Returns the current datetime in utc (UTC+0).
pub fn utc_now() -> UtcDateTime {
    now().into()
}

/// Returns the current system time (UTC+0).
#[cfg(any(feature = "system", feature = "web"))]
pub fn now() -> Time {
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        Time::now()
    }
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        use web_time::web::SystemTimeExt;
        web_time::SystemTime::now().to_std()
    }
}

/// Returns a dummy time on environments that do not support time.
#[cfg(not(any(feature = "system", feature = "web")))]
pub fn now() -> Time {
    Time::UNIX_EPOCH
}

/// The trait helping convert to a [`UtcDateTime`].
pub trait ToUtcDateTime {
    /// Converts to a [`UtcDateTime`].
    fn to_utc_datetime(self) -> Option<UtcDateTime>;
}

impl ToUtcDateTime for i64 {
    /// Converts a UNIX timestamp to a [`UtcDateTime`].
    fn to_utc_datetime(self) -> Option<UtcDateTime> {
        UtcDateTime::from_unix_timestamp(self).ok()
    }
}

impl ToUtcDateTime for Time {
    /// Converts a system time to a [`UtcDateTime`].
    fn to_utc_datetime(self) -> Option<UtcDateTime> {
        Some(UtcDateTime::from(self))
    }
}

/// Converts a [`UtcDateTime`] to typst's datetime.
#[cfg(feature = "typst")]
pub fn to_typst_time(timestamp: UtcDateTime) -> typst::foundations::Datetime {
    let datetime = ::time::PrimitiveDateTime::new(timestamp.date(), timestamp.time());
    typst::foundations::Datetime::Datetime(datetime)
}
