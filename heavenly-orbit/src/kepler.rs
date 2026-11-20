//! Universal-variable two-body propagation.

use heavenly_core::{Error, Vec3};

/// Propagates a two-body state by `dt` seconds under a central body with
/// gravitational parameter `gm` (m³/s²). Positions in metres, velocities in
/// metres per second.
///
/// One algorithm covers elliptic, parabolic and hyperbolic orbits:
/// Kepler's equation in the universal anomaly χ, solved with the
/// Laguerre–Conway iteration, then Lagrange f and g coefficients. Elliptic
/// propagation is reduced modulo the period first, so long spans do not
/// lose precision.
pub fn propagate(gm: f64, r0: Vec3, v0: Vec3, dt: f64) -> Result<(Vec3, Vec3), Error> {
    if !(gm.is_finite() && r0.is_finite() && v0.is_finite() && dt.is_finite()) {
        return Err(Error::NonFinite);
    }
    if gm <= 0.0 {
        return Err(Error::InvalidInput(
            "gravitational parameter must be positive",
        ));
    }
    let r0n = r0.norm();
    if r0n == 0.0 {
        return Err(Error::InvalidInput(
            "position is at the central body's centre",
        ));
    }
    if r0.cross(v0).norm_squared() == 0.0 {
        return Err(Error::InvalidInput(
            "rectilinear (zero angular momentum) orbits are not supported",
        ));
    }
    if dt == 0.0 {
        return Ok((r0, v0));
    }
    let sqrt_mu = libm::sqrt(gm);
    let rv = r0.dot(v0);
    // alpha = 1/a (> 0 elliptic, 0 parabolic, < 0 hyperbolic).
    let alpha = 2.0 / r0n - v0.norm_squared() / gm;

    let mut t = dt;
    if alpha > 1e-15 / r0n {
        let period = core::f64::consts::TAU / (sqrt_mu * libm::pow(alpha, 1.5));
        t = libm::fmod(dt, period);
    }

    // Initial guess.
    let mut chi = if alpha > 0.0 {
        sqrt_mu * t * alpha
    } else if alpha < 0.0 {
        // Vallado, Algorithm 8.
        let a = 1.0 / alpha;
        let s = if t >= 0.0 { 1.0 } else { -1.0 };
        let arg = (-2.0 * gm * alpha * t) / (rv + s * libm::sqrt(-gm * a) * (1.0 - r0n * alpha));
        s * libm::sqrt(-a) * libm::log(arg.max(1e-300))
    } else {
        sqrt_mu * t / r0n
    };
    if !chi.is_finite() {
        chi = sqrt_mu * t / r0n;
    }

    // Laguerre–Conway on F(χ) = r0·v0/√μ χ²C + (1 − αr0) χ³S + r0 χ − √μ t.
    let n = 5.0;
    let mut converged = false;
    let (mut c, mut s) = stumpff(alpha * chi * chi);
    for _ in 0..200 {
        let z = alpha * chi * chi;
        let (cz, sz) = stumpff(z);
        c = cz;
        s = sz;
        let chi2 = chi * chi;
        let f = rv / sqrt_mu * chi2 * c + (1.0 - alpha * r0n) * chi2 * chi * s + r0n * chi
            - sqrt_mu * t;
        let df = rv / sqrt_mu * chi * (1.0 - z * s) + (1.0 - alpha * r0n) * chi2 * c + r0n;
        let ddf = rv / sqrt_mu * (1.0 - z * c) + (1.0 - alpha * r0n) * chi * (1.0 - z * s);
        let disc = ((n - 1.0) * (n - 1.0) * df * df - n * (n - 1.0) * f * ddf).abs();
        let denom = df + df.signum() * libm::sqrt(disc);
        let delta = n * f / denom;
        chi -= delta;
        if delta.abs() <= 1e-14 * chi.abs().max(1.0) {
            converged = true;
            let z = alpha * chi * chi;
            let (cz, sz) = stumpff(z);
            c = cz;
            s = sz;
            break;
        }
    }
    if !converged || !chi.is_finite() {
        return Err(Error::InvalidInput("Kepler's equation did not converge"));
    }

    let chi2 = chi * chi;
    let f = 1.0 - chi2 / r0n * c;
    let g = t - chi2 * chi / sqrt_mu * s;
    let r = r0 * f + v0 * g;
    let rn = r.norm();
    let fdot = sqrt_mu / (rn * r0n) * (alpha * chi2 * chi * s - chi);
    let gdot = 1.0 - chi2 / rn * c;
    Ok((r, r0 * fdot + v0 * gdot))
}

/// Stumpff functions C(z) and S(z).
fn stumpff(z: f64) -> (f64, f64) {
    if z > 1e-3 {
        let sz = libm::sqrt(z);
        ((1.0 - libm::cos(sz)) / z, (sz - libm::sin(sz)) / (z * sz))
    } else if z < -1e-3 {
        let sz = libm::sqrt(-z);
        (
            (libm::cosh(sz) - 1.0) / -z,
            (libm::sinh(sz) - sz) / (-z * sz),
        )
    } else {
        // Series, accurate to rounding for |z| ≤ 1e-3.
        (
            1.0 / 2.0 - z / 24.0 + z * z / 720.0 - z * z * z / 40_320.0,
            1.0 / 6.0 - z / 120.0 + z * z / 5_040.0 - z * z * z / 362_880.0,
        )
    }
}
