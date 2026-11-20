//! Probability of collision at a close approach, from the two objects'
//! states and covariances.

use heavenly_core::{Error, UncertainState, Vec3};

/// The probability that two objects collide at a close approach, with what
/// it was computed from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionProbability {
    /// Probability that the objects come within the hard-body radius.
    pub probability: f64,
    /// Distance between the objects at the epoch of the states (m).
    pub miss_distance_m: f64,
    /// Relative speed (m/s).
    pub relative_speed_m_s: f64,
    /// Standard deviations of the combined position uncertainty in the
    /// encounter plane, along its major and minor axes (m).
    pub sigma_major_m: f64,
    pub sigma_minor_m: f64,
    /// The miss distance in the encounter plane in units of the combined
    /// uncertainty (the Mahalanobis distance). Below about 3 a collision is
    /// statistically plausible; the probability also depends on the
    /// hard-body radius.
    pub miss_sigmas: f64,
}

/// The probability of collision of two objects at their time of closest
/// approach, in the standard short-encounter model (Foster & Estes 1992;
/// Chan; Alfano):
///
/// * the relative motion is a straight line through the encounter, and the
///   uncertainty does not change during it;
/// * the two position errors are independent and Gaussian, so their
///   covariances add;
/// * a collision is the relative position passing within
///   `hard_body_radius_m`, the sum of the objects' radii.
///
/// The combined position covariance is projected onto the encounter plane
/// (perpendicular to the relative velocity), and the 2-D Gaussian is
/// integrated over the disc of the hard-body radius: exactly across the disc
/// (with `erf`) and by Simpson's rule along it.
///
/// Both states must be in the same inertial frame (GCRF, say) and at the
/// same epoch, ideally the time of closest approach from
/// [`catalog::conjunctions`](crate::catalog::conjunctions). The model
/// does not apply when the relative speed is so low (co-orbiting objects,
/// under about 10 m/s in low orbit) that the encounter lasts minutes; that
/// fails with [`Error::InvalidInput`].
pub fn collision_probability(
    a: &UncertainState,
    b: &UncertainState,
    hard_body_radius_m: f64,
) -> Result<CollisionProbability, Error> {
    if a.state.frame != b.state.frame {
        return Err(Error::FrameMismatch {
            expected: a.state.frame,
            found: b.state.frame,
        });
    }
    if a.state.epoch != b.state.epoch {
        return Err(Error::EpochMismatch {
            expected: a.state.epoch,
            found: b.state.epoch,
        });
    }
    if hard_body_radius_m.is_nan() || hard_body_radius_m <= 0.0 {
        return Err(Error::InvalidInput("the hard-body radius must be positive"));
    }
    let r = b.state.position_m - a.state.position_m;
    let v = b.state.velocity_m_s - a.state.velocity_m_s;
    let speed = v.norm();
    if speed < 1e-3 {
        return Err(Error::InvalidInput(
            "the short-encounter model needs a relative velocity",
        ));
    }
    // Encounter plane: z along the relative velocity, x towards the miss
    // (its part across the velocity), y completing the set.
    let z = v / speed;
    let across = r - z * r.dot(z);
    let x = across.normalized().unwrap_or_else(|| {
        // A head-on hit: any direction across the velocity will do.
        let t = if libm::fabs(z.x) < 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        (t - z * t.dot(z)).normalized().unwrap_or(Vec3::X)
    });
    let y = z.cross(x);
    let c = a.covariance.position();
    let cb = b.covariance.position();
    let quad = |p: Vec3, q: Vec3| {
        let (p, q) = (p.to_array(), q.to_array());
        let mut s = 0.0;
        for i in 0..3 {
            for j in 0..3 {
                s += p[i] * (c[i][j] + cb[i][j]) * q[j];
            }
        }
        s
    };
    let (sxx, sxy, syy) = (quad(x, x), quad(x, y), quad(y, y));
    // Principal axes of the 2-D covariance.
    let half_tr = 0.5 * (sxx + syy);
    let disc = libm::sqrt(0.25 * (sxx - syy) * (sxx - syy) + sxy * sxy);
    let (l1, l2) = (half_tr + disc, half_tr - disc);
    if l2.is_nan() || l2 <= 1e-12 * l1.max(f64::MIN_POSITIVE) {
        return Err(Error::InvalidInput(
            "the combined covariance is degenerate in the encounter plane",
        ));
    }
    let theta = 0.5 * libm::atan2(2.0 * sxy, sxx - syy);
    let (st, ct) = (libm::sin(theta), libm::cos(theta));
    let miss = across.norm();
    // The miss vector (miss, 0) in the principal axes.
    let (mx, my) = (miss * ct, -miss * st);
    let (sx, sy) = (libm::sqrt(l1), libm::sqrt(l2));
    let probability = disc_probability(mx, my, sx, sy, hard_body_radius_m);
    Ok(CollisionProbability {
        probability,
        miss_distance_m: r.norm(),
        relative_speed_m_s: speed,
        sigma_major_m: sx,
        sigma_minor_m: sy,
        miss_sigmas: libm::sqrt(mx * mx / l1 + my * my / l2),
    })
}

