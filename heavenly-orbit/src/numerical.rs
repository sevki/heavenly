//! Numerical propagation: integrating the equations of motion under a
//! [`ForceModel`], for when the two-body and SGP4 models are not enough.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};

use crate::ForceModel;

/// Error tolerances for the adaptive step. Each step's estimated error in
/// every position (m) and velocity (m/s) component must be below
/// `absolute + relative × |component|`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    pub relative: f64,
    pub absolute: f64,
}

impl Default for Tolerance {
    /// 1e-13 relative and 1e-10 absolute: for a low orbit, about 1 mm of
    /// integration error per day. Each factor of ten looser makes the error
    /// ten times larger and the integration about 1.6 times faster
    /// (`cargo run --release -p heavenly-orbit --example tolerance`).
    fn default() -> Self {
        Tolerance {
            relative: 1e-13,
            absolute: 1e-10,
        }
    }
}

/// An orbit integrated numerically from an initial state under a
/// [`ForceModel`], with an adaptive Dormand–Prince 5(4) integrator.
///
/// The state is integrated in [`FrameId::GCRF`]. Each call integrates from
/// the initial state to the requested epoch, forwards or backwards, landing
/// on it exactly. To look up many epochs, use
/// [`for_each`](Self::for_each), which integrates once through them, or
/// [`sample`](Self::sample), which tabulates the orbit as a
/// [`SampledTrajectory`](crate::SampledTrajectory) to interpolate.
///
/// ```
/// use heavenly_core::*;
/// use heavenly_orbit::{gm, ConicElements, ForceModel, NumericalPropagator};
///
/// # fn main() -> Result<(), Error> {
/// let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
/// let elements = ConicElements {
///     periapsis_m: 6_378_137.0 + 700e3,
///     eccentricity: 0.001,
///     inclination_rad: 98.2f64.to_radians(),
///     ascending_node_rad: 0.0,
///     argument_of_periapsis_rad: 0.0,
///     mean_anomaly_rad: 0.0,
///     epoch,
///     gm_m3_s2: gm::EARTH,
/// };
/// let (r, v) = elements.state_at(epoch)?;
/// let orbit = NumericalPropagator::new(
///     StateVector::new(FrameId::GCRF, epoch, r, v),
///     ForceModel::earth(),
/// )?;
/// // J2 turns this sun-synchronous orbit's plane about 1° per day, east.
/// let later = orbit.propagate(epoch.add_seconds(86_400.0)?)?;
/// let h = later.position_m.cross(later.velocity_m_s);
/// let node_deg = h.x.atan2(-h.y).to_degrees();
/// assert!((node_deg - 0.99).abs() < 0.05, "{node_deg}");
/// # Ok(()) }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct NumericalPropagator {
    initial: StateVector,
    pub forces: ForceModel,
    pub tolerance: Tolerance,
    /// The most steps one call may take before it fails, so that a bad
    /// force model or a re-entry cannot loop for ever.
    pub max_steps: usize,
}

type Y = [f64; 6];

// Dormand–Prince 5(4) (Dormand & Prince 1980): nodes, coupling
// coefficients, 5th-order weights, and the difference between the 5th- and
// 4th-order weights, which estimates the error.
const C: [f64; 7] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0, 1.0];
const A: [[f64; 6]; 7] = [
    [0.0; 6],
    [1.0 / 5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0, 0.0],
    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0, 0.0],
    [
        19_372.0 / 6_561.0,
        -25_360.0 / 2_187.0,
        64_448.0 / 6_561.0,
        -212.0 / 729.0,
        0.0,
        0.0,
    ],
    [
        9_017.0 / 3_168.0,
        -355.0 / 33.0,
        46_732.0 / 5_247.0,
        49.0 / 176.0,
        -5_103.0 / 18_656.0,
        0.0,
    ],
    [
        35.0 / 384.0,
        0.0,
        500.0 / 1_113.0,
        125.0 / 192.0,
        -2_187.0 / 6_784.0,
        11.0 / 84.0,
    ],
];
const E: [f64; 7] = [
    71.0 / 57_600.0,
    0.0,
    -71.0 / 16_695.0,
    71.0 / 1_920.0,
    -17_253.0 / 339_200.0,
    22.0 / 525.0,
    -1.0 / 40.0,
];

fn split(y: &Y) -> (Vec3, Vec3) {
    (Vec3::new(y[0], y[1], y[2]), Vec3::new(y[3], y[4], y[5]))
}

impl NumericalPropagator {
    /// An orbit starting from `initial`, which must be in
    /// [`FrameId::GCRF`] (convert it with `StateVector::to_frame`).
    pub fn new(initial: StateVector, forces: ForceModel) -> Result<NumericalPropagator, Error> {
        if initial.frame != FrameId::GCRF {
            return Err(Error::FrameMismatch {
                expected: FrameId::GCRF,
                found: initial.frame,
            });
        }
        if !(initial.position_m.is_finite() && initial.velocity_m_s.is_finite()) {
            return Err(Error::NonFinite);
        }
        forces.acceleration(initial.epoch, initial.position_m, initial.velocity_m_s)?;
        Ok(NumericalPropagator {
            initial,
            forces,
            tolerance: Tolerance::default(),
            max_steps: 1_000_000,
        })
    }

    /// The initial state.
    pub fn initial(&self) -> StateVector {
        self.initial
    }

    /// The state at `epoch`, in GCRF.
    pub fn propagate(&self, epoch: Epoch) -> Result<StateVector, Error> {
        let mut out = None;
        self.for_each([epoch], |s| out = Some(s))?;
        out.ok_or(Error::InvalidInput("no epoch"))
    }

