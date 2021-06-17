use std::fmt;
use std::time::Duration;

use number_prefix::NumberPrefix;

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const YEAR: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// Wraps an std duration for human basic formatting.
#[derive(Debug)]
pub struct FormattedDuration(pub Duration);

/// Wraps an std duration for human readable formatting.
#[derive(Debug)]
pub struct HumanDuration(pub Duration);

/// Formats bytes for human readability
#[derive(Debug)]
pub struct HumanBytes(pub u64);

/// Formats bytes for human readability using SI prefixes
#[derive(Debug)]
pub struct DecimalBytes(pub u64);

/// Formats bytes for human readability using ISO/IEC prefixes
#[derive(Debug)]
pub struct BinaryBytes(pub u64);

impl fmt::Display for FormattedDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

// `HumanDuration` should be as intuitively understandable as possible.
// So we want to round, not truncate: otherwise 1 hour and 59 minutes
// would display an ETA of "1 hour" which underestimates the time
// remaining by a factor 2.
//
// To make the precision more uniform, we avoid displaying "1 unit"
// (except for seconds), because it would be displayed for a relatively
// long duration compared to the unit itself. Instead, when we arrive
// around 1.5 unit, we change from "2 units" to the next smaller unit
// (e.g. "89 seconds").
//
// Formally:
// * for n >= 2, we go from "n+1 units" to "n units" exactly at (n + 1/2) units
// * we switch from "2 units" to the next smaller unit at (1.5 unit minus half of the next smaller unit)

const UNITS_NAMES_ALTS: &[(Duration, &str, &str)] = &[
    (YEAR, "year", "y"),
    (WEEK, "week", "w"),
    (DAY, "day", "d"),
    (HOUR, "hour", "h"),
    (MINUTE, "minute", "m"),
    (SECOND, "second", "s"),
];

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // FIXME when `div_duration_f64` is stable
        let t = self.0.as_secs_f64();
        for ((unit, name, alt), (nextunit, _, _)) in
            UNITS_NAMES_ALTS.iter().zip(UNITS_NAMES_ALTS[1..].iter())
        {
            if self.0 + *nextunit / 2 >= *unit + *unit / 2 {
                let x = (t / unit.as_secs_f64()).round() as usize;
                return if f.alternate() {
                    write!(f, "{}{}", x.max(2), alt)
                } else {
                    write!(f, "{} {}s", x.max(2), name)
                };
            }
        }
        // unwrap is safe because it doesn't make sense to call
        // this function with an empty table of units
        let (unit, name, alt) = UNITS_NAMES_ALTS.last().unwrap();
        let x = (t / unit.as_secs_f64()).round() as usize;
        if f.alternate() {
            write!(f, "{}{}", x, alt)
        } else {
            write!(f, "{} {}{}", x, name, if x == 1 { "" } else { "s" })
        }
    }
}

impl fmt::Display for HumanBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match NumberPrefix::binary(self.0 as f64) {
            NumberPrefix::Standalone(number) => write!(f, "{:.0}B", number),
            NumberPrefix::Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix),
        }
    }
}

impl fmt::Display for DecimalBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match NumberPrefix::decimal(self.0 as f64) {
            NumberPrefix::Standalone(number) => write!(f, "{:.0}B", number),
            NumberPrefix::Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix),
        }
    }
}

