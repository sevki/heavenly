//! NORAD two-line element sets.

use heavenly_core::{Epoch, Error, TimeScale};

/// A parsed two-line element set.
///
/// Angles are in degrees and the mean motion in revolutions per day, as
/// written in the TLE. These are SGP4 *mean* elements: use them with
/// [`crate::Sgp4`], not as osculating elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tle {
    /// Catalogue number. Alpha-5 numbers (`A0000` = 100000) are decoded.
    pub catalog_number: u32,
    /// Security classification character (`U`, `C`, `S`).
    pub classification: u8,
    /// Two-digit epoch year as written (57–99 → 19xx, 00–56 → 20xx).
    pub epoch_year: i32,
    /// Day of the year, 1-based, with fraction.
    pub epoch_day: f64,
    /// First derivative of mean motion / 2, rev/day².
    pub mean_motion_dot: f64,
    /// Second derivative of mean motion / 6, rev/day³.
    pub mean_motion_ddot: f64,
    /// B* drag term, 1/Earth radii.
    pub bstar: f64,
    /// Element set number.
    pub element_set: u32,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub eccentricity: f64,
    pub arg_perigee_deg: f64,
    pub mean_anomaly_deg: f64,
    pub mean_motion_rev_day: f64,
    /// Revolution number at epoch.
    pub revolution: u32,
}

fn field(line: &[u8], from: usize, to: usize) -> Result<&str, Error> {
    // 1-based inclusive columns, as in the format description.
    let end = to.min(line.len());
    if from > end {
        return Err(Error::Parse("TLE line is too short"));
    }
    core::str::from_utf8(&line[from - 1..end]).map_err(|_| Error::Parse("TLE is not ASCII"))
}

fn float(s: &str, what: &'static str) -> Result<f64, Error> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(0.0);
    }
    t.parse::<f64>().map_err(|_| Error::Parse(what))
}

fn uint(s: &str, what: &'static str) -> Result<u32, Error> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(0);
    }
    t.parse::<u32>().map_err(|_| Error::Parse(what))
}

/// A number with an implied leading decimal point and a signed exponent,
/// such as ` 28098-4` (= 0.28098e-4) or `-11606-4`.
fn implied(s: &str, what: &'static str) -> Result<f64, Error> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(0.0);
    }
    let (sign, rest) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let split = rest
        .rfind(['-', '+'])
        .filter(|&i| i > 0)
        .ok_or(Error::Parse(what))?;
    let mantissa: f64 = float(&rest[..split], what)?;
    let digits = rest[..split].trim().len() as i32;
    let exponent: i32 = rest[split..].parse().map_err(|_| Error::Parse(what))?;
    Ok(sign * mantissa * libm::pow(10.0, (exponent - digits) as f64))
}

fn catalog(s: &str) -> Result<u32, Error> {
    let t = s.trim();
    let b = t.as_bytes();
    match b.first() {
        Some(c) if c.is_ascii_uppercase() => {
            // Alpha-5: A–Z without I and O stand for 10–33.
            let mut v = (*c - b'A') as u32 + 10;
            if *c > b'I' {
                v -= 1;
            }
            if *c > b'O' {
                v -= 1;
            }
            if *c == b'I' || *c == b'O' {
                return Err(Error::Parse("invalid Alpha-5 catalogue number"));
            }
            Ok(v * 10000 + uint(&t[1..], "invalid catalogue number")?)
        }
        _ => uint(t, "invalid catalogue number"),
    }
}

/// The TLE checksum of the first 68 columns: digits count their value and
/// `-` counts 1, modulo 10.
pub fn checksum(line: &str) -> u8 {
    let s: u32 = line
        .bytes()
        .take(68)
        .map(|c| match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'-' => 1,
            _ => 0,
        })
        .sum();
    (s % 10) as u8
}

impl Tle {
    /// Parses a TLE, verifying the line numbers, catalogue numbers and
    /// checksums.
    pub fn parse(line1: &str, line2: &str) -> Result<Tle, Error> {
        for l in [line1, line2] {
            let l = l.trim_end();
            if l.len() != 69 {
                return Err(Error::Parse("TLE lines must be 69 characters"));
            }
            let want = l.as_bytes()[68];
            if !want.is_ascii_digit() || want - b'0' != checksum(l) {
                return Err(Error::Parse("TLE checksum mismatch"));
            }
        }
        Tle::parse_unchecked(line1, line2)
    }

