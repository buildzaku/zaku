use jiff::{Timestamp, tz::TimeZone};

use time_format::TimestampFormat;

pub(crate) fn format_git_timestamp(timestamp: Timestamp, format: TimestampFormat) -> String {
    time_format::format_localized_timestamp(timestamp, Timestamp::now(), TimeZone::system(), format)
}
