//! Conic (Keplerian) orbital elements.

use heavenly_core::{Epoch, Error, Rotation, Vec3};

use crate::kepler;

/// Osculating conic elements, using the same conventions as SPICE's
/// `conics` / `oscelt`.
///
/// Periapsis distance and eccentricity are used rather than the semi-major
/// axis, so circular, elliptic, parabolic and hyperbolic orbits share one
/// representation. Angles are relative to the axes of whatever inertial frame
/// the elements are given in, which is usually an ICRF-aligned frame centred
/// on the attracting body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConicElements {
    /// Periapsis distance `q`, in metres.
    pub periapsis_m: f64,
    /// Eccentricity `e` (0 circular, < 1 elliptic, 1 parabolic, > 1 hyperbolic).
    pub eccentricity: f64,
    /// Inclination `i`, in radians.
    pub inclination_rad: f64,
    /// Longitude of the ascending node `Ω`, in radians.
    pub ascending_node_rad: f64,
    /// Argument of periapsis `ω`, in radians.
    pub argument_of_periapsis_rad: f64,
    /// Mean anomaly `M₀` at `epoch`, in radians. For hyperbolic orbits this
    /// is `e·sinh H − H`; for parabolic orbits `D + D³/3` with `D = tan(ν/2)`.
    pub mean_anomaly_rad: f64,
    /// The epoch at which the mean anomaly applies.
    pub epoch: Epoch,
    /// Gravitational parameter of the attracting body, in m³/s².
    pub gm_m3_s2: f64,
}

impl ConicElements {
    /// Validates the elements.
    pub fn validate(&self) -> Result<(), Error> {
        let fields = [
            self.periapsis_m,
            self.eccentricity,
            self.inclination_rad,
            self.ascending_node_rad,
            self.argument_of_periapsis_rad,
            self.mean_anomaly_rad,
            self.gm_m3_s2,
        ];
        if fields.iter().any(|v| !v.is_finite()) {
            return Err(Error::NonFinite);
        }
        if self.periapsis_m <= 0.0 {
            return Err(Error::InvalidInput("periapsis distance must be positive"));
        }
        if self.eccentricity < 0.0 {
            return Err(Error::InvalidInput("eccentricity must not be negative"));
        }
        if self.gm_m3_s2 <= 0.0 {
            return Err(Error::InvalidInput(
                "gravitational parameter must be positive",
            ));
        }
        if !(0.0..=core::f64::consts::PI).contains(&self.inclination_rad) {
            return Err(Error::InvalidInput("inclination must be within [0, π]"));
        }
        Ok(())
    }

    /// Mean motion `n` in rad/s: `√(μ/|a|³)` for elliptic and hyperbolic
    /// orbits, and `√(μ/(2q³))` for parabolic ones.
    pub fn mean_motion_rad_s(&self) -> f64 {
        let (q, e, mu) = (self.periapsis_m, self.eccentricity, self.gm_m3_s2);
        if e == 1.0 {
            libm::sqrt(mu / (2.0 * q * q * q))
        } else {
            let a = (q / (1.0 - e)).abs();
            libm::sqrt(mu / (a * a * a))
        }
    }

    /// Orbital period in seconds, for elliptic orbits.
    pub fn period_s(&self) -> Option<f64> {
        (self.eccentricity < 1.0).then(|| core::f64::consts::TAU / self.mean_motion_rad_s())
    }

    /// Position (m) and velocity (m/s) at `epoch`.
    pub fn state_at(&self, epoch: Epoch) -> Result<(Vec3, Vec3), Error> {
        self.validate()?;
        let (q, e, mu) = (self.periapsis_m, self.eccentricity, self.gm_m3_s2);
        // State at periapsis, in the perifocal frame, rotated into the
        // reference axes by Rz(Ω) Rx(i) Rz(ω).
        let rot = Rotation::from_axis_angle(Vec3::Z, self.ascending_node_rad)
            .compose(Rotation::from_axis_angle(Vec3::X, self.inclination_rad))
            .compose(Rotation::from_axis_angle(
                Vec3::Z,
                self.argument_of_periapsis_rad,
            ));
        let rp = rot.apply(Vec3::new(q, 0.0, 0.0));
        let vp = rot.apply(Vec3::new(0.0, libm::sqrt(mu * (1.0 + e) / q), 0.0));
        // Time from periapsis to `epoch`.
        // Intervals are in TDB, the time argument of SPICE and the DE
        // ephemerides. (For geocentric orbits TT is the strictly correct
        // scale; the two differ by a periodic term of at most 1.7 ms.)
        let dt = epoch.tdb_seconds_since(self.epoch);
        let since_periapsis = self.mean_anomaly_rad / self.mean_motion_rad_s() + dt;
        kepler::propagate(mu, rp, vp, since_periapsis)
    }