    /// Parses a TLE without checking the line length or checksums, for old
    /// or hand-edited element sets. Anything past column 69 is ignored.
    pub fn parse_unchecked(line1: &str, line2: &str) -> Result<Tle, Error> {
        let (l1, l2) = (line1.as_bytes(), line2.as_bytes());
        if l1.first() != Some(&b'1') || l2.first() != Some(&b'2') {
            return Err(Error::Parse("TLE lines must start with 1 and 2"));
        }
        let catalog_number = catalog(field(l1, 3, 7)?)?;
        if catalog(field(l2, 3, 7)?)? != catalog_number {
            return Err(Error::Parse("TLE lines have different catalogue numbers"));
        }
        let yy = uint(field(l1, 19, 20)?, "invalid epoch year")? as i32;
        let epoch_year = if yy < 57 { 2000 + yy } else { 1900 + yy };
        let epoch_day = float(field(l1, 21, 32)?, "invalid epoch day")?;
        if !(1.0..367.0).contains(&epoch_day) {
            return Err(Error::Parse("TLE epoch day out of range"));
        }
        let ecc = field(l2, 27, 33)?.trim();
        Ok(Tle {
            catalog_number,
            classification: l1.get(7).copied().unwrap_or(b'U'),
            epoch_year,
            epoch_day,
            mean_motion_dot: float(field(l1, 34, 43)?, "invalid mean motion derivative")?,
            mean_motion_ddot: implied(field(l1, 45, 52)?, "invalid second derivative")?,
            bstar: implied(field(l1, 54, 61)?, "invalid B*")?,
            element_set: uint(field(l1, 65, 68)?, "invalid element set number")?,
            inclination_deg: float(field(l2, 9, 16)?, "invalid inclination")?,
            raan_deg: float(field(l2, 18, 25)?, "invalid RAAN")?,
            eccentricity: if ecc.is_empty() {
                0.0
            } else {
                float(ecc, "invalid eccentricity")? * libm::pow(10.0, -(ecc.len() as f64))
            },
            arg_perigee_deg: float(field(l2, 35, 42)?, "invalid argument of perigee")?,
            mean_anomaly_deg: float(field(l2, 44, 51)?, "invalid mean anomaly")?,
            mean_motion_rev_day: float(field(l2, 53, 63)?, "invalid mean motion")?,
            revolution: uint(field(l2, 64, 68)?, "invalid revolution number")?,
        })
    }

    /// The epoch, which TLEs give in UTC.
    pub fn epoch(&self) -> Result<Epoch, Error> {
        let whole = libm::floor(self.epoch_day);
        let (m, d) = month_day(self.epoch_year, whole as u32 - 1);
        let midnight = Epoch::from_gregorian(TimeScale::Utc, self.epoch_year, m, d, 0, 0, 0, 0)?;
        // Leap seconds are inserted at the end of a day, so SI seconds from
        // UTC midnight equal UTC seconds of day.
        midnight.add_seconds((self.epoch_day - whole) * 86400.0)
    }

    /// The epoch as SGP4 counts it internally: days since 1949-12-31 00:00
    /// UTC, computed the way the reference implementation does (through
    /// hours, minutes and seconds and a split Julian date), so that results
    /// match it to the last bit.
    pub(crate) fn days_since_1950(&self) -> f64 {
        let day0 = libm::floor(self.epoch_day);
        let (mon, day) = month_day(self.epoch_year, day0 as u32 - 1);
        let mut temp = (self.epoch_day - day0) * 24.0;
        let hr = libm::floor(temp);
        temp = (temp - hr) * 60.0;
        let minute = libm::floor(temp);
        let sec = (temp - minute) * 60.0;
        let (y, m) = (self.epoch_year as f64, mon as f64);
        let mut jd = 367.0 * y - libm::floor(7.0 * (y + libm::floor((m + 9.0) / 12.0)) * 0.25)
            + libm::floor(275.0 * m / 9.0)
            + day as f64
            + 1721013.5;
        let mut frac = (sec + minute * 60.0 + hr * 3600.0) / 86400.0;
        if libm::fabs(frac) > 1.0 {
            let dtt = libm::floor(frac);
            jd += dtt;
            frac -= dtt;
        }
        (jd + frac) - 2433281.5
    }
}

fn month_day(year: i32, day0: u32) -> (u8, u8) {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut d = day0;
    let mut m = 0;
    while m < 11 && d >= lengths[m] {
        d -= lengths[m];
        m += 1;
    }
    (m as u8 + 1, d as u8 + 1)
}
