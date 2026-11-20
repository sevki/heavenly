//! SPK (ephemeris) kernels.

use alloc::vec::Vec;

use heavenly_core::{Epoch, Vec3};

use crate::daf::{evaluate_chebyshev, ChebyshevSegment, Daf};
use crate::SpiceError;

/// NAIF frame code of J2000. DE ephemerides labelled J2000 are aligned with
/// the ICRF (the J2000/ICRF frame bias, ~20 mas, applies only to
/// non-DE data and is not modelled by SPICE for them either).
pub const J2000: i32 = 1;

/// Metadata of one SPK segment.
#[derive(Clone, Debug, PartialEq)]
pub struct SpkSegment {
    /// NAIF ID of the body whose motion the segment describes.
    pub target: i32,
    /// NAIF ID of the body the motion is relative to.
    pub center: i32,
    /// NAIF frame code of the segment's axes.
    pub frame: i32,
    /// SPK data type (2 and 3 are evaluated).
    pub data_type: i32,
    /// Coverage in TDB seconds past J2000 (SPICE "ET").
    pub start_et: f64,
    pub end_et: f64,
    start: usize,
    end: usize,
}

/// A loaded SPK file.
#[derive(Debug)]
pub struct Spk {
    daf: Daf,
    segments: Vec<SpkSegment>,
}

impl Spk {
    /// Parses an SPK file from its bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Spk, SpiceError> {
        let daf = Daf::parse(bytes, "DAF/SPK")?;
        let mut segments = Vec::with_capacity(daf.summaries.len());
        for s in &daf.summaries {
            if s.doubles.len() != 2 || s.ints.len() != 6 {
                return Err(SpiceError::Format("SPK summaries must have ND=2, NI=6"));
            }
            if s.ints[4] <= 0 || s.ints[5] < s.ints[4] {
                return Err(SpiceError::Format("bad segment addresses"));
            }
            segments.push(SpkSegment {
                target: s.ints[0],
                center: s.ints[1],
                frame: s.ints[2],
                data_type: s.ints[3],
                start_et: s.doubles[0],
                end_et: s.doubles[1],
                start: s.ints[4] as usize,
                end: s.ints[5] as usize,
            });
        }
        Ok(Spk { daf, segments })
    }

    /// Reads and parses an SPK file.
    #[cfg(feature = "std")]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Spk, SpiceError> {
        Spk::from_bytes(std::fs::read(path)?)
    }

    /// The segments in file order.
    pub fn segments(&self) -> &[SpkSegment] {
        &self.segments
    }

    /// Position (m) and velocity (m/s) of `target` relative to `observer`,
    /// in ICRF axes, at `epoch`. Geometric: no light-time or aberration.
    pub fn state(
        &self,
        target: i32,
        observer: i32,
        epoch: Epoch,
    ) -> Result<(Vec3, Vec3), SpiceError> {
        self.state_at_et(target, observer, epoch.tdb_seconds_since_j2000())
    }

    /// Like [`Spk::state`], at an ephemeris time given directly as TDB
    /// seconds past J2000 (SPICE "ET").
    ///
    /// Segment selection is discontinuous at segment boundaries, so code
    /// that must match SPICE exactly there should pass ET rather than an
    /// [`Epoch`], whose nanosecond rounding can move an instant across one.
    pub fn state_at_et(
        &self,
        target: i32,
        observer: i32,
        et: f64,
    ) -> Result<(Vec3, Vec3), SpiceError> {
        let epoch = Epoch::from_tdb_seconds_since_j2000(et)?;
        let t = self.chain(target, et, epoch)?;
        let o = self.chain(observer, et, epoch)?;
        // Find the first common centre.
        let common = t
            .iter()
            .map(|l| l.0)
            .find(|b| o.iter().any(|l| l.0 == *b))
            .ok_or(SpiceError::NoPath { target, observer })?;
        let sum = |links: &[(i32, Vec3, Vec3)]| {
            let mut p = Vec3::ZERO;
            let mut v = Vec3::ZERO;
            for (body, lp, lv) in links {
                if *body == common {
                    break;
                }
                p += *lp;
                v += *lv;
            }
            (p, v)
        };
        let (tp, tv) = sum(&t);
        let (op, ov) = sum(&o);
        Ok(((tp - op) * 1e3, (tv - ov) * 1e3))
    }

    /// Walks from `body` up through segment centres. Each entry is
    /// `(body, state of body relative to its centre in km, km/s)`; the last
    /// entry is the root, with a zero state.
    fn chain(
        &self,
        body: i32,
        et: f64,
        epoch: Epoch,
    ) -> Result<Vec<(i32, Vec3, Vec3)>, SpiceError> {
        let mut links = Vec::new();
        let mut current = body;
        loop {
            match self.segment_for(current, et) {
                None => {
                    // A body is a root (e.g. the Solar System barycentre) only
                    // if no segment names it as a target; otherwise the
                    // kernel simply does not cover this epoch.
                    if self.segments.iter().any(|s| s.target == current) {
                        return Err(SpiceError::NoCoverage { id: current, epoch });
                    }
                    links.push((current, Vec3::ZERO, Vec3::ZERO));
                    return Ok(links);
                }
                Some(seg) => {
                    if seg.frame != J2000 {
                        return Err(SpiceError::UnsupportedFrame {
                            id: current,
                            frame: seg.frame,
                        });
                    }
                    let (p, v) = self.evaluate(seg, et)?;
                    links.push((current, p, v));
                    current = seg.center;
                    if links.len() > 64 {
                        return Err(SpiceError::Format("segment centre chain loops"));
                    }
                }
            }
        }
    }

    /// SPICE precedence: the last segment in the file that covers `et`.
    fn segment_for(&self, target: i32, et: f64) -> Option<&SpkSegment> {
        self.segments
            .iter()
            .rev()
            .find(|s| s.target == target && s.start_et <= et && et <= s.end_et)
    }

    fn evaluate(&self, seg: &SpkSegment, et: f64) -> Result<(Vec3, Vec3), SpiceError> {
        let c = ChebyshevSegment {
            start: seg.start,
            end: seg.end,
        };
        match seg.data_type {
            2 => {
                let (p, v) = evaluate_chebyshev::<3>(&self.daf, c, et)?;
                Ok((Vec3::from_array(p), Vec3::from_array(v)))
            }
            3 => {
                let (s, _) = evaluate_chebyshev::<6>(&self.daf, c, et)?;
                Ok((Vec3::new(s[0], s[1], s[2]), Vec3::new(s[3], s[4], s[5])))
            }
            t => Err(SpiceError::UnsupportedType {
                id: seg.target,
                data_type: t,
            }),
        }
    }
}
