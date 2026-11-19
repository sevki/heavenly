//! Epochs and astronomical time scales.
//!
//! An [`Epoch`] is a single instant. Internally it is stored as an integer
//! number of nanoseconds of Terrestrial Time (TT) since J2000.0
//! (2000-01-01T12:00:00 TT). Conversions to other scales are applied only at
//! the boundary:
//!
//! | Scale | Relation used                                                        |
//! |-------|----------------------------------------------------------------------|
//! | TAI   | `TT = TAI + 32.184 s` (exact, by definition)                         |
//! | UTC   | `TAI = UTC + ΔAT`, using the built-in IERS leap-second table         |
//! | TDB   | `TDB ≈ TT + 1.657 ms·sin g + 13.85 µs·sin 2g` (≈ 30 µs accuracy)     |
//!
//! The TDB model is the conventional two-term approximation. Applications that
//! need better than tens of microseconds should obtain TDB from an ephemeris
//! (e.g. a SPICE adapter) rather than from this crate.
//!
//! The leap-second table ends with ΔAT = 37 s from 2017-01-01. UTC instants
//! after the last entry are assumed to keep the last value; the table must be
//! updated when IERS announces a new leap second. UTC before 1972-01-01 is
//! rejected with [`Error::UtcOutOfRange`].

use core::fmt;
use core::str::FromStr;

use crate::Error;

const NS: i128 = 1_000_000_000;
const DAY_NS: i128 = 86_400 * NS;
const TT_MINUS_TAI_NS: i128 = 32_184_000_000;
/// Days from 1970-01-01 to 2000-01-01.
const J2000_DAYS: i64 = days_from_civil(2000, 1, 1);

/// A time scale in which a calendar date or a count of seconds is expressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimeScale {
    /// Coordinated Universal Time (with leap seconds).
    Utc,
    /// International Atomic Time.
    Tai,
    /// Terrestrial Time.
    Tt,
    /// Barycentric Dynamical Time (the scale of most planetary ephemerides).
    Tdb,
}

impl TimeScale {
    pub const fn abbreviation(self) -> &'static str {
        match self {
            TimeScale::Utc => "UTC",
            TimeScale::Tai => "TAI",
            TimeScale::Tt => "TT",
            TimeScale::Tdb => "TDB",
        }
    }
}

impl fmt::Display for TimeScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abbreviation())
    }
}

/// A broken-down Gregorian calendar date and time of day in a given scale.
///
/// `second` may be 60 only for a UTC leap second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calendar {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
    pub scale: TimeScale,
}

impl fmt::Display for Calendar {
    /// ISO 8601-style, e.g. `2026-11-19T17:00:00Z` or `2000-01-01T12:00:00 TT`.
    /// The output is accepted by [`Epoch::parse`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )?;
        if self.nanosecond != 0 {
            let (mut n, mut width) = (self.nanosecond, 9usize);
            while n % 10 == 0 {
                n /= 10;
                width -= 1;
            }
            write!(f, ".{n:0width$}")?;
        }
        match self.scale {
            TimeScale::Utc => f.write_str("Z"),
            s => write!(f, " {s}"),
        }
    }
}

/// An instant in time, independent of any time scale.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Epoch {
    /// Nanoseconds of TT since J2000.0 TT.
    tt_ns: i128,
}

impl Epoch {
    /// J2000.0: 2000-01-01T12:00:00 TT.
    pub const J2000: Epoch = Epoch { tt_ns: 0 };

    /// Builds an epoch from whole nanoseconds of TT since J2000.0.
    pub const fn from_tt_nanoseconds_since_j2000(ns: i128) -> Epoch {
        Epoch { tt_ns: ns }
    }

    /// Whole nanoseconds of TT since J2000.0.
    pub const fn tt_nanoseconds_since_j2000(self) -> i128 {
        self.tt_ns
    }

    /// Builds an epoch from (possibly fractional) TT seconds since J2000.0.
    pub fn from_tt_seconds_since_j2000(s: f64) -> Result<Epoch, Error> {
        Ok(Epoch {
            tt_ns: seconds_to_ns(s)?,
        })
    }

    /// TT seconds since J2000.0.
    pub fn tt_seconds_since_j2000(self) -> f64 {
        ns_to_seconds(self.tt_ns)
    }

