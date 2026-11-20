//! Moving a covariance along an orbit: the state transition matrix by
//! finite differences, for any propagator.

use heavenly_core::{Covariance, Epoch, Error, Matrix6, StateVector, UncertainState, Vec3};

use crate::NumericalPropagator;

/// The state transition matrix `Φ = ∂x(t)/∂x(t0)` of a propagator, by
/// central differences with Richardson extrapolation: each of the six
/// initial components is nudged up and down by `h` and by `h/2`, and the two
/// estimates are combined as `(4 Φ(h/2) − Φ(h)) / 3`, which cancels the
/// error of order `h²`. `h` is a millionth of the position's and velocity's
/// size (about 7 m and 7 mm/s in a low orbit).
///
/// `propagate` maps an initial state to the state at the target epoch, in
/// the same frame. Because it is any function, this works for every
/// propagator: two-body, J2 secular, SGP4 (from its state) or numerical.
/// Twenty-four propagations are made.
///
/// The nudges are small enough that the orbit is close to linear over them,
/// and large enough that a numerical integrator's error (about a millimetre
/// per day by default) does not swamp the differences.
pub fn state_transition_matrix(
    initial: &StateVector,
    mut propagate: impl FnMut(&StateVector) -> Result<StateVector, Error>,
) -> Result<Matrix6, Error> {
    let coarse = central_differences(initial, 1.0, &mut propagate)?;
    let fine = central_differences(initial, 0.5, &mut propagate)?;
    Ok(core::array::from_fn(|i| {
        core::array::from_fn(|j| (4.0 * fine[i][j] - coarse[i][j]) / 3.0)
    }))
}

fn central_differences(
    initial: &StateVector,
    scale: f64,
    propagate: &mut impl FnMut(&StateVector) -> Result<StateVector, Error>,
) -> Result<Matrix6, Error> {
    let dr = scale * 1e-6 * initial.position_m.norm().max(1.0);
    let dv = scale * 1e-6 * initial.velocity_m_s.norm().max(1e-3);
    let mut phi = [[0.0; 6]; 6];
    for c in 0..6 {
        let h = if c < 3 { dr } else { dv };
        let nudged = |sign: f64| {
            let mut x = [
                initial.position_m.to_array(),
                initial.velocity_m_s.to_array(),
            ];
            x[c / 3][c % 3] += sign * h;
            StateVector {
                position_m: Vec3::from_array(x[0]),
                velocity_m_s: Vec3::from_array(x[1]),
                ..*initial
            }
        };
        let up = propagate(&nudged(1.0))?;
        let down = propagate(&nudged(-1.0))?;
        let du = [
            (up.position_m - down.position_m).to_array(),
            (up.velocity_m_s - down.velocity_m_s).to_array(),
        ];
        for r in 0..6 {
            phi[r][c] = du[r / 3][r % 3] / (2.0 * h);
        }
    }
    Ok(phi)
}

/// An uncertain state propagated to another epoch: the state through
/// `propagate`, and the covariance through the linearised dynamics,
/// `Φ C Φᵀ`.
///
/// Linear propagation is accurate while the uncertainty is small compared
/// with the orbit's curvature over the interval. In a low orbit, an
/// along-track uncertainty of kilometres for days stays well within that;
/// for long spans or large uncertainties, the true distribution becomes
/// curved (banana-shaped) and no Gaussian describes it.
pub fn propagate_uncertain(
    initial: &UncertainState,
    mut propagate: impl FnMut(&StateVector) -> Result<StateVector, Error>,
) -> Result<UncertainState, Error> {
    let phi = state_transition_matrix(&initial.state, &mut propagate)?;
    let state = propagate(&initial.state)?;
    Ok(UncertainState {
        state,
        covariance: initial.covariance.transformed(&phi)?,
    })
}

impl NumericalPropagator {
    /// The state and covariance at `epoch`, for a covariance of the initial
    /// state (in GCRF). The 25 integrations use this propagator's forces
    /// and tolerance.
    pub fn propagate_uncertain(
        &self,
        covariance: &Covariance,
        epoch: Epoch,
    ) -> Result<UncertainState, Error> {
        propagate_uncertain(&UncertainState::new(self.initial(), *covariance), |s| {
            let mut p = NumericalPropagator::new(*s, self.forces)?;
            p.tolerance = self.tolerance;
            p.max_steps = self.max_steps;
            p.propagate(epoch)
        })
    }
}
