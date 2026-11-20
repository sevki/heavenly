//! Binary PCK (body orientation) kernels, e.g. `moon_pa_de440_200625.bpc`.

use alloc::vec::Vec;

use heavenly_core::{Epoch, Rotation, Transform, Vec3};

use crate::daf::{evaluate_chebyshev, ChebyshevSegment, Daf};
use crate::rot;
use crate::spk::J2000;
use crate::SpiceError;

/// Metadata of one binary PCK segment.
#[derive(Clone, Debug, PartialEq)]
pub struct PckSegment {
    /// NAIF frame class ID of the body-fixed frame (e.g. 31008 for
    /// `MOON_PA_DE440`).
    pub frame_class_id: i32,
    /// NAIF frame code of the reference (inertial) frame.
    pub reference_frame: i32,
    /// PCK data type (2 is evaluated).
    pub data_type: i32,
    pub start_et: f64,
    pub end_et: f64,
    start: usize,
    end: usize,
}

/// A loaded binary PCK file.
#[derive(Debug)]
pub struct Pck {
    daf: Daf,
    segments: Vec<PckSegment>,
}

impl Pck {
    /// Parses a binary PCK file from its bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Pck, SpiceError> {
        let daf = Daf::parse(bytes, "DAF/PCK")?;
        let mut segments = Vec::with_capacity(daf.summaries.len());
        for s in &daf.summaries {
            if s.doubles.len() != 2 || s.ints.len() != 5 {
                return Err(SpiceError::Format("PCK summaries must have ND=2, NI=5"));
            }
            if s.ints[3] <= 0 || s.ints[4] < s.ints[3] {
                return Err(SpiceError::Format("bad segment addresses"));
            }
            segments.push(PckSegment {
                frame_class_id: s.ints[0],
                reference_frame: s.ints[1],
                data_type: s.ints[2],
                start_et: s.doubles[0],
                end_et: s.doubles[1],
                start: s.ints[3] as usize,
                end: s.ints[4] as usize,
            });
        }
        Ok(Pck { daf, segments })
    }

    /// Reads and parses a binary PCK file.
    #[cfg(feature = "std")]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Pck, SpiceError> {
        Pck::from_bytes(std::fs::read(path)?)
    }

    pub fn segments(&self) -> &[PckSegment] {
        &self.segments
    }

    /// Rotation matrix from ICRF (J2000) axes to the body-fixed axes:
    /// `p_body = M · p_icrf`.
    pub fn icrf_to_body_matrix(
        &self,
        frame_class_id: i32,
        epoch: Epoch,
    ) -> Result<[[f64; 3]; 3], SpiceError> {
        let (angles, _) = self.euler(frame_class_id, epoch.tdb_seconds_since_j2000())?;
        Ok(matrix(angles))
    }

    /// The kinematic transform from the body-fixed frame to the ICRF-aligned
    /// frame at the same origin.
    pub fn body_fixed_to_icrf(
        &self,
        frame_class_id: i32,
        epoch: Epoch,
    ) -> Result<Transform, SpiceError> {
        self.body_fixed_to_icrf_at_et(frame_class_id, epoch.tdb_seconds_since_j2000())
    }

    /// Like [`Pck::body_fixed_to_icrf`], at TDB seconds past J2000 (ET).
    pub fn body_fixed_to_icrf_at_et(
        &self,
        frame_class_id: i32,
        et: f64,
    ) -> Result<Transform, SpiceError> {
        let ([phi, delta, w], [phi_dot, delta_dot, w_dot]) = self.euler(frame_class_id, et)?;
        let m = matrix([phi, delta, w]);
        let rotation = Rotation::from_matrix(rot::transpose(&m))
            .ok_or(SpiceError::Format("non-finite angles"))?;
        // ω = φ̇ ẑ + δ̇ n̂ + ẇ p̂ for M = [w]₃ [δ]₁ [φ]₃.
        let (sp, cp) = (libm::sin(phi), libm::cos(phi));
        let (sd, cd) = (libm::sin(delta), libm::cos(delta));
        let node = Vec3::new(cp, sp, 0.0);
        let pole = Vec3::new(sd * sp, -sd * cp, cd);
        let omega = Vec3::Z * phi_dot + node * delta_dot + pole * w_dot;
        Ok(Transform::new(rotation, Vec3::ZERO, Vec3::ZERO, omega))
    }

    /// Euler angles `(φ, δ, w)` in radians and their rates in rad/s.
    fn euler(&self, frame_class_id: i32, et: f64) -> Result<([f64; 3], [f64; 3]), SpiceError> {
        let seg = self
            .segments
            .iter()
            .rev()
            .find(|s| s.frame_class_id == frame_class_id && s.start_et <= et && et <= s.end_et)
            .ok_or_else(|| SpiceError::NoCoverage {
                id: frame_class_id,
                epoch: Epoch::from_tdb_seconds_since_j2000(et).unwrap_or(Epoch::J2000),
            })?;
        if seg.reference_frame != J2000 {
            return Err(SpiceError::UnsupportedFrame {
                id: frame_class_id,
                frame: seg.reference_frame,
            });
        }
        if seg.data_type != 2 {
            return Err(SpiceError::UnsupportedType {
                id: frame_class_id,
                data_type: seg.data_type,
            });
        }
        evaluate_chebyshev::<3>(
            &self.daf,
            ChebyshevSegment {
                start: seg.start,
                end: seg.end,
            },
            et,
        )
    }
}

fn matrix([phi, delta, w]: [f64; 3]) -> [[f64; 3]; 3] {
    rot::mul(
        &rot::axis(3, w),
        &rot::mul(&rot::axis(1, delta), &rot::axis(3, phi)),
    )
}