    /// Builds an epoch from TDB seconds since J2000.0 (SPICE "ephemeris time").
    pub fn from_tdb_seconds_since_j2000(s: f64) -> Result<Epoch, Error> {
        let tdb = seconds_to_ns(s)?;
        Ok(Epoch {
            tt_ns: tdb_to_tt_ns(tdb),
        })
    }

    /// TDB seconds since J2000.0 (SPICE "ephemeris time").
    pub fn tdb_seconds_since_j2000(self) -> f64 {
        self.tt_seconds_since_j2000() + tdb_minus_tt_seconds(self.tt_ns)
    }

    /// Builds an epoch from a calendar date in the given scale.
    pub fn from_calendar(c: Calendar) -> Result<Epoch, Error> {
        if !(1..=12).contains(&c.month)
            || c.day < 1
            || c.day > days_in_month(c.year as i64, c.month)
            || c.hour > 23
            || c.minute > 59
            || c.second > 60
            || c.nanosecond >= NS as u32
        {
            return Err(Error::InvalidCalendar);
        }
        let days = days_from_civil(c.year as i64, c.month as u32, c.day as u32);
        if c.second == 60 {
            let is_leap_second = c.scale == TimeScale::Utc
                && c.hour == 23
                && c.minute == 59
                && matches!(
                    (delta_at(days), delta_at(days + 1)),
                    (Some(a), Some(b)) if b == a + 1
                );
            if !is_leap_second {
                return Err(Error::InvalidCalendar);
            }
        }
        let sod_ns = (c.hour as i128 * 3600 + c.minute as i128 * 60 + c.second as i128) * NS
            + c.nanosecond as i128;
        let naive = naive_ns(days, sod_ns);
        let tt_ns = match c.scale {
            TimeScale::Tt => naive,
            TimeScale::Tai => naive + TT_MINUS_TAI_NS,
            TimeScale::Tdb => tdb_to_tt_ns(naive),
            TimeScale::Utc => {
                let dat = delta_at(days).ok_or(Error::UtcOutOfRange)?;
                naive + dat as i128 * NS + TT_MINUS_TAI_NS
            }
        };
        Ok(Epoch { tt_ns })
    }

    /// Convenience wrapper around [`Epoch::from_calendar`].
    #[allow(clippy::too_many_arguments)]
    pub fn from_gregorian(
        scale: TimeScale,
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanosecond: u32,
    ) -> Result<Epoch, Error> {
        Epoch::from_calendar(Calendar {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond,
            scale,
        })
    }

    /// Breaks the epoch down into a calendar date in the given scale.
    pub fn to_calendar(self, scale: TimeScale) -> Result<Calendar, Error> {
        match scale {
            TimeScale::Tt => calendar_from_naive(self.tt_ns, scale),
            TimeScale::Tai => calendar_from_naive(self.tt_ns - TT_MINUS_TAI_NS, scale),
            TimeScale::Tdb => calendar_from_naive(
                self.tt_ns + seconds_to_ns_rounded(tdb_minus_tt_seconds(self.tt_ns)),
                scale,
            ),
            TimeScale::Utc => self.to_utc_calendar(),
        }
    }

    /// Parses `YYYY-MM-DDTHH:MM:SS[.fffffffff]` followed by an explicit time
    /// scale: `Z` (UTC) or a space and one of `UTC`, `TAI`, `TT`, `TDB`.
    ///
    /// A string without a time scale is rejected rather than guessed.
    pub fn parse(s: &str) -> Result<Epoch, Error> {
        Epoch::from_calendar(parse_calendar(s)?)
    }

    /// Elapsed SI seconds (on the TT/TAI scale) from `earlier` to `self`.
    pub fn seconds_since(self, earlier: Epoch) -> f64 {
        ns_to_seconds(self.tt_ns - earlier.tt_ns)
    }

    /// Returns `self` advanced by `s` SI seconds on the TT/TAI scale.
    pub fn add_seconds(self, s: f64) -> Result<Epoch, Error> {
        Ok(Epoch {
            tt_ns: self.tt_ns + seconds_to_ns(s)?,
        })
    }

    /// Returns `self` advanced by `ns` nanoseconds on the TT/TAI scale.
    pub const fn add_nanoseconds(self, ns: i128) -> Epoch {
        Epoch {
            tt_ns: self.tt_ns + ns,
        }
    }

