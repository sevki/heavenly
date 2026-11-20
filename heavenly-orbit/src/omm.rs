//! CCSDS Orbit Mean-Elements Messages (OMM) carrying SGP4 elements.
//!
//! OMM is the successor to the two-line element set. CelesTrak and
//! Space-Track publish the same SGP4 mean elements as OMM, without the
//! TLE's five-digit catalogue limit. This module reads the three forms they
//! serve:
//!
//! * **KVN** (`KEY = value` lines, CCSDS 502.0-B-3), one or more messages;
//! * **JSON**, an array of flat objects with the same keys
//!   (CelesTrak `FORMAT=json`);
//! * **CSV**, with the keys as a header row (CelesTrak `FORMAT=csv`).
//!
//! An [`Omm`] is propagated with [`Sgp4::from_omm`](crate::Sgp4::from_omm),
//! which follows the reference implementation's own OMM path (the `sgp4`
//! package's `omm.initialize`). The tests check it against that path.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use heavenly_core::{Epoch, Error};

/// One OMM with SGP4 mean elements, as the TLE-compatible subset of the
/// standard defines it. Angles are in degrees, the mean motion in
/// revolutions per day, as in the message.
#[derive(Clone, Debug, PartialEq)]
pub struct Omm {
    pub object_name: String,
    /// International designator, e.g. `1998-067A`.
    pub object_id: String,
    /// The epoch (`EPOCH`, UTC).
    pub epoch: Epoch,
    epoch_fields: EpochFields,
    pub mean_motion_rev_day: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_pericenter_deg: f64,
    pub mean_anomaly_deg: f64,
    pub ephemeris_type: u8,
    pub classification: char,
    pub norad_cat_id: u32,
    pub element_set_no: u32,
    pub rev_at_epoch: u32,
    pub bstar: f64,
    /// Rev/day².
    pub mean_motion_dot: f64,
    /// Rev/day³.
    pub mean_motion_ddot: f64,
}

/// The fields of one message, by key.
type Fields<'a> = Vec<(&'a str, String)>;

/// An epoch's UTC calendar fields: year, month, day, hour, minute, and
/// microseconds of the minute, as written.
type EpochFields = (i32, u8, u8, u8, u8, u64);

fn get<'a>(fields: &'a Fields, key: &'static str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn require<'a>(fields: &'a Fields, key: &'static str) -> Result<&'a str, Error> {
    get(fields, key).ok_or(Error::Parse(key_missing(key)))
}

/// A static message naming the missing key.
fn key_missing(key: &'static str) -> &'static str {
    match key {
        "EPOCH" => "OMM: EPOCH is missing",
        "MEAN_MOTION" => "OMM: MEAN_MOTION is missing",
        "ECCENTRICITY" => "OMM: ECCENTRICITY is missing",
        "INCLINATION" => "OMM: INCLINATION is missing",
        "RA_OF_ASC_NODE" => "OMM: RA_OF_ASC_NODE is missing",
        "ARG_OF_PERICENTER" => "OMM: ARG_OF_PERICENTER is missing",
        "MEAN_ANOMALY" => "OMM: MEAN_ANOMALY is missing",
        "NORAD_CAT_ID" => "OMM: NORAD_CAT_ID is missing",
        "BSTAR" => "OMM: BSTAR is missing",
        _ => "OMM: a required key is missing",
    }
}

fn number(fields: &Fields, key: &'static str) -> Result<f64, Error> {
    require(fields, key)?
        .trim()
        .parse::<f64>()
        .map_err(|_| Error::Parse("OMM: a numeric field is not a number"))
}

fn optional_number(fields: &Fields, key: &'static str) -> Result<f64, Error> {
    match get(fields, key) {
        None => Ok(0.0),
        Some(v) if v.trim().is_empty() => Ok(0.0),
        Some(v) => v
            .trim()
            .parse()
            .map_err(|_| Error::Parse("OMM: a numeric field is not a number")),
    }
}

fn optional_uint(fields: &Fields, key: &'static str) -> Result<u32, Error> {
    optional_number(fields, key).and_then(|v| {
        if v >= 0.0 && v <= u32::MAX as f64 && libm::trunc(v) == v {
            Ok(v as u32)
        } else {
            Err(Error::Parse(
                "OMM: an integer field is not a non-negative integer",
            ))
        }
    })
}

/// `YYYY-MM-DDTHH:MM:SS[.ffffff]`, UTC (CCSDS also allows a trailing `Z`).
fn parse_epoch(s: &str) -> Result<(Epoch, EpochFields), Error> {
    let s = s.trim().trim_end_matches('Z');
    let bad = Error::Parse("OMM: EPOCH must be YYYY-MM-DDTHH:MM:SS[.ffffff]");
    let b = s.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || !(b[10] == b'T' || b[10] == b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return Err(bad);
    }
    let int = |r: core::ops::Range<usize>| {
        s.get(r)
            .and_then(|t| t.parse::<u32>().ok())
            .ok_or(bad.clone())
    };
    let (y, mo, d, h, mi, sec) = (
        int(0..4)?,
        int(5..7)?,
        int(8..10)?,
        int(11..13)?,
        int(14..16)?,
        int(17..19)?,
    );
    let frac = &s[19..];
    let (ns, us) = if frac.is_empty() {
        (0u32, 0u64)
    } else {
        let digits = frac.strip_prefix('.').ok_or(bad.clone())?;
        if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad);
        }
        let mut ns: u32 = digits.parse().map_err(|_| bad.clone())?;
        for _ in digits.len()..9 {
            ns *= 10;
        }
        (ns, (ns / 1000) as u64)
    };
    let epoch = Epoch::from_gregorian(
        heavenly_core::TimeScale::Utc,
        y as i32,
        mo as u8,
        d as u8,
        h as u8,
        mi as u8,
        sec as u8,
        ns,
    )?;
    Ok((
        epoch,
        (
            y as i32,
            mo as u8,
            d as u8,
            h as u8,
            mi as u8,
            sec as u64 * 1_000_000 + us,
        ),
    ))
}

