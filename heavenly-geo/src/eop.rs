//! Earth orientation parameters from IERS data files.
//!
//! [`EopTable`] reads the two standard IERS series and interpolates them:
//!
//! * **finals2000A** (`finals2000A.all`, `.data`, `.daily`), from IERS
//!   Rapid Service/Prediction Centre (USNO). It covers 1973 to the present,
//!   followed by about a year of **predictions**. Where the final
//!   (Bulletin B) values are present they are used; otherwise the rapid
//!   (Bulletin A) ones. `finals.all` has the same layout and is read the
//!   same way.
//! * **EOP 20 C04** (`eopc04.1962-now`), from the IERS Earth Orientation
//!   Centre (Paris Observatory): the definitive combined series, without
//!   predictions. Rows before 1972, where UTC (and so [`Epoch`]) is not
//!   defined, are skipped.
//!
//! # Interpolation
//!
//! Values are tabulated daily at 0h UTC. They are interpolated with
//! four-point Lagrange polynomials, as IERS Gazette #13 recommends (without
//! its sub-0.1 ms tidal corrections). UT1 is interpolated as UT1 − TAI,
//! which is continuous, and converted back to UT1 − UTC at the requested
//! epoch, so a leap second never ends up inside an interpolated difference.
//!
//! Outside the table [`EopTable::get`] returns [`Error::OutsideCoverage`]
//! rather than extrapolating. [`EopTable::first_predicted`] says where the
//! observed values end.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "std")] {
//! use heavenly_core::{Epoch, FrameGraph};
//! use heavenly_geo::{earth, eop::EopTable};
//!
//! let eop = EopTable::open("finals2000A.all")?;
//! let mut frames = FrameGraph::new();
//! earth::register(&mut frames, eop)?;
//! # }
//! # Ok::<(), heavenly_core::Error>(())
//! ```

use alloc::vec::Vec;

use heavenly_core::{Epoch, Error, TimeScale};

use crate::earth::{EarthOrientation, EopProvider};
use crate::mat::ARCSEC;

/// The values IERS tabulates for one day, at 0h UTC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EopRecord {
    /// 0h UTC of the day.
    pub epoch: Epoch,
    /// Pole coordinate x, arcseconds.
    pub xp_arcsec: f64,
    /// Pole coordinate y, arcseconds.
    pub yp_arcsec: f64,
    /// UT1 − UTC, seconds.
    pub ut1_minus_utc_s: f64,
    /// Celestial pole offsets dX, dY with respect to IAU 2000A, in
    /// milliarcseconds, where given.
    ///
    /// They are not applied: the Earth-orientation model here uses a
    /// truncated nutation series, whose own error (up to 15 mas) is far
    /// larger than these corrections (≲ 1 mas).
    pub dx_mas: Option<f64>,
    pub dy_mas: Option<f64>,
    /// Excess length of day, milliseconds, where given.
    pub lod_ms: Option<f64>,
    /// Whether the polar motion or UT1 value is a prediction.
    pub predicted: bool,
}

/// A daily series of Earth orientation parameters, interpolated on demand.
///
/// It implements [`EopProvider`], so it can be passed straight to
/// [`earth::register`](crate::earth::register).
#[derive(Clone, Debug, PartialEq)]
pub struct EopTable {
    records: Vec<EopRecord>,
    /// UT1 − TAI at each record, seconds (continuous across leap seconds).
    ut1_minus_tai: Vec<f64>,
}

impl EopTable {
    /// Builds a table from records in increasing time order.
    pub fn from_records(records: Vec<EopRecord>) -> Result<EopTable, Error> {
        if records.len() < 2 {
            return Err(Error::InvalidInput("an EOP table needs at least two days"));
        }
        let mut ut1_minus_tai = Vec::with_capacity(records.len());
        for (i, r) in records.iter().enumerate() {
            if !(r.xp_arcsec.is_finite()
                && r.yp_arcsec.is_finite()
                && r.ut1_minus_utc_s.is_finite())
            {
                return Err(Error::NonFinite);
            }
            if i > 0 && r.epoch <= records[i - 1].epoch {
                return Err(Error::InvalidInput(
                    "EOP records are not in increasing time order",
                ));
            }
            ut1_minus_tai.push(r.ut1_minus_utc_s - r.epoch.tai_minus_utc_seconds()? as f64);
        }
        Ok(EopTable {
            records,
            ut1_minus_tai,
        })
    }