    /// TAI − UTC in whole seconds at this instant (ΔAT). During an inserted
    /// leap second the value before the leap applies.
    pub fn tai_minus_utc_seconds(self) -> Result<i64, Error> {
        let tai = self.tt_ns - TT_MINUS_TAI_NS;
        LEAP_SECONDS
            .iter()
            .rev()
            .find(|&&(y, m, d, dat)| {
                naive_ns(days_from_civil(y, m, d), 0) + dat as i128 * NS <= tai
            })
            .map(|e| e.3)
            .ok_or(Error::UtcOutOfRange)
    }

    fn to_utc_calendar(self) -> Result<Calendar, Error> {
        let tai = self.tt_ns - TT_MINUS_TAI_NS;
        let tai_start = |i: usize| {
            let (y, m, d, dat) = LEAP_SECONDS[i];
            naive_ns(days_from_civil(y, m, d), 0) + dat as i128 * NS
        };
        let i = (0..LEAP_SECONDS.len())
            .rev()
            .find(|&i| tai_start(i) <= tai)
            .ok_or(Error::UtcOutOfRange)?;
        let dat = LEAP_SECONDS[i].3 as i128;
        if let Some(next) = LEAP_SECONDS.get(i + 1) {
            let step = next.3 as i128 - dat;
            if step > 0 && tai >= tai_start(i + 1) - step * NS {
                // Inside an inserted leap second: label it 23:59:60.
                let mut c = calendar_from_naive(tai - dat * NS - NS, TimeScale::Utc)?;
                c.second += 1;
                return Ok(c);
            }
        }
        calendar_from_naive(tai - dat * NS, TimeScale::Utc)
    }
}

impl FromStr for Epoch {
    type Err = Error;
    fn from_str(s: &str) -> Result<Epoch, Error> {
        Epoch::parse(s)
    }
}

impl fmt::Debug for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_calendar(TimeScale::Tt) {
            Ok(c) => write!(f, "Epoch({c})"),
            Err(_) => write!(f, "Epoch({} ns TT since J2000)", self.tt_ns),
        }
    }
}

impl fmt::Display for Epoch {
    /// Formats in TT, which is defined for every representable epoch.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_calendar(TimeScale::Tt) {
            Ok(c) => c.fmt(f),
            Err(_) => write!(f, "{} ns TT since J2000", self.tt_ns),
        }
    }
}

/// (year, month, day, TAI−UTC in seconds from 00:00 UTC on that date).
const LEAP_SECONDS: [(i64, u32, u32, i64); 28] = [
    (1972, 1, 1, 10),
    (1972, 7, 1, 11),
    (1973, 1, 1, 12),
    (1974, 1, 1, 13),
    (1975, 1, 1, 14),
    (1976, 1, 1, 15),
    (1977, 1, 1, 16),
    (1978, 1, 1, 17),
    (1979, 1, 1, 18),
    (1980, 1, 1, 19),
    (1981, 7, 1, 20),
    (1982, 7, 1, 21),
    (1983, 7, 1, 22),
    (1985, 7, 1, 23),
    (1988, 1, 1, 24),
    (1990, 1, 1, 25),
    (1991, 1, 1, 26),
    (1992, 7, 1, 27),
    (1993, 7, 1, 28),
    (1994, 7, 1, 29),
    (1996, 1, 1, 30),
    (1997, 7, 1, 31),
    (1999, 1, 1, 32),
    (2006, 1, 1, 33),
    (2009, 1, 1, 34),
    (2012, 7, 1, 35),
    (2015, 7, 1, 36),
    (2017, 1, 1, 37),
];

/// TAI−UTC in whole seconds for the UTC day `days` (since 1970-01-01).
fn delta_at(days: i64) -> Option<i64> {
    LEAP_SECONDS
        .iter()
        .rev()
        .find(|&&(y, m, d, _)| days_from_civil(y, m, d) <= days)
        .map(|e| e.3)
}

/// Nanoseconds from J2000.0 (noon) to `sod_ns` into day `days`, ignoring leap
/// seconds — i.e. on a uniform 86 400 s/day calendar.
fn naive_ns(days: i64, sod_ns: i128) -> i128 {
    (days - J2000_DAYS) as i128 * DAY_NS - DAY_NS / 2 + sod_ns
}

