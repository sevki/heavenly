//! Signal light time between two moving objects.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};

/// The speed of light in vacuum, in m/s (exact, by definition of the metre).
pub const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Which end of the signal is fixed at the given epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// The observer *receives* at the epoch; the target transmitted earlier.
    /// This is what you see: SPICE's `CN` correction.
    Reception,
    /// The observer *transmits* at the epoch; the target receives later.
    /// This is where to point a transmitter: SPICE's `XCN` correction.
    Transmission,
}

/// The result of a light-time solution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightTime {
    /// One-way light time in seconds.
    pub light_time_s: f64,
    /// The epoch at the target end: `epoch − τ` on reception, `epoch + τ` on
    /// transmission, with τ counted in TDB.
    pub target_epoch: Epoch,
    /// Target position at `target_epoch` minus observer position at the
    /// observer's epoch, in the solution frame, in metres. This is the
    /// direction the observer should point.
    pub relative_position_m: Vec3,
    /// The light-time-corrected range, `c·τ`, in metres.
    pub range_m: f64,
    /// Rate of change of the corrected range as the observer's epoch
    /// advances, in m/s (positive when receding).
    pub range_rate_m_s: f64,
    /// Fixed-point iterations used.
    pub iterations: u32,
}

/// Solves for the one-way light time between `observer` and `target`.
///
/// The solution is Newtonian and converged, like SPICE's `CN` / `XCN`. It
/// iterates `τ = |r_T(t ∓ τ) − r_O(t)| / c` until τ changes by less than
/// 1 ps. `frame` must be inertial; use [`FrameId::SSB_ICRF`] for anything
/// beyond the Earth–Moon system. For near-Earth work, `GCRF` errs by the
/// Earth's acceleration times τ², under a micrometre for the Moon.
///
/// Not modelled: relativistic (Shapiro) delay, about 0.1 ns for an
/// Earth–Moon path and tens of µs for a ray grazing the Sun, and
/// atmospheric delay.
pub fn light_time<O, T, R, E>(
    observer: &O,
    target: &T,
    epoch: Epoch,
    frame: FrameId,
    resolver: &R,
    direction: Direction,
) -> Result<LightTime, E>
where
    O: Locator + ?Sized,
    T: Locator + ?Sized,
    R: FrameResolver + ?Sized,
    O::Error: From<R::Error>,
    T::Error: From<R::Error>,
    E: From<O::Error> + From<T::Error> + From<Error>,
{
    let c = SPEED_OF_LIGHT_M_S;
    let obs: StateVector = observer.state_at(epoch, frame, resolver)?;
    let sign = match direction {
        Direction::Reception => -1.0,
        Direction::Transmission => 1.0,
    };
    // τ is an interval of coordinate time in `frame`; in the barycentric
    // frame that is TDB, the time scale SPICE and the DE ephemerides use.
    let target_at = |tau: f64| -> Result<StateVector, E> {
        let t = epoch.add_tdb_seconds(sign * tau)?;
        Ok(target.state_at(t, frame, resolver)?)
    };
    let mut tgt = target_at(0.0)?;
    let mut tau = (tgt.position_m - obs.position_m).norm() / c;
    let mut iterations = 0;
    loop {
        iterations += 1;
        tgt = target_at(tau)?;
        let next = (tgt.position_m - obs.position_m).norm() / c;
        let done = (next - tau).abs() < 1e-12;
        tau = next;
        if done {
            break;
        }
        if iterations >= 20 {
            return Err(Error::InvalidInput(
                "light time did not converge (is the target faster than light?)",
            )
            .into());
        }
    }
    // Recompute the state at the converged epoch so position and τ agree.
    let target_epoch = epoch.add_tdb_seconds(sign * tau)?;
    tgt = target.state_at(target_epoch, frame, resolver)?;
    let rho = tgt.position_m - obs.position_m;
    let u = rho
        .normalized()
        .ok_or(Error::InvalidInput("observer and target coincide"))?;
    // d|ρ|/dt with ρ = r_T(t ∓ τ(t)) − r_O(t) and τ = |ρ|/c:
    //   ρ̇ = u·(v_T − v_O) / (1 ± u·v_T / c)
    let denom = 1.0 - sign * u.dot(tgt.velocity_m_s) / c;
    let range_rate_m_s = u.dot(tgt.velocity_m_s - obs.velocity_m_s) / denom;
    Ok(LightTime {
        light_time_s: tau,
        target_epoch,
        relative_position_m: rho,
        range_m: rho.norm(),
        range_rate_m_s,
        iterations,
    })
}
