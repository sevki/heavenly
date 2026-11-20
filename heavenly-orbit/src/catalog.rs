//! Many satellites at once: a catalogue of TLEs or OMMs, propagated
//! together.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, StateVector, Vec3};

use crate::{Gravity, Omm, OpsMode, Sgp4, Tle};

/// One object in a [`Catalog`].
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    /// The name line of a three-line set, `OBJECT_NAME` of an OMM, or empty.
    pub name: String,
    pub norad_cat_id: u32,
    pub sgp4: Sgp4,
}

/// An element set that could not be loaded, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Rejected {
    /// The 1-based line of a TLE's first line, or the 0-based index of an
    /// OMM.
    pub at: usize,
    pub error: Error,
}

/// A set of satellites propagated with SGP4, for working with many at once.
///
/// [`Catalog::states_at`] propagates every object to one epoch (each in
/// TEME) and moves them all into the requested frame with one transform,
/// using [`StateVector::to_frame_batch`], which is vectorised with the
/// `simd` feature.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Loads two- or three-line element sets: any line starting with `1 `
    /// followed by one starting with `2 `, optionally preceded by a name
    /// line (CelesTrak's `FORMAT=tle`). Checksums are verified. Sets that
    /// fail to parse or to initialise are returned as [`Rejected`] rather
    /// than failing the whole catalogue.
    pub fn from_tle_text(
        text: &str,
        gravity: Gravity,
        opsmode: OpsMode,
    ) -> (Catalog, Vec<Rejected>) {
        let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
        let (mut cat, mut rejected) = (Catalog::default(), Vec::new());
        let mut k = 0;
        while k < lines.len() {
            let l = lines[k];
            if !(l.starts_with("1 ") && lines.get(k + 1).is_some_and(|n| n.starts_with("2 "))) {
                k += 1;
                continue;
            }
            let name = match k.checked_sub(1).map(|p| lines[p]) {
                Some(p) if !p.is_empty() && !p.starts_with("1 ") && !p.starts_with("2 ") => {
                    p.trim_start_matches("0 ").trim().to_string()
                }
                _ => String::new(),
            };
            let loaded = Tle::parse(l, lines[k + 1]).and_then(|tle| {
                Ok(CatalogEntry {
                    name,
                    norad_cat_id: tle.catalog_number,
                    sgp4: Sgp4::new(&tle, gravity, opsmode)?,
                })
            });
            match loaded {
                Ok(e) => cat.entries.push(e),
                Err(error) => rejected.push(Rejected { at: k + 1, error }),
            }
            k += 2;
        }
        (cat, rejected)
    }

    /// Loads OMMs (see [`Omm::parse_kvn`], [`Omm::parse_json`],
    /// [`Omm::parse_csv`]). Messages that fail to initialise are returned as
    /// [`Rejected`].
    pub fn from_omms(omms: &[Omm], gravity: Gravity, opsmode: OpsMode) -> (Catalog, Vec<Rejected>) {
        let (mut cat, mut rejected) = (Catalog::default(), Vec::new());
        for (at, o) in omms.iter().enumerate() {
            match Sgp4::from_omm(o, gravity, opsmode) {
                Ok(sgp4) => cat.entries.push(CatalogEntry {
                    name: o.object_name.clone(),
                    norad_cat_id: o.norad_cat_id,
                    sgp4,
                }),
                Err(error) => rejected.push(Rejected { at, error }),
            }
        }
        (cat, rejected)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every object's state at `epoch` in `frame`, in catalogue order;
    /// `None` where SGP4 fails (typically a decayed object).
    pub fn states_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<Vec<Option<StateVector>>, R::Error> {
        let mut index = Vec::with_capacity(self.entries.len());
        let mut states = Vec::with_capacity(self.entries.len());
        for (k, e) in self.entries.iter().enumerate() {
            if let Ok(s) = e.sgp4.state(epoch) {
                index.push(k);
                states.push(s);
            }
        }
        StateVector::to_frame_batch(&mut states, frame, resolver)?;
        let mut out = alloc::vec![None; self.entries.len()];
        for (k, s) in index.into_iter().zip(states) {
            out[k] = Some(s);
        }
        Ok(out)
    }

    /// Every object's position (metres) at `epoch` in `frame`.
    pub fn positions_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<Vec<Option<Vec3>>, R::Error> {
        Ok(self
            .states_at(epoch, frame, resolver)?
            .into_iter()
            .map(|s| s.map(|s| s.position_m))
            .collect())
    }

    /// One object's position (metres) at `epoch` in `frame`, or `None` where
    /// SGP4 fails.
    pub fn position_of<R: FrameResolver + ?Sized>(
        &self,
        index: usize,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<Option<Vec3>, R::Error> {
        let Some(e) = self.entries.get(index) else {
            return Err(Error::InvalidInput("catalogue index out of range").into());
        };
        match e.sgp4.state(epoch) {
            Ok(s) => Ok(Some(s.to_frame(frame, resolver)?.position_m)),
            Err(_) => Ok(None),
        }
    }
}
