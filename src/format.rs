use std::fmt;
use std::time::Duration;

use number_prefix::{binary_prefix, decimal_prefix, PrefixNames, Prefixed, Standalone};

/// Wraps an std duration for human basic formatting.
pub struct FormattedDuration(pub Duration);

/// Wraps an std duration for human readable formatting.
pub struct HumanDuration(pub Duration);

/// Formats bytes for human readability
pub struct HumanBytes(pub u64);

/// Formats bytes for human readability using SI prefixes
pub struct DecimalBytes(pub u64);

/// Formats bytes for human readability using ISO/IEC prefixes
pub struct BinaryBytes(pub u64);

impl fmt::Display for FormattedDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut t = self.0.as_secs();
        let seconds = t % 60;
        t /= 60;
        let minutes = t % 60;
        t /= 60;
        let hours = t % 24;
        t /= 24;
        if t > 0 {
            let days = t;
            write!(f, "{}d {:02}:{:02}:{:02}", days, hours, minutes, seconds)
        } else {
            write!(f, "{:02}:{:02}:{:02}", hours, minutes, seconds)
        }
    }
}

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let t = self.0.as_secs();
        let alt = f.alternate();
        macro_rules! try_unit {
            ($secs:expr, $sg:expr, $pl:expr, $s:expr) => {
                let cnt = t / $secs;
                if cnt == 1 {
                    if alt {
                        return write!(f, "{}{}", cnt, $s);
                    } else {
                        return write!(f, "{} {}", cnt, $sg);
                    }
                } else if cnt > 1 {
                    if alt {
                        return write!(f, "{}{}", cnt, $s);
                    } else {
                        return write!(f, "{} {}", cnt, $pl);
                    }
                }
            }
        }

        try_unit!(365 * 24 * 60 * 60, "year", "years", "y");
        try_unit!(7 * 24 * 60 * 60, "week", "weeks", "w");
        try_unit!(24 * 60 * 60, "day", "days", "d");
        try_unit!(60 * 60, "hour", "hours", "h");
        try_unit!(60, "minute", "minutes", "m");
        try_unit!(1, "second", "seconds", "s");
        write!(f, "0{}", if alt { "s" } else { " seconds" })
    }
}

impl fmt::Display for HumanBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match binary_prefix(self.0 as f64) {
            Standalone(number) => write!(f, "{:.0}B", number),
            Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix.upper().chars().next().unwrap()),
        }
    }
}

impl fmt::Display for DecimalBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match decimal_prefix(self.0 as f64) {
            Standalone(number) => write!(f, "{:.0}B", number),
            Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix),
        }
    }
}

impl fmt::Display for BinaryBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match binary_prefix(self.0 as f64) {
            Standalone(number) => write!(f, "{:.0}B", number),
            Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix),
        }
    }
}
