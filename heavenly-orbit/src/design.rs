//! Orbit design helpers.

use crate::gm;

/// Earth's J2 (EGM2008, unnormalised) and the equatorial radius it goes with.
const J2: f64 = 1.082_626_68e-3;
const R_EQ: f64 = 6_378_137.0;

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
        assert!(sun_synchronous_inclination_rad(R_EQ - 1.0, 0.0).is_none());
    }
}
