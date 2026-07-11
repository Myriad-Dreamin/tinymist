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

/// A local datetime and its available timezone information.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalDatetime {
    /// The local wall-clock datetime.
    pub datetime: time::PrimitiveDateTime,
    /// The local offset from UTC in whole minutes.
    ///
    /// `None` means that the environment does not provide local timezone
    /// information and `datetime` is in UTC.
    pub local_offset_minutes: Option<i32>,
}

impl LocalDatetime {
    /// Creates a local datetime from calendar and clock components.
    pub fn from_ymd_hms(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        local_offset_minutes: Option<i32>,
    ) -> Option<Self> {
        let date =
            time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
        let time = time::Time::from_hms(hour, minute, second).ok()?;
        Some(Self {
            datetime: time::PrimitiveDateTime::new(date, time),
            local_offset_minutes,
        })
    }
}

/// Returns the current local datetime when the environment provides it.
///
/// Environments without the `system` or `web` capability return the existing
/// UTC epoch fallback without accessing a host clock or timezone database.
#[cfg(any(feature = "system", feature = "web"))]
pub fn local_now() -> Option<LocalDatetime> {
    use chrono::{Datelike, Timelike};

    let now: chrono::DateTime<chrono::Local> = now().into();
    LocalDatetime::from_ymd_hms(
        now.year(),
        now.month().try_into().ok()?,
        now.day().try_into().ok()?,
        now.hour().try_into().ok()?,
        now.minute().try_into().ok()?,
        now.second().try_into().ok()?,
        Some(now.offset().local_minus_utc() / 60),
    )
}

/// Returns the UTC fallback in environments without host time capabilities.
#[cfg(not(any(feature = "system", feature = "web")))]
pub fn local_now() -> Option<LocalDatetime> {
    let now = utc_now();
    Some(LocalDatetime {
        datetime: time::PrimitiveDateTime::new(now.date(), now.time()),
        local_offset_minutes: None,
    })
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

pub use time::format_description::well_known::Rfc3339;

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

/// Creates a format description for yyyy-mm-dd.
pub fn yyyy_mm_dd() -> Vec<::time::format_description::BorrowedFormatItem<'static>> {
    ::time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(any(feature = "system", feature = "web")))]
    #[test]
    fn local_now_uses_utc_epoch_fallback() {
        assert_eq!(
            local_now(),
            LocalDatetime::from_ymd_hms(1970, 1, 1, 0, 0, 0, None)
        );
    }

    #[test]
    fn test_yyyy_mm_dd() {
        let format = yyyy_mm_dd();
        assert!(!format.is_empty(), "format should not be empty");
    }
}