    /// Calls `f` with the state at each of `epochs`, in order, integrating
    /// once through them. Epochs on the same side of the initial epoch
    /// should be sorted away from it (ascending after it, descending before
    /// it); otherwise each change of direction restarts from the initial
    /// state, which is correct but slower.
    pub fn for_each(
        &self,
        epochs: impl IntoIterator<Item = Epoch>,
        mut f: impl FnMut(StateVector),
    ) -> Result<(), Error> {
        let (r0, v0) = (self.initial.position_m, self.initial.velocity_m_s);
        let start: Y = [r0.x, r0.y, r0.z, v0.x, v0.y, v0.z];
        let (mut t, mut y, mut h) = (0.0, start, 0.0);
        let mut steps = 0;
        for epoch in epochs {
            let target = epoch.seconds_since(self.initial.epoch);
            if !target.is_finite() {
                return Err(Error::NonFinite);
            }
            // Restart from the initial state when the target is on the
            // other side of it, or behind us.
            if (target - t) * t < 0.0 || target.abs() < t.abs() {
                (t, y, h) = (0.0, start, 0.0);
            }
            self.integrate(&mut t, &mut y, &mut h, target, &mut steps)?;
            let (r, v) = split(&y);
            f(StateVector::new(FrameId::GCRF, epoch, r, v));
        }
        Ok(())
    }

    /// Tabulates the orbit from `start` to `end` every `step_s` seconds
    /// (and at `end`), for interpolation. Cubic Hermite interpolation
    /// between samples is good to about 0.4 m at 60 s spacing in a low
    /// orbit and 0.3 mm at 10 s.
    #[cfg(feature = "alloc")]
    pub fn sample(
        &self,
        start: Epoch,
        end: Epoch,
        step_s: f64,
    ) -> Result<crate::SampledTrajectory, Error> {
        let span = end.seconds_since(start);
        if step_s.is_nan() || step_s <= 0.0 || span.is_nan() || span <= 0.0 {
            return Err(Error::InvalidInput(
                "sample needs start < end and a positive step",
            ));
        }
        let n = libm::ceil(span / step_s) as usize;
        let mut epochs = alloc::vec::Vec::with_capacity(n + 1);
        for k in 0..n {
            epochs.push(start.add_seconds(k as f64 * step_s)?);
        }
        epochs.push(end);
        // Integrate outwards from the initial epoch in both directions.
        let split_at = epochs.partition_point(|e| *e < self.initial.epoch);
        let mut states = alloc::vec::Vec::with_capacity(epochs.len());
        self.for_each(epochs[..split_at].iter().rev().copied(), |s| states.push(s))?;
        states.reverse();
        self.for_each(epochs[split_at..].iter().copied(), |s| states.push(s))?;
        crate::SampledTrajectory::new(states, step_s * 1.5)
    }

    fn deriv(&self, t: f64, y: &Y) -> Result<Y, Error> {
        let (r, v) = split(y);
        let a = self
            .forces
            .acceleration(self.initial.epoch.add_seconds(t)?, r, v)?;
        Ok([v.x, v.y, v.z, a.x, a.y, a.z])
    }

    // Integrates from (t, y) to `target` with adaptive steps, updating t, y
    // and the next step size h in place.
    fn integrate(
        &self,
        t: &mut f64,
        y: &mut Y,
        h: &mut f64,
        target: f64,
        steps: &mut usize,
    ) -> Result<(), Error> {
        let dir = if target >= *t { 1.0 } else { -1.0 };
        if *h == 0.0 {
            // First step: a hundredth of the time to cross the orbit's radius.
            let (r, v) = split(y);
            *h = (0.01 * r.norm() / v.norm().max(1e-9)).clamp(1e-3, 300.0);
        }
        *h = h.abs() * dir;
        let mut k1 = self.deriv(*t, y)?;
        while (target - *t) * dir > 0.0 {
            *steps += 1;
            if *steps > self.max_steps {
                return Err(Error::InvalidInput(
                    "numerical propagation exceeded max_steps",
                ));
            }
            let last = (*t + *h - target) * dir >= 0.0;
            let step = if last { target - *t } else { *h };
            let mut k = [[0.0; 6]; 7];
            k[0] = k1;
            let mut y5 = *y;
            for s in 1..7 {
                let mut ys = *y;
                for (i, a) in A[s].iter().enumerate().take(s) {
                    if *a != 0.0 {
                        for c in 0..6 {
                            ys[c] += step * a * k[i][c];
                        }
                    }
                }
                k[s] = self.deriv(*t + C[s] * step, &ys)?;
                if s == 6 {
                    y5 = ys; // the 7th stage is evaluated at the 5th-order solution
                }
            }
            // Error estimate, scaled by the tolerance.
            let mut sum = 0.0;
            for c in 0..6 {
                let e: f64 = (0..7).map(|i| E[i] * k[i][c]).sum::<f64>() * step;
                let scale =
                    self.tolerance.absolute + self.tolerance.relative * y[c].abs().max(y5[c].abs());
                sum += (e / scale) * (e / scale);
            }
            let err = libm::sqrt(sum / 6.0);
            if !err.is_finite() {
                return Err(Error::NonFinite);
            }
            let factor = (0.9 * libm::pow(err.max(1e-10), -0.2)).clamp(0.2, 5.0);
            if err <= 1.0 {
                *t = if last { target } else { *t + step };
                *y = y5;
                k1 = k[6];
                // A step shortened to land on the target says nothing new
                // about the step size.
                if !last {
                    *h = step * factor;
                }
            } else {
                *h = step * factor.min(1.0);
                if h.abs() < 1e-6 {
                    return Err(Error::InvalidInput("numerical propagation step underflow"));
                }
            }
        }
        Ok(())
    }
}

impl Locator for NumericalPropagator {
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
        Ok(self.propagate(epoch)?.to_frame(frame, resolver)?)
    }
}