    /// Parses an IERS `finals2000A` (or `finals`) file.
    ///
    /// Rows without a UT1 − UTC value (the empty rows past the end of the
    /// predictions) are skipped.
    pub fn parse_finals(text: &str) -> Result<EopTable, Error> {
        let mut records = Vec::new();
        for line in text.lines() {
            let l = line.as_bytes();
            if l.len() < 68 || l[57] == b' ' {
                continue;
            }
            let col = |a: usize, b: usize| -> Option<&str> {
                let s = line.get(a - 1..b.min(line.len()))?.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let num = |a: usize, b: usize, what: &'static str| -> Result<Option<f64>, Error> {
                col(a, b)
                    .map(|s| s.parse::<f64>().map_err(|_| Error::Parse(what)))
                    .transpose()
            };
            let req = |a: usize, b: usize, what: &'static str| -> Result<f64, Error> {
                num(a, b, what)?.ok_or(Error::Parse(what))
            };
            let mjd = req(8, 15, "finals: invalid MJD")?;
            let a = (
                req(19, 27, "finals: invalid PM-x")?,
                req(38, 46, "finals: invalid PM-y")?,
                req(59, 68, "finals: invalid UT1-UTC")?,
            );
            let b = (
                num(135, 144, "finals: invalid Bulletin B PM-x")?,
                num(145, 154, "finals: invalid Bulletin B PM-y")?,
                num(155, 165, "finals: invalid Bulletin B UT1-UTC")?,
            );
            let predicted = l.get(16) == Some(&b'P') || l[57] == b'P';
            let (dx_mas, dy_mas) = match (
                num(166, 175, "finals: invalid Bulletin B dX")?,
                num(176, 185, "finals: invalid Bulletin B dY")?,
            ) {
                (Some(x), Some(y)) => (Some(x), Some(y)),
                _ => (
                    num(98, 106, "finals: invalid dX")?,
                    num(117, 125, "finals: invalid dY")?,
                ),
            };
            records.push(EopRecord {
                epoch: epoch_from_mjd(mjd)?,
                xp_arcsec: match b {
                    (Some(x), Some(_), _) => x,
                    _ => a.0,
                },
                yp_arcsec: match b {
                    (Some(_), Some(y), _) => y,
                    _ => a.1,
                },
                ut1_minus_utc_s: b.2.unwrap_or(a.2),
                dx_mas,
                dy_mas,
                lod_ms: num(80, 86, "finals: invalid LOD")?,
                predicted,
            });
        }
        EopTable::from_records(records)
    }

    /// Parses an IERS EOP 20 C04 file (`eopc04.1962-now`).
    pub fn parse_c04(text: &str) -> Result<EopTable, Error> {
        let mut records = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let mut f = [0.0f64; 13];
            let mut n = 0;
            for (slot, word) in f.iter_mut().zip(t.split_ascii_whitespace()) {
                *slot = word
                    .parse()
                    .map_err(|_| Error::Parse("C04: invalid number"))?;
                n += 1;
            }
            if n < 13 {
                return Err(Error::Parse("C04: too few columns"));
            }
            // Columns: year month day hour MJD x y UT1-UTC dX dY xrt yrt LOD.
            if f[0] < 1972.0 {
                continue;
            }
            records.push(EopRecord {
                epoch: epoch_from_mjd(f[4])?,
                xp_arcsec: f[5],
                yp_arcsec: f[6],
                ut1_minus_utc_s: f[7],
                dx_mas: Some(f[8] * 1e3),
                dy_mas: Some(f[9] * 1e3),
                lod_ms: Some(f[12] * 1e3),
                predicted: false,
            });
        }
        EopTable::from_records(records)
    }

    /// Reads a finals2000A or C04 file, recognising the format from its
    /// content.
    #[cfg(feature = "std")]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<EopTable, Error> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| Error::InvalidInput("cannot read the EOP file"))?;
        EopTable::parse(&text)
    }

    /// Parses a finals2000A or C04 file, recognising the format from its
    /// content: C04 files start with `#` comment lines.
    pub fn parse(text: &str) -> Result<EopTable, Error> {
        if text.trim_start().starts_with('#') {
            EopTable::parse_c04(text)
        } else {
            EopTable::parse_finals(text)
        }
    }

    /// The tabulated days.
    pub fn records(&self) -> &[EopRecord] {
        &self.records
    }

    /// The first and last tabulated epochs (0h UTC).
    pub fn range(&self) -> (Epoch, Epoch) {
        (
            self.records[0].epoch,
            self.records[self.records.len() - 1].epoch,
        )
    }

    /// The first day whose values are predictions, if any. Results at or
    /// after it are forecasts.
    pub fn first_predicted(&self) -> Option<Epoch> {
        self.records.iter().find(|r| r.predicted).map(|r| r.epoch)
    }

    /// The interpolated parameters at `epoch`.
    pub fn get(&self, epoch: Epoch) -> Result<EarthOrientation, Error> {
        let (start, end) = self.range();
        if epoch < start || epoch > end {
            return Err(Error::OutsideCoverage { epoch, start, end });
        }
        // Index of the last record at or before `epoch`.
        let i = self.records.partition_point(|r| r.epoch <= epoch) - 1;
        // Four points around the interval [i, i+1], shifted inwards at the ends.
        let n = self.records.len();
        let lo = i.saturating_sub(1).min(n.saturating_sub(4));
        let hi = (lo + 4).min(n);
        let t = |k: usize| self.records[k].epoch.seconds_since(start) / 86_400.0;
        let x = epoch.seconds_since(start) / 86_400.0;
        let lagrange = |value: &dyn Fn(usize) -> f64| -> f64 {
            let mut sum = 0.0;
            for j in lo..hi {
                let mut w = 1.0;
                for m in lo..hi {
                    if m != j {
                        w *= (x - t(m)) / (t(j) - t(m));
                    }
                }
                sum += w * value(j);
            }
            sum
        };
        let ut1_minus_tai = lagrange(&|k| self.ut1_minus_tai[k]);
        Ok(EarthOrientation {
            ut1_minus_utc_s: ut1_minus_tai + epoch.tai_minus_utc_seconds()? as f64,
            xp_rad: lagrange(&|k| self.records[k].xp_arcsec) * ARCSEC,
            yp_rad: lagrange(&|k| self.records[k].yp_arcsec) * ARCSEC,
        })
    }
}

impl EopProvider for EopTable {
    fn earth_orientation(&self, epoch: Epoch) -> Result<EarthOrientation, Error> {
        self.get(epoch)
    }
}

/// 0h UTC of an integral Modified Julian Date.
fn epoch_from_mjd(mjd: f64) -> Result<Epoch, Error> {
    if libm::trunc(mjd) != mjd || !(41_317.0..=200_000.0).contains(&mjd) {
        return Err(Error::Parse("EOP: MJD must be a whole day from 1972 on"));
    }
    // Civil date from the day number (days since 1970-01-01 = MJD − 40587).
    let z = mjd as i64 - 40_587 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    Epoch::from_gregorian(TimeScale::Utc, year, month, day, 0, 0, 0, 0)
}
