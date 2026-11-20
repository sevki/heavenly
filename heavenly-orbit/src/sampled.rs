//! Trajectories given as a table of states (tracking data, OEM files,
//! pre-computed propagations).

use alloc::vec::Vec;

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};

/// A trajectory tabulated as position/velocity samples in one frame,
/// interpolated with cubic Hermite polynomials (position and velocity at
/// both ends of each interval).
///
/// This is the interpolation of CCSDS OEM files and of SPK type 13 of
/// degree 3. The position error is bounded by `h⁴/384 · max|r⁗|` for sample
/// spacing `h`, so it falls with the fourth power of the spacing. For a low
/// Earth orbit that is ≈0.4 m at 60 s spacing and ≈0.3 mm at 10 s; the
/// tests measure both against the analytic orbit.
///
/// It fails explicitly:
/// * [`Error::OutsideCoverage`] before the first or after the last sample;
/// * [`Error::DataGap`] inside an interval longer than `max_gap_s`, where
///   interpolation would be a guess.
#[derive(Clone, Debug, PartialEq)]
pub struct SampledTrajectory {
    frame: FrameId,
    samples: Vec<StateVector>,
    max_gap_s: f64,
}

impl SampledTrajectory {
    /// Builds a trajectory. Samples must be in one frame, strictly
    /// increasing in time, finite, and at least two.
    pub fn new(samples: Vec<StateVector>, max_gap_s: f64) -> Result<SampledTrajectory, Error> {
        if samples.len() < 2 {
            return Err(Error::InvalidInput(
                "a trajectory needs at least two samples",
            ));
        }
        if max_gap_s.is_nan() || max_gap_s <= 0.0 {
            return Err(Error::InvalidInput("max_gap_s must be positive"));
        }
        let frame = samples[0].frame;
        for s in &samples {
            if s.frame != frame {
                return Err(Error::FrameMismatch {
                    expected: frame,
                    found: s.frame,
                });
            }
            if !(s.position_m.is_finite() && s.velocity_m_s.is_finite()) {
                return Err(Error::NonFinite);
            }
        }
        if samples.windows(2).any(|w| w[1].epoch <= w[0].epoch) {
            return Err(Error::InvalidInput(
                "samples must be strictly increasing in time",
            ));
        }
        Ok(SampledTrajectory {
            frame,
            samples,
            max_gap_s,
        })
    }

    pub fn frame(&self) -> FrameId {
        self.frame
    }

    pub fn samples(&self) -> &[StateVector] {
        &self.samples
    }

    /// First and last sample epochs.
    pub fn coverage(&self) -> (Epoch, Epoch) {
        (
            self.samples[0].epoch,
            self.samples[self.samples.len() - 1].epoch,
        )
    }

    /// The interpolated state in the trajectory's own frame.
    pub fn state(&self, epoch: Epoch) -> Result<StateVector, Error> {
        let (start, end) = self.coverage();
        if epoch < start || epoch > end {
            return Err(Error::OutsideCoverage { epoch, start, end });
        }
        // Index of the first sample at or after `epoch`.
        let i = self.samples.partition_point(|s| s.epoch < epoch);
        if self.samples[i].epoch == epoch {
            return Ok(self.samples[i]);
        }
        let (a, b) = (&self.samples[i - 1], &self.samples[i]);
        let h = b.epoch.seconds_since(a.epoch);
        if h > self.max_gap_s {
            return Err(Error::DataGap {
                epoch,
                before: a.epoch,
                after: b.epoch,
            });
        }
        let s = epoch.seconds_since(a.epoch) / h;
        let (p, v) = hermite(
            a.position_m,
            a.velocity_m_s,
            b.position_m,
            b.velocity_m_s,
            h,
            s,
        );
        Ok(StateVector::new(self.frame, epoch, p, v))
    }
}

/// Cubic Hermite interpolation on `[0, 1]`, returning position and its time
/// derivative.
fn hermite(p0: Vec3, v0: Vec3, p1: Vec3, v1: Vec3, h: f64, s: f64) -> (Vec3, Vec3) {
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    let p = p0 * h00 + v0 * (h10 * h) + p1 * h01 + v1 * (h11 * h);
    // d/dt = (1/h) d/ds
    let d00 = 6.0 * s2 - 6.0 * s;
    let d10 = 3.0 * s2 - 4.0 * s + 1.0;
    let d01 = -6.0 * s2 + 6.0 * s;
    let d11 = 3.0 * s2 - 2.0 * s;
    let v = p0 * (d00 / h) + v0 * d10 + p1 * (d01 / h) + v1 * d11;
    (p, v)
}

impl Locator for SampledTrajectory {
    type Error = Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Error>
    where
        Error: From<R::Error>,
    {
        Ok(self.state(epoch)?.to_frame(frame, resolver)?)
    }
}