fn calendar_from_naive(naive: i128, scale: TimeScale) -> Result<Calendar, Error> {
    let since_midnight = naive + DAY_NS / 2;
    let day_index = since_midnight.div_euclid(DAY_NS);
    let sod = since_midnight.rem_euclid(DAY_NS);
    let days = i64::try_from(day_index + J2000_DAYS as i128).map_err(|_| Error::InvalidCalendar)?;
    let (y, m, d) = civil_from_days(days);
    let secs = (sod / NS) as u32;
    Ok(Calendar {
        year: i32::try_from(y).map_err(|_| Error::InvalidCalendar)?,
        month: m as u8,
        day: d as u8,
        hour: (secs / 3600) as u8,
        minute: (secs / 60 % 60) as u8,
        second: (secs % 60) as u8,
        nanosecond: (sod % NS) as u32,
        scale,
    })
}

fn tdb_minus_tt_seconds(tt_ns: i128) -> f64 {
    let days = tt_ns as f64 / DAY_NS as f64;
    let g = (357.53 + 0.985_600_28 * days) * (core::f64::consts::PI / 180.0);
    0.001_657 * libm::sin(g) + 0.000_013_85 * libm::sin(2.0 * g)
}

fn tdb_to_tt_ns(tdb_ns: i128) -> i128 {
    // Fixed-point iteration; the correction varies by < 1e-9 per step after two.
    let mut tt = tdb_ns;
    for _ in 0..3 {
        tt = tdb_ns - seconds_to_ns_rounded(tdb_minus_tt_seconds(tt));
    }
    tt
}

fn seconds_to_ns(s: f64) -> Result<i128, Error> {
    if !s.is_finite() {
        return Err(Error::NonFinite);
    }
    let whole = libm::trunc(s);
    Ok(whole as i128 * NS + libm::round((s - whole) * 1e9) as i128)
}

fn seconds_to_ns_rounded(s: f64) -> i128 {
    libm::round(s * 1e9) as i128
}

fn ns_to_seconds(ns: i128) -> f64 {
    (ns / NS) as f64 + (ns % NS) as f64 * 1e-9
}

const fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

const fn days_in_month(y: i64, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant).
const fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn parse_calendar(s: &str) -> Result<Calendar, Error> {
    let b = s.as_bytes();
    let mut i = 0usize;

    fn digits(b: &[u8], i: &mut usize, n: usize) -> Result<u32, Error> {
        let mut v = 0u32;
        for _ in 0..n {
            match b.get(*i) {
                Some(c @ b'0'..=b'9') => v = v * 10 + (c - b'0') as u32,
                _ => return Err(Error::Parse("expected digit")),
            }
            *i += 1;
        }
        Ok(v)
    }
    fn expect(b: &[u8], i: &mut usize, c: u8, msg: &'static str) -> Result<(), Error> {
        if b.get(*i) == Some(&c) {
            *i += 1;
            Ok(())
        } else {
            Err(Error::Parse(msg))
        }
    }

    let year = digits(b, &mut i, 4)? as i32;
    expect(b, &mut i, b'-', "expected '-' after year")?;
    let month = digits(b, &mut i, 2)? as u8;
    expect(b, &mut i, b'-', "expected '-' after month")?;
    let day = digits(b, &mut i, 2)? as u8;
    expect(b, &mut i, b'T', "expected 'T' between date and time")?;
    let hour = digits(b, &mut i, 2)? as u8;
    expect(b, &mut i, b':', "expected ':' after hour")?;
    let minute = digits(b, &mut i, 2)? as u8;
    expect(b, &mut i, b':', "expected ':' after minute")?;
    let second = digits(b, &mut i, 2)? as u8;

    let mut nanosecond = 0u32;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while let Some(c @ b'0'..=b'9') = b.get(i) {
            if i - start == 9 {
                return Err(Error::Parse("more than 9 fractional digits"));
            }
            nanosecond = nanosecond * 10 + (c - b'0') as u32;
            i += 1;
        }
        if i == start {
            return Err(Error::Parse("expected digits after '.'"));
        }
        for _ in (i - start)..9 {
            nanosecond *= 10;
        }
    }

    let scale = match &s[i..] {
        "Z" | " UTC" => TimeScale::Utc,
        " TAI" => TimeScale::Tai,
        " TT" => TimeScale::Tt,
        " TDB" => TimeScale::Tdb,
        "" => {
            return Err(Error::Parse(
                "missing time scale (use Z, UTC, TAI, TT or TDB)",
            ))
        }
        _ => return Err(Error::Parse("unrecognised time scale suffix")),
    };

    Ok(Calendar {
        year,
        month,
        day,
        hour,
        minute,
        second,
        nanosecond,
        scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for z in [-800_000i64, -1, 0, 1, 10_957, 20_000, 2_932_896] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(J2000_DAYS, 10_957);
    }
}