impl fmt::Display for BinaryBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match NumberPrefix::binary(self.0 as f64) {
            NumberPrefix::Standalone(number) => write!(f, "{:.0}B", number),
            NumberPrefix::Prefixed(prefix, number) => write!(f, "{:.2}{}B", number, prefix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MILLI: Duration = Duration::from_millis(1);

    #[test]
    fn human_duration_alternate() {
        for (unit, _, alt) in UNITS_NAMES_ALTS {
            assert_eq!(
                format!("2{}", alt),
                format!("{:#}", HumanDuration(2 * *unit))
            );
        }
    }

    #[test]
    fn human_duration_less_than_one_second() {
        assert_eq!("0 seconds", format!("{}", HumanDuration(Duration::ZERO)));
        assert_eq!("0 seconds", format!("{}", HumanDuration(MILLI)));
        assert_eq!("0 seconds", format!("{}", HumanDuration(499 * MILLI)));
        assert_eq!("1 second", format!("{}", HumanDuration(500 * MILLI)));
        assert_eq!("1 second", format!("{}", HumanDuration(999 * MILLI)));
    }

    #[test]
    fn human_duration_less_than_two_seconds() {
        assert_eq!("1 second", format!("{}", HumanDuration(1499 * MILLI)));
        assert_eq!("2 seconds", format!("{}", HumanDuration(1500 * MILLI)));
        assert_eq!("2 seconds", format!("{}", HumanDuration(1999 * MILLI)));
    }

    #[test]
    fn human_duration_one_unit() {
        assert_eq!("1 second", format!("{}", HumanDuration(SECOND)));
        assert_eq!("60 seconds", format!("{}", HumanDuration(MINUTE)));
        assert_eq!("60 minutes", format!("{}", HumanDuration(HOUR)));
        assert_eq!("24 hours", format!("{}", HumanDuration(DAY)));
        assert_eq!("7 days", format!("{}", HumanDuration(WEEK)));
        assert_eq!("52 weeks", format!("{}", HumanDuration(YEAR)));
    }

    #[test]
    fn human_duration_less_than_one_and_a_half_unit() {
        // this one is actually done at 1.5 unit - half of the next smaller unit - epsilon
        // and should display the next smaller unit
        let d = HumanDuration(MINUTE + MINUTE / 2 - SECOND / 2 - MILLI);
        assert_eq!("89 seconds", format!("{}", d));
        let d = HumanDuration(HOUR + HOUR / 2 - MINUTE / 2 - MILLI);
        assert_eq!("89 minutes", format!("{}", d));
        let d = HumanDuration(DAY + DAY / 2 - HOUR / 2 - MILLI);
        assert_eq!("35 hours", format!("{}", d));
        let d = HumanDuration(WEEK + WEEK / 2 - DAY / 2 - MILLI);
        assert_eq!("10 days", format!("{}", d));
        let d = HumanDuration(YEAR + YEAR / 2 - WEEK / 2 - MILLI);
        assert_eq!("78 weeks", format!("{}", d));
    }

    #[test]
    fn human_duration_one_and_a_half_unit() {
        // this one is actually done at 1.5 unit - half of the next smaller unit
        // and should still display "2 units"
        let d = HumanDuration(MINUTE + MINUTE / 2 - SECOND / 2);
        assert_eq!("2 minutes", format!("{}", d));
        let d = HumanDuration(HOUR + HOUR / 2 - MINUTE / 2);
        assert_eq!("2 hours", format!("{}", d));
        let d = HumanDuration(DAY + DAY / 2 - HOUR / 2);
        assert_eq!("2 days", format!("{}", d));
        let d = HumanDuration(WEEK + WEEK / 2 - DAY / 2);
        assert_eq!("2 weeks", format!("{}", d));
        let d = HumanDuration(YEAR + YEAR / 2 - WEEK / 2);
        assert_eq!("2 years", format!("{}", d));
    }

    #[test]
    fn human_duration_two_units() {
        assert_eq!("2 seconds", format!("{}", HumanDuration(2 * SECOND)));
        assert_eq!("2 minutes", format!("{}", HumanDuration(2 * MINUTE)));
        assert_eq!("2 hours", format!("{}", HumanDuration(2 * HOUR)));
        assert_eq!("2 days", format!("{}", HumanDuration(2 * DAY)));
        assert_eq!("2 weeks", format!("{}", HumanDuration(2 * WEEK)));
        assert_eq!("2 years", format!("{}", HumanDuration(2 * YEAR)));
    }

    #[test]
    fn human_duration_less_than_two_and_a_half_units() {
        let d = HumanDuration(2 * SECOND + SECOND / 2 - MILLI);
        assert_eq!("2 seconds", format!("{}", d));
        let d = HumanDuration(2 * MINUTE + MINUTE / 2 - MILLI);
        assert_eq!("2 minutes", format!("{}", d));
        let d = HumanDuration(2 * HOUR + HOUR / 2 - MILLI);
        assert_eq!("2 hours", format!("{}", d));
        let d = HumanDuration(2 * DAY + DAY / 2 - MILLI);
        assert_eq!("2 days", format!("{}", d));
        let d = HumanDuration(2 * WEEK + WEEK / 2 - MILLI);
        assert_eq!("2 weeks", format!("{}", d));
        let d = HumanDuration(2 * YEAR + YEAR / 2 - MILLI);
        assert_eq!("2 years", format!("{}", d));
    }

    #[test]
    fn human_duration_two_and_a_half_units() {
        let d = HumanDuration(2 * SECOND + SECOND / 2);
        assert_eq!("3 seconds", format!("{}", d));
        let d = HumanDuration(2 * MINUTE + MINUTE / 2);
        assert_eq!("3 minutes", format!("{}", d));
        let d = HumanDuration(2 * HOUR + HOUR / 2);
        assert_eq!("3 hours", format!("{}", d));
        let d = HumanDuration(2 * DAY + DAY / 2);
        assert_eq!("3 days", format!("{}", d));
        let d = HumanDuration(2 * WEEK + WEEK / 2);
        assert_eq!("3 weeks", format!("{}", d));
        let d = HumanDuration(2 * YEAR + YEAR / 2);
        assert_eq!("3 years", format!("{}", d));
    }

    #[test]
    fn human_duration_three_units() {
        assert_eq!("3 seconds", format!("{}", HumanDuration(3 * SECOND)));
        assert_eq!("3 minutes", format!("{}", HumanDuration(3 * MINUTE)));
        assert_eq!("3 hours", format!("{}", HumanDuration(3 * HOUR)));
        assert_eq!("3 days", format!("{}", HumanDuration(3 * DAY)));
        assert_eq!("3 weeks", format!("{}", HumanDuration(3 * WEEK)));
        assert_eq!("3 years", format!("{}", HumanDuration(3 * YEAR)));
    }
}