    /// Osculating elements of a state (m, m/s) at `epoch`.
    ///
    /// Degenerate geometries follow SPICE's `oscelt`: for an equatorial orbit
    /// the node is 0 and `ω` is measured from the x axis; for a circular
    /// orbit `ω` is 0 and the mean anomaly is measured from the node.
    pub fn from_state(
        position_m: Vec3,
        velocity_m_s: Vec3,
        epoch: Epoch,
        gm_m3_s2: f64,
    ) -> Result<ConicElements, Error> {
        if !(position_m.is_finite() && velocity_m_s.is_finite() && gm_m3_s2.is_finite()) {
            return Err(Error::NonFinite);
        }
        if gm_m3_s2 <= 0.0 {
            return Err(Error::InvalidInput(
                "gravitational parameter must be positive",
            ));
        }
        let mu = gm_m3_s2;
        let (r, v) = (position_m, velocity_m_s);
        let rn = r.norm();
        let h = r.cross(v);
        let hn = h.norm();
        if rn == 0.0 || hn == 0.0 {
            return Err(Error::InvalidInput(
                "rectilinear (zero angular momentum) orbits are not supported",
            ));
        }
        let e_vec = (r * (v.norm_squared() - mu / rn) - v * r.dot(v)) / mu;
        let mut e = e_vec.norm();
        // Snap round-off near circular / parabolic, as SPICE does.
        if e < 1e-10 {
            e = 0.0;
        } else if (e - 1.0).abs() < 1e-10 {
            e = 1.0;
        }
        let q = hn * hn / (mu * (1.0 + e));
        let inc = libm::acos((h.z / hn).clamp(-1.0, 1.0));

        let node_vec = Vec3::Z.cross(h);
        let equatorial = node_vec.norm() <= 1e-12 * hn;
        let node = if equatorial {
            Vec3::X
        } else {
            node_vec.normalized().unwrap()
        };
        let omega_node = if equatorial {
            0.0
        } else {
            libm::atan2(node.y, node.x)
        };
        let hu = h / hn;

        // Angle from `from` to `to`, measured positively about the orbit normal.
        let angle = |from: Vec3, to: Vec3| libm::atan2(hu.dot(from.cross(to)), from.dot(to));

        let (argp, nu) = if e == 0.0 {
            (0.0, angle(node, r))
        } else {
            (angle(node, e_vec), angle(e_vec, r))
        };

        let m = if e < 1.0 {
            let ea = 2.0 * libm::atan(libm::sqrt((1.0 - e) / (1.0 + e)) * libm::tan(nu / 2.0));
            ea - e * libm::sin(ea)
        } else if e > 1.0 {
            let ha = 2.0 * libm::atanh(libm::sqrt((e - 1.0) / (e + 1.0)) * libm::tan(nu / 2.0));
            e * libm::sinh(ha) - ha
        } else {
            let d = libm::tan(nu / 2.0);
            d + d * d * d / 3.0
        };

        let wrap = |a: f64| {
            let r = libm::fmod(a, core::f64::consts::TAU);
            if r < 0.0 {
                r + core::f64::consts::TAU
            } else {
                r
            }
        };
        Ok(ConicElements {
            periapsis_m: q,
            eccentricity: e,
            inclination_rad: inc,
            ascending_node_rad: wrap(omega_node),
            argument_of_periapsis_rad: wrap(argp),
            mean_anomaly_rad: if e < 1.0 { wrap(m) } else { m },
            epoch,
            gm_m3_s2: mu,
        })
    }
}
