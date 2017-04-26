use std::fmt;
use std::time::Duration;


/// Wraps an std duration for human basic formatting.
pub struct FormattedDuration(pub Duration);

/// Wraps an std duration for human readable formatting.
pub struct HumanDuration(pub Duration);

/// Formats bytes for human readability
pub struct HumanBytes(pub u64);

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
        let n = self.0 as f64;
        let kb = 1024f64;
        match n {
            n if n >= kb.powf(4_f64) => write!(f, "{:.*}TB", 2, n / kb.powf(4_f64)),
            n if n >= kb.powf(3_f64) => write!(f, "{:.*}GB", 2, n / kb.powf(3_f64)),
            n if n >= kb.powf(2_f64) => write!(f, "{:.*}MB", 2, n / kb.powf(2_f64)),
            n if n >= kb => write!(f, "{:.*}KB", 2, n / kb),
            _ => write!(f, "{:.*}B", 0, n)
        }
    }
}