/// `P(|X| ≤ R)` for `X ~ N((mx, my), diag(sx², sy²))` in 2-D.
fn disc_probability(mx: f64, my: f64, sx: f64, sy: f64, radius: f64) -> f64 {
    use core::f64::consts::{FRAC_1_SQRT_2, PI};
    // Only x within 9 σ of the mean contributes (to 1e-18).
    let lo = (mx - 9.0 * sx).max(-radius);
    let hi = (mx + 9.0 * sx).min(radius);
    if lo >= hi {
        return 0.0;
    }
    // x = R sin t removes the square-root singularity at the disc's edge.
    let (t0, t1) = (libm::asin(lo / radius), libm::asin(hi / radius));
    let f = |t: f64| {
        let xx = radius * libm::sin(t);
        let half = radius * libm::cos(t);
        let across = 0.5
            * (libm::erf((half - my) * FRAC_1_SQRT_2 / sy)
                - libm::erf((-half - my) * FRAC_1_SQRT_2 / sy));
        let along =
            libm::exp(-0.5 * (xx - mx) * (xx - mx) / (sx * sx)) / (sx * libm::sqrt(2.0 * PI));
        along * across * half
    };
    // Composite Simpson's rule.
    const N: usize = 2_000;
    let h = (t1 - t0) / N as f64;
    let mut s = f(t0) + f(t1);
    for k in 1..N {
        s += f(t0 + k as f64 * h) * if k % 2 == 1 { 4.0 } else { 2.0 };
    }
    (s * h / 3.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isotropic_head_on_has_a_closed_form() {
        // Zero miss, circular uncertainty σ: P = 1 − exp(−R²/2σ²).
        for (sigma, radius) in [(100.0, 10.0), (10.0, 10.0), (3.0, 20.0), (1_000.0, 5.0)] {
            let p = disc_probability(0.0, 0.0, sigma, sigma, radius);
            let exact = -libm::expm1(-radius * radius / (2.0 * sigma * sigma));
            assert!(
                (p - exact).abs() <= 1e-10 * exact.max(1e-12),
                "{sigma} {radius}: {p} vs {exact}"
            );
        }
    }

    #[test]
    fn far_misses_and_tiny_spreads() {
        assert_eq!(disc_probability(1e5, 0.0, 10.0, 10.0, 5.0), 0.0);
        // A spread much smaller than the disc, centred inside it: certain.
        let p = disc_probability(1.0, -2.0, 0.01, 0.002, 20.0);
        assert!((p - 1.0).abs() < 1e-9, "{p}");
        // Centred on the edge: half.
        let p = disc_probability(20.0, 0.0, 0.001, 0.001, 20.0);
        assert!((p - 0.5).abs() < 1e-3, "{p}");
    }
}