impl Omm {
    fn from_fields(fields: &Fields) -> Result<Omm, Error> {
        // Only SGP4 mean elements of Earth satellites in TEME make sense
        // here; reject anything that says otherwise.
        let check = |key: &'static str, ok: &[&str], msg: &'static str| match get(fields, key) {
            Some(v) if !ok.iter().any(|o| v.trim().eq_ignore_ascii_case(o)) => {
                Err(Error::Parse(msg))
            }
            _ => Ok(()),
        };
        check(
            "MEAN_ELEMENT_THEORY",
            &["SGP4", "SGP/SGP4"],
            "OMM: MEAN_ELEMENT_THEORY is not SGP4",
        )?;
        check("REF_FRAME", &["TEME"], "OMM: REF_FRAME is not TEME")?;
        check("CENTER_NAME", &["EARTH"], "OMM: CENTER_NAME is not EARTH")?;
        check("TIME_SYSTEM", &["UTC"], "OMM: TIME_SYSTEM is not UTC")?;
        let (epoch, epoch_fields) = parse_epoch(require(fields, "EPOCH")?)?;
        let ephemeris_type = optional_uint(fields, "EPHEMERIS_TYPE")?;
        Ok(Omm {
            object_name: get(fields, "OBJECT_NAME").unwrap_or("").trim().to_string(),
            object_id: get(fields, "OBJECT_ID").unwrap_or("").trim().to_string(),
            epoch,
            epoch_fields,
            mean_motion_rev_day: number(fields, "MEAN_MOTION")?,
            eccentricity: number(fields, "ECCENTRICITY")?,
            inclination_deg: number(fields, "INCLINATION")?,
            raan_deg: number(fields, "RA_OF_ASC_NODE")?,
            arg_pericenter_deg: number(fields, "ARG_OF_PERICENTER")?,
            mean_anomaly_deg: number(fields, "MEAN_ANOMALY")?,
            ephemeris_type: u8::try_from(ephemeris_type)
                .map_err(|_| Error::Parse("OMM: EPHEMERIS_TYPE is out of range"))?,
            classification: get(fields, "CLASSIFICATION_TYPE")
                .and_then(|c| c.trim().chars().next())
                .unwrap_or('U'),
            norad_cat_id: {
                let v = number(fields, "NORAD_CAT_ID")?;
                if !(v >= 0.0 && v <= u32::MAX as f64 && libm::trunc(v) == v) {
                    return Err(Error::Parse("OMM: NORAD_CAT_ID is not a catalogue number"));
                }
                v as u32
            },
            element_set_no: optional_uint(fields, "ELEMENT_SET_NO")?,
            rev_at_epoch: optional_uint(fields, "REV_AT_EPOCH")?,
            bstar: number(fields, "BSTAR")?,
            mean_motion_dot: optional_number(fields, "MEAN_MOTION_DOT")?,
            mean_motion_ddot: optional_number(fields, "MEAN_MOTION_DDOT")?,
        })
    }

    /// The epoch as SGP4 counts it, computed as the reference
    /// implementation's OMM reader does: microseconds since 1949-12-31
    /// 00:00, divided into seconds and then days.
    pub(crate) fn days_since_1950(&self) -> f64 {
        let (y, m, d, h, mi, us) = self.epoch_fields;
        let days = days_from_civil(y, m, d) - days_from_civil(1949, 12, 31);
        let total_us = (days * 86_400 + h as i64 * 3_600 + mi as i64 * 60) as u64 * 1_000_000 + us;
        total_us as f64 / 1e6 / 86_400.0
    }

