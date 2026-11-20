//! Orbit design helpers.

use heavenly_core::Epoch;

use crate::{gm, ConicElements};

/// A Walker delta constellation `i: t/p/f`: `total` satellites in `planes`
/// equally spaced planes, all circular at `semi_major_axis_m` and
/// `inclination_rad`. Satellites are equally spaced within each plane, and
/// each plane is shifted along its orbit by `phasing × 360°/total` from the
/// one before. `node0_rad` is the first plane's ascending node.
///
/// Returns them plane by plane, at `epoch`, about the Earth. Fails if
/// `total` is not a multiple of `planes`, or `phasing` is not below
/// `planes`.
#[allow(clippy::too_many_arguments)]
pub fn walker_delta(
    total: usize,
    planes: usize,
    phasing: usize,
    inclination_rad: f64,
    semi_major_axis_m: f64,
    node0_rad: f64,
    epoch: Epoch,
) -> Option<impl Iterator<Item = ConicElements>> {
    if planes == 0 || total % planes != 0 || phasing >= planes.max(1) {
        return None;
    }
    let per_plane = total / planes;
    let tau = core::f64::consts::TAU;
    Some((0..total).map(move |k| {
        let (j, s) = (k / per_plane, k % per_plane);
        ConicElements {
            periapsis_m: semi_major_axis_m,
            eccentricity: 0.0,
            inclination_rad,
            ascending_node_rad: node0_rad + tau * j as f64 / planes as f64,
            argument_of_periapsis_rad: 0.0,
            mean_anomaly_rad: tau * s as f64 / per_plane as f64
                + tau * (phasing * j) as f64 / total as f64,
            epoch,
            gm_m3_s2: gm::EARTH,
        }
    }))
}

/// Earth's J2 (EGM2008, unnormalised) and the equatorial radius it goes with.
const J2: f64 = crate::forces::EARTH_ZONALS[0];
const R_EQ: f64 = crate::forces::EARTH_RADIUS_M;

/// The rate at which a sun-synchronous orbit's node must turn: once per
/// tropical year (365.242 19 days), in rad/s.
pub const SUN_SYNCHRONOUS_NODE_RATE: f64 = core::f64::consts::TAU / (365.242_19 * 86_400.0);

/// The inclination (rad) at which the Earth's oblateness turns an orbit's
/// plane once a year, so it keeps the same angle to the Sun: a
/// sun-synchronous orbit.
///
/// Uses the secular J2 nodal rate, `Ω̇ = −(3/2) n J2 (R/p)² cos i`. Returns
/// `None` when no inclination is fast enough (above about 5 970 km
/// altitude for a circular orbit) or the inputs are not a closed orbit.
pub fn sun_synchronous_inclination_rad(semi_major_axis_m: f64, eccentricity: f64) -> Option<f64> {
    if !(semi_major_axis_m > R_EQ && (0.0..1.0).contains(&eccentricity)) {
        return None;
    }
    let a = semi_major_axis_m;
    let n = libm::sqrt(gm::EARTH / (a * a * a));
    let p = a * (1.0 - eccentricity * eccentricity);
    let cos_i = -SUN_SYNCHRONOUS_NODE_RATE / (1.5 * n * J2 * (R_EQ / p) * (R_EQ / p));
    (-1.0..=1.0).contains(&cos_i).then(|| libm::acos(cos_i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn familiar_sun_synchronous_inclinations() {
        // Circular orbits; published values (e.g. Vallado, table 11-4;
        // Landsat 8 at 705 km flies 98.2°).
        for (alt_km, deg) in [(550.0, 97.59), (705.0, 98.21), (800.0, 98.60)] {
            let i = sun_synchronous_inclination_rad(R_EQ + alt_km * 1e3, 0.0)
                .unwrap()
                .to_degrees();
            assert!((i - deg).abs() < 0.02, "{alt_km} km: {i}");
        }
        assert!(sun_synchronous_inclination_rad(R_EQ + 7_000e3, 0.0).is_none());
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn walker_delta_layout() {
        let e = Epoch::J2000;
        let w: alloc::vec::Vec<_> = walker_delta(24, 3, 1, 1.0, 7e6, 0.0, e).unwrap().collect();
        assert_eq!(w.len(), 24);
        let tau = core::f64::consts::TAU;
        // Planes 120° apart; 8 per plane, 45° apart; 15° phase per plane.
        assert!((w[8].ascending_node_rad - tau / 3.0).abs() < 1e-15);
        assert!((w[1].mean_anomaly_rad - tau / 8.0).abs() < 1e-15);
        assert!((w[8].mean_anomaly_rad - tau / 24.0).abs() < 1e-15);
        assert!(walker_delta(24, 5, 1, 1.0, 7e6, 0.0, e).is_none());
        assert!(walker_delta(24, 3, 3, 1.0, 7e6, 0.0, e).is_none());
        assert!(walker_delta(24, 0, 0, 1.0, 7e6, 0.0, e).is_none());
        assert!(sun_synchronous_inclination_rad(R_EQ - 1.0, 0.0).is_none());
    }
}
