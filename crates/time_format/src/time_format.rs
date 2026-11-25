use jiff::{Timestamp, ZonedDateTime, tz::TimeZone};
#[cfg(target_os = "linux")]
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampFormat {
    Absolute,
    EnhancedAbsolute,
    Relative,
}

#[cfg(target_os = "linux")]
static CURRENT_LOCALE: LazyLock<String> =
    LazyLock::new(|| sys_locale::get_locale().unwrap_or_else(|| String::from("en-US")));

pub fn format_localized_timestamp(
    timestamp: Timestamp,
    reference: Timestamp,
    timezone: TimeZone,
    format: TimestampFormat,
) -> String {
    let timestamp = timestamp.to_zoned_datetime(timezone.clone());
    let reference = reference.to_zoned_datetime(timezone);
    match format {
        TimestampFormat::Absolute => format_absolute_timestamp(&timestamp, &reference, false),
        TimestampFormat::EnhancedAbsolute => {
            format_absolute_timestamp(&timestamp, &reference, true)
        }
        TimestampFormat::Relative => format_relative_time(&timestamp, &reference)
            .unwrap_or_else(|| format_relative_date(&timestamp, &reference)),
    }
}

fn format_absolute_timestamp(
    timestamp: &ZonedDateTime,
    reference: &ZonedDateTime,
    enhanced_date_formatting: bool,
) -> String {
    #[cfg(target_os = "linux")]
    {
        let is_12_hour_time = is_12_hour_time_by_locale(&CURRENT_LOCALE);
        if enhanced_date_formatting {
            format_timestamp_naive(timestamp, reference, is_12_hour_time)
        } else {
            let date = format_timestamp_naive_date(timestamp, is_12_hour_time);
            let time = format_timestamp_naive_time(timestamp, is_12_hour_time);
            format!("{date} {time}")
        }
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if !enhanced_date_formatting {
            let date = format_absolute_date(timestamp);
            let time = format_absolute_time(timestamp);
            return format!("{date} {time}");
        }

        let timestamp_date = timestamp.date();
        let reference_date = reference.date();
        let time = format_absolute_time(timestamp);
        if timestamp_date == reference_date {
            format!("Today at {time}")
        } else if reference_date
            .yesterday()
            .is_ok_and(|yesterday| yesterday == timestamp_date)
        {
            format!("Yesterday at {time}")
        } else {
            let date = format_absolute_date(timestamp);
            format!("{date} {time}")
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn format_absolute_date(timestamp: &ZonedDateTime) -> String {
    #[cfg(target_os = "macos")]
    {
        macos::format_date(timestamp)
    }
    #[cfg(target_os = "windows")]
    {
        windows::format_date(timestamp)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn format_absolute_time(timestamp: &ZonedDateTime) -> String {
    #[cfg(target_os = "macos")]
    {
        macos::format_time(timestamp)
    }
    #[cfg(target_os = "windows")]
    {
        windows::format_time(timestamp)
    }
}

fn format_relative_time(timestamp: &ZonedDateTime, reference: &ZonedDateTime) -> Option<String> {
    let difference = reference.duration_since(timestamp);
    let minutes = difference.as_mins();
    match minutes {
        ..=0 => Some("Just now".to_string()),
        1 => Some("1 minute ago".to_string()),
        2..=59 => Some(format!("{minutes} minutes ago")),
        _ => {
            let hours = difference.as_hours();
            match hours {
                1 => Some("1 hour ago".to_string()),
                2..=23 => Some(format!("{hours} hours ago")),
                _ => None,
            }
        }
    }
}

fn format_relative_date(timestamp: &ZonedDateTime, reference: &ZonedDateTime) -> String {
    let days = (reference.date() - timestamp.date()).get_days();
    match days {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => format!("{days} days ago"),
        _ => {
            let weeks = days / 7;
            match weeks {
                1 => "1 week ago".to_string(),
                2..=4 => format!("{weeks} weeks ago"),
                _ => {
                    let month_difference = calculate_month_difference(timestamp, reference);
                    match month_difference {
                        ..=1 => "1 month ago".to_string(),
                        2..=11 => format!("{month_difference} months ago"),
                        12..60 => format_compound_year_month(month_difference),
                        months => {
                            let years = (months + 6) / 12;
                            format!("{years} years ago")
                        }
                    }
                }
            }
        }
    }
}

fn format_compound_year_month(month_difference: i32) -> String {
    let years = month_difference / 12;
    let months = month_difference % 12;
    let year_unit = if years == 1 { "year" } else { "years" };
    if months == 0 {
        format!("{years} {year_unit} ago")
    } else {
        let month_unit = if months == 1 { "month" } else { "months" };
        format!("{years} {year_unit}, {months} {month_unit} ago")
    }
}

fn calculate_month_difference(timestamp: &ZonedDateTime, reference: &ZonedDateTime) -> i32 {
    let year_difference = i32::from(reference.year()) - i32::from(timestamp.year());
    let month_difference = i32::from(reference.month()) - i32::from(timestamp.month());
    year_difference * 12 + month_difference
}

#[cfg(target_os = "linux")]
fn format_timestamp_naive(
    timestamp: &ZonedDateTime,
    reference: &ZonedDateTime,
    is_12_hour_time: bool,
) -> String {
    let timestamp_date = timestamp.date();
    let reference_date = reference.date();
    let time = format_timestamp_naive_time(timestamp, is_12_hour_time);
    if timestamp_date == reference_date {
        format!("Today at {time}")
    } else if reference_date
        .yesterday()
        .is_ok_and(|yesterday| yesterday == timestamp_date)
    {
        format!("Yesterday at {time}")
    } else {
        let date = format_timestamp_naive_date(timestamp, is_12_hour_time);
        format!("{date} {time}")
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn format_timestamp_naive_date(timestamp: &ZonedDateTime, is_12_hour_time: bool) -> String {
    let year = timestamp.year();
    let month = timestamp.month();
    let day = timestamp.day();
    if is_12_hour_time {
        format!("{month:02}/{day:02}/{year}")
    } else {
        format!("{day:02}/{month:02}/{year}")
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn format_timestamp_naive_time(timestamp: &ZonedDateTime, is_12_hour_time: bool) -> String {
    let hour = timestamp.hour();
    let minute = timestamp.minute();
    if is_12_hour_time {
        let meridiem = if hour >= 12 { "PM" } else { "AM" };
        let hour = match hour {
            0 => 12,
            13..=23 => hour - 12,
            _ => hour,
        };
        format!("{hour}:{minute:02} {meridiem}")
    } else {
        format!("{hour:02}:{minute:02}")
    }
}

#[cfg(target_os = "linux")]
fn is_12_hour_time_by_locale(locale: &str) -> bool {
    [
        "es-MX", "es-CO", "es-SV", "es-NI", "es-HN", "en-US", "en-CA", "en-AU", "en-NZ", "ar-SA",
        "ar-EG", "ar-JO", "en-IN", "hi-IN", "en-PK", "ur-PK", "en-PH", "fil-PH", "bn-BD", "ccp-BD",
        "en-IE", "ga-IE", "en-MY", "ms-MY",
    ]
    .contains(&locale)
}

#[cfg(target_os = "macos")]
mod macos {
    use jiff::ZonedDateTime;
    use objc2::rc::Retained;
    use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};

    thread_local! {
        static DATE_FORMATTER: Retained<NSDateFormatter> = {
            let formatter = NSDateFormatter::new();
            formatter.setDateStyle(NSDateFormatterStyle::ShortStyle);
            formatter
        };
        static TIME_FORMATTER: Retained<NSDateFormatter> = {
            let formatter = NSDateFormatter::new();
            formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
            formatter
        };
    }

    pub(crate) fn format_date(timestamp: &ZonedDateTime) -> String {
        DATE_FORMATTER.with(|formatter| format_with_date_formatter(timestamp, formatter))
    }

    pub(crate) fn format_time(timestamp: &ZonedDateTime) -> String {
        TIME_FORMATTER.with(|formatter| format_with_date_formatter(timestamp, formatter))
    }

    fn format_with_date_formatter(
        timestamp: &ZonedDateTime,
        formatter: &NSDateFormatter,
    ) -> String {
        let date = NSDate::dateWithTimeIntervalSince1970(
            timestamp.timestamp().as_duration().as_secs_f64(),
        );
        formatter.stringFromDate(&date).to_string()
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use jiff::ZonedDateTime;
    use windows::{Foundation::DateTime, Globalization::DateTimeFormatting::DateTimeFormatter};

    const UNIX_EPOCH_IN_WINDOWS_TICKS: i64 = 116_444_736_000_000_000;

    pub(crate) fn format_date(timestamp: &ZonedDateTime) -> String {
        format_with_formatter(DateTimeFormatter::ShortDate(), timestamp).unwrap_or_else(|error| {
            log::error!("Failed to format date: {error:#}");
            super::format_timestamp_naive_date(timestamp, true)
        })
    }

    pub(crate) fn format_time(timestamp: &ZonedDateTime) -> String {
        format_with_formatter(DateTimeFormatter::ShortTime(), timestamp).unwrap_or_else(|error| {
            log::error!("Failed to format time: {error:#}");
            super::format_timestamp_naive_time(timestamp, true)
        })
    }

    fn format_with_formatter(
        formatter: windows::core::Result<DateTimeFormatter>,
        timestamp: &ZonedDateTime,
    ) -> anyhow::Result<String> {
        let formatted = formatter?.Format(to_winrt_datetime(timestamp)?)?;
        Ok(formatted.to_string())
    }

    fn to_winrt_datetime(timestamp: &ZonedDateTime) -> anyhow::Result<DateTime> {
        let universal_time = i64::try_from(timestamp.timestamp().as_nanosecond() / 100)?
            + UNIX_EPOCH_IN_WINDOWS_TICKS;
        Ok(DateTime {
            UniversalTime: universal_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use jiff::{SignedDuration, Span};

    fn create_zoned_datetime(
        year: i16,
        month: i8,
        day: i8,
        hour: i8,
        minute: i8,
        second: i8,
    ) -> ZonedDateTime {
        jiff::civil::datetime(year, month, day, hour, minute, second, 0)
            .to_zoned_datetime(TimeZone::UTC)
            .unwrap()
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn test_format_absolute_time() {
        let timestamp = create_zoned_datetime(1990, 4, 12, 9, 30, 0);

        assert!(!format_absolute_time(&timestamp).is_empty());
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn test_format_timestamp_naive_time() {
        let timestamp = create_zoned_datetime(1990, 4, 12, 9, 30, 0);
        assert_eq!(format_timestamp_naive_time(&timestamp, true), "9:30 AM");
        assert_eq!(format_timestamp_naive_time(&timestamp, false), "09:30");

        let timestamp_pm = create_zoned_datetime(1990, 4, 12, 15, 45, 0);
        assert_eq!(format_timestamp_naive_time(&timestamp_pm, true), "3:45 PM");
        assert_eq!(format_timestamp_naive_time(&timestamp_pm, false), "15:45");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_24_hour_time() {
        let reference = create_zoned_datetime(1990, 4, 12, 16, 45, 0);
        let timestamp = create_zoned_datetime(1990, 4, 12, 15, 30, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, false),
            "Today at 15:30"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_today() {
        let reference = create_zoned_datetime(1990, 4, 12, 16, 45, 0);
        let timestamp = create_zoned_datetime(1990, 4, 12, 15, 30, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Today at 3:30 PM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_yesterday() {
        let reference = create_zoned_datetime(1990, 4, 12, 10, 30, 0);
        let timestamp = create_zoned_datetime(1990, 4, 11, 9, 0, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Yesterday at 9:00 AM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_yesterday_less_than_24_hours_ago() {
        let reference = create_zoned_datetime(1990, 4, 12, 19, 59, 0);
        let timestamp = create_zoned_datetime(1990, 4, 11, 20, 0, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Yesterday at 8:00 PM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_yesterday_more_than_24_hours_ago() {
        let reference = create_zoned_datetime(1990, 4, 12, 19, 59, 0);
        let timestamp = create_zoned_datetime(1990, 4, 11, 18, 0, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Yesterday at 6:00 PM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_yesterday_over_midnight() {
        let reference = create_zoned_datetime(1990, 4, 12, 0, 5, 0);
        let timestamp = create_zoned_datetime(1990, 4, 11, 23, 55, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Yesterday at 11:55 PM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_yesterday_over_month() {
        let reference = create_zoned_datetime(1990, 4, 2, 9, 0, 0);
        let timestamp = create_zoned_datetime(1990, 4, 1, 20, 0, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "Yesterday at 8:00 PM"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_format_before_yesterday() {
        let reference = create_zoned_datetime(1990, 4, 12, 10, 30, 0);
        let timestamp = create_zoned_datetime(1990, 4, 10, 20, 20, 0);

        assert_eq!(
            format_timestamp_naive(&timestamp, &reference, true),
            "04/10/1990 8:20 PM"
        );
    }

    #[test]
    fn test_relative_format_minutes() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);
        let mut current_timestamp = reference.clone();

        let mut next_minute = || {
            current_timestamp = current_timestamp
                .checked_sub(SignedDuration::from_mins(1))
                .unwrap();
            current_timestamp.clone()
        };

        assert_eq!(
            format_relative_time(&reference, &reference),
            Some("Just now".to_string())
        );

        assert_eq!(
            format_relative_time(&next_minute(), &reference),
            Some("1 minute ago".to_string())
        );

        for minutes in 2..=59 {
            assert_eq!(
                format_relative_time(&next_minute(), &reference),
                Some(format!("{minutes} minutes ago"))
            );
        }

        assert_eq!(
            format_relative_time(&next_minute(), &reference),
            Some("1 hour ago".to_string())
        );
    }

    #[test]
    fn test_relative_format_hours() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);
        let mut current_timestamp = reference.clone();

        let mut next_hour = || {
            current_timestamp = current_timestamp
                .checked_sub(SignedDuration::from_hours(1))
                .unwrap();
            current_timestamp.clone()
        };

        assert_eq!(
            format_relative_time(&next_hour(), &reference),
            Some("1 hour ago".to_string())
        );

        for hours in 2..=23 {
            assert_eq!(
                format_relative_time(&next_hour(), &reference),
                Some(format!("{hours} hours ago"))
            );
        }

        assert_eq!(format_relative_time(&next_hour(), &reference), None);
    }

    #[test]
    fn test_relative_format_days() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);
        let mut current_timestamp = reference.clone();

        let mut next_day = || {
            current_timestamp = current_timestamp.yesterday().unwrap();
            current_timestamp.clone()
        };

        assert_eq!(
            format_relative_date(&reference, &reference),
            "Today".to_string()
        );

        assert_eq!(
            format_relative_date(&next_day(), &reference),
            "Yesterday".to_string()
        );

        for days in 2..=6 {
            assert_eq!(
                format_relative_date(&next_day(), &reference),
                format!("{days} days ago")
            );
        }

        assert_eq!(format_relative_date(&next_day(), &reference), "1 week ago");
    }

    #[test]
    fn test_relative_format_weeks() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);
        let mut current_timestamp = reference.clone();

        let mut next_week = || {
            for _ in 0..7 {
                current_timestamp = current_timestamp.yesterday().unwrap();
            }
            current_timestamp.clone()
        };

        assert_eq!(
            format_relative_date(&next_week(), &reference),
            "1 week ago".to_string()
        );

        for weeks in 2..=4 {
            assert_eq!(
                format_relative_date(&next_week(), &reference),
                format!("{weeks} weeks ago")
            );
        }

        assert_eq!(
            format_relative_date(&next_week(), &reference),
            "1 month ago"
        );
    }

    #[test]
    fn test_relative_format_months() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);
        let mut current_timestamp = reference.clone();

        let mut next_month = || {
            current_timestamp = current_timestamp
                .checked_sub(Span::new().months(1))
                .unwrap();
            current_timestamp.clone()
        };

        assert_eq!(
            format_relative_date(&next_month(), &reference),
            "4 weeks ago".to_string()
        );

        for months in 2..=11 {
            assert_eq!(
                format_relative_date(&next_month(), &reference),
                format!("{months} months ago")
            );
        }

        assert_eq!(
            format_relative_date(&next_month(), &reference),
            "1 year ago"
        );
    }

    #[test]
    fn test_relative_format_years() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);

        assert_eq!(
            format_relative_date(&create_zoned_datetime(1989, 4, 12, 23, 0, 0), &reference),
            "1 year ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1989, 3, 12, 23, 0, 0), &reference),
            "1 year, 1 month ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1988, 6, 12, 23, 0, 0), &reference),
            "1 year, 10 months ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1988, 5, 12, 23, 0, 0), &reference),
            "1 year, 11 months ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1988, 4, 12, 23, 0, 0), &reference),
            "2 years ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1988, 3, 12, 23, 0, 0), &reference),
            "2 years, 1 month ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1987, 5, 12, 23, 0, 0), &reference),
            "2 years, 11 months ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1987, 4, 12, 23, 0, 0), &reference),
            "3 years ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1987, 3, 12, 23, 0, 0), &reference),
            "3 years, 1 month ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1985, 5, 12, 23, 0, 0), &reference),
            "4 years, 11 months ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1985, 4, 12, 23, 0, 0), &reference),
            "5 years ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1984, 11, 12, 23, 0, 0), &reference),
            "5 years ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1984, 10, 12, 23, 0, 0), &reference),
            "6 years ago"
        );
        assert_eq!(
            format_relative_date(&create_zoned_datetime(1980, 4, 12, 23, 0, 0), &reference),
            "10 years ago"
        );
    }

    #[test]
    fn test_calculate_month_difference() {
        let reference = create_zoned_datetime(1990, 4, 12, 23, 0, 0);

        assert_eq!(calculate_month_difference(&reference, &reference), 0);
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1990, 1, 12, 23, 0, 0), &reference),
            3
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1989, 11, 12, 23, 0, 0), &reference),
            5
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1989, 4, 12, 23, 0, 0), &reference),
            12
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1989, 3, 12, 23, 0, 0), &reference),
            13
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1987, 5, 12, 23, 0, 0), &reference),
            35
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1987, 4, 12, 23, 0, 0), &reference),
            36
        );
        assert_eq!(
            calculate_month_difference(&create_zoned_datetime(1987, 3, 12, 23, 0, 0), &reference),
            37
        );
    }
}