    /// Parses KVN text holding one or more messages. Each message starts at
    /// `CCSDS_OMM_VERS` (or, without headers, at `OBJECT_NAME`); comments
    /// (`COMMENT …`) and blank lines are skipped.
    pub fn parse_kvn(text: &str) -> Result<Vec<Omm>, Error> {
        let mut out = Vec::new();
        let mut fields: Fields = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("COMMENT") {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or(Error::Parse("OMM KVN: expected KEY = value"))?;
            let (k, v) = (k.trim(), v.trim());
            let starts_new = k.eq_ignore_ascii_case("CCSDS_OMM_VERS")
                || (k.eq_ignore_ascii_case("OBJECT_NAME") && get(&fields, "EPOCH").is_some());
            if starts_new && get(&fields, "EPOCH").is_some() {
                out.push(Omm::from_fields(&fields)?);
                fields.clear();
            }
            // Values may carry units in brackets: `INCLINATION = 51.6 [deg]`.
            let v = v.split_once('[').map_or(v, |(a, _)| a.trim());
            fields.push((k, v.to_string()));
        }
        if !fields.is_empty() {
            out.push(Omm::from_fields(&fields)?);
        }
        if out.is_empty() {
            return Err(Error::Parse("OMM KVN: no messages"));
        }
        Ok(out)
    }

    /// Parses a JSON array of OMM objects (or a single object), as
    /// CelesTrak serves with `FORMAT=json`.
    pub fn parse_json(text: &str) -> Result<Vec<Omm>, Error> {
        let mut p = Json {
            s: text.as_bytes(),
            i: 0,
        };
        p.ws();
        let objects = if p.peek() == Some(b'[') {
            p.array()?
        } else {
            alloc::vec![p.object()?]
        };
        p.ws();
        if p.i != p.s.len() {
            return Err(Error::Parse("OMM JSON: trailing characters"));
        }
        let mut out = Vec::with_capacity(objects.len());
        for o in &objects {
            let fields: Fields = o.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            out.push(Omm::from_fields(&fields)?);
        }
        Ok(out)
    }

    /// Parses CSV with the OMM keys as its header row, as CelesTrak serves
    /// with `FORMAT=csv`. Fields may be quoted.
    pub fn parse_csv(text: &str) -> Result<Vec<Omm>, Error> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header = csv_row(lines.next().ok_or(Error::Parse("OMM CSV: empty"))?)?;
        let mut out = Vec::new();
        for line in lines {
            let row = csv_row(line)?;
            if row.len() != header.len() {
                return Err(Error::Parse(
                    "OMM CSV: a row has the wrong number of fields",
                ));
            }
            let fields: Fields = header.iter().map(String::as_str).zip(row).collect();
            out.push(Omm::from_fields(&fields)?);
        }
        Ok(out)
    }
}

fn csv_row(line: &str) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(core::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    if quoted {
        return Err(Error::Parse("OMM CSV: unterminated quote"));
    }
    out.push(cur);
    Ok(out)
}

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = y as i64 - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A minimal JSON reader for arrays of flat objects whose values are
/// strings, numbers, booleans or null. Numbers are kept as their text, so
/// they parse to exactly the same `f64` as the original.
struct Json<'a> {
    s: &'a [u8],
    i: usize,
}

impl Json<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), Error> {
        self.ws();
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(Error::Parse("OMM JSON: unexpected character"))
        }
    }

    fn array(&mut self) -> Result<Vec<Vec<(String, String)>>, Error> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.object()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return Err(Error::Parse("OMM JSON: expected , or ]")),
            }
        }
    }

    fn object(&mut self) -> Result<Vec<(String, String)>, Error> {
        self.expect(b'{')?;
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(out);
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.expect(b':')?;
            self.ws();
            let v = match self.peek() {
                Some(b'"') => self.string()?,
                Some(b'{' | b'[') => {
                    return Err(Error::Parse("OMM JSON: nested values are not OMM fields"))
                }
                _ => self.bare()?,
            };
            // `null` means the field is absent.
            if v != "null" {
                out.push((k, v));
            }
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return Err(Error::Parse("OMM JSON: expected , or }")),
            }
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        let bad = Error::Parse("OMM JSON: invalid string");
        if self.peek() != Some(b'"') {
            return Err(bad);
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while !matches!(self.peek(), Some(b'"' | b'\\') | None) {
                self.i += 1;
            }
            out.push_str(core::str::from_utf8(&self.s[start..self.i]).map_err(|_| bad.clone())?);
            match self.peek() {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    let c = *self.s.get(self.i + 1).ok_or(bad.clone())?;
                    self.i += 2;
                    match c {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self.s.get(self.i..self.i + 4).ok_or(bad.clone())?;
                            let code = u32::from_str_radix(
                                core::str::from_utf8(hex).map_err(|_| bad.clone())?,
                                16,
                            )
                            .map_err(|_| bad.clone())?;
                            self.i += 4;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(bad),
                    }
                }
                _ => return Err(bad),
            }
        }
    }

    /// A number, `true`, `false` or `null`, as text.
    fn bare(&mut self) -> Result<String, Error> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.'))
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(Error::Parse("OMM JSON: expected a value"));
        }
        Ok(core::str::from_utf8(&self.s[start..self.i])
            .expect("ASCII")
            .to_string())
    }
}
