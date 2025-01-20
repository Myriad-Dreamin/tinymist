//! Cross platform time utilities.

pub use std::time::SystemTime as Time;
pub use web_time::Duration;
pub use web_time::Instant;

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
