//! The forces on an Earth satellite, as accelerations in GCRF: the Earth's
//! gravity with its zonal harmonics, atmospheric drag, solar radiation
//! pressure, and the pull of the Sun and the Moon.

use heavenly_core::{Epoch, Error, Vec3};

use crate::{analytic, gm};

/// WGS84 / EGM2008 equatorial radius of the Earth, in metres.
pub const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// WGS84 flattening.
pub const EARTH_FLATTENING: f64 = 1.0 / 298.257_223_563;

/// The Earth's rotation rate (IERS nominal), rad/s.
pub const EARTH_ROTATION_RAD_S: f64 = 7.292_115e-5;

/// The Earth's zonal harmonics J2…J6 (EGM2008, unnormalised; tide-free).
///
/// `Jn = −C̄n0 · √(2n+1)` from the normalised EGM2008 coefficients C̄20 =
/// −4.841 651 437 908 15e-4, C̄30 = 9.571 612 070 934 73e-7, C̄40 =
/// 5.399 658 666 389 91e-7, C̄50 = 6.867 029 137 366 81e-8 and C̄60 =
/// −1.499 539 279 785 27e-7.
pub const EARTH_ZONALS: [f64; 5] = [
    1.082_626_173_852_222_7e-3,
    -2.532_410_518_567_722_5e-6,
    -1.619_897_599_916_973_1e-6,
    -2.277_535_907_308_361_8e-7,
    5.406_665_762_838_132e-7,
];

/// Solar radiation pressure at 1 AU, N/m² (solar constant 1361 W/m² / c).
pub const SOLAR_PRESSURE_1AU_N_M2: f64 = 4.54e-6;

/// The zonal (axially symmetric) part of a body's gravity field: the
/// potential `μ/r · [1 − Σ Jn (R/r)ⁿ Pn(sin φ)]`, with latitude φ measured
/// from the equator of `pole`.
#[derive(Clone, Copy, Debug)]
pub struct Zonal {
    pub equatorial_radius_m: f64,
    /// J2, J3, … J6. Only the first `degree − 1` are used.
    pub j: [f64; 5],
    /// The highest degree used, 2 to 6.
    pub degree: u8,
    /// The body's rotation axis in GCRF.
    pub pole: fn(Epoch) -> Vec3,
}

impl Zonal {
    /// The Earth to degree `degree` (EGM2008), about its precessing and
    /// nutating axis ([`analytic::earth_pole`]).
    pub const fn earth(degree: u8) -> Zonal {
        Zonal {
            equatorial_radius_m: EARTH_RADIUS_M,
            j: EARTH_ZONALS,
            degree,
            pole: analytic::earth_pole,
        }
    }

    /// The acceleration (m/s²) at `r` (m, GCRF) from the zonal terms alone,
    /// for a body with gravitational parameter `gm`.
    pub fn acceleration(&self, gm: f64, epoch: Epoch, r: Vec3) -> Vec3 {
        let pole = (self.pole)(epoch);
        let rn = r.norm();
        let rhat = r / rn;
        let u = rhat.dot(pole);
        let k = self.equatorial_radius_m / rn;
        // Legendre polynomials and their derivatives by recursion:
        // n Pn = (2n−1) u Pn−1 − (n−1) Pn−2,  P′n = P′n−2 + (2n−1) Pn−1.
        let (mut p, mut dp) = ([0.0; 7], [0.0; 7]);
        p[0] = 1.0;
        p[1] = u;
        dp[1] = 1.0;
        let top = self.degree.clamp(2, 6) as usize;
        let (mut radial, mut axial) = (0.0, 0.0);
        let mut kn = k;
        for n in 2..=top {
            let nf = n as f64;
            p[n] = ((2.0 * nf - 1.0) * u * p[n - 1] - (nf - 1.0) * p[n - 2]) / nf;
            dp[n] = dp[n - 2] + (2.0 * nf - 1.0) * p[n - 1];
            kn *= k;
            let jn = self.j[n - 2] * kn;
            // a = μ/r² Σ Jn (R/r)ⁿ [((n+1) Pn + u P′n) r̂ − P′n ẑ]
            radial += jn * ((nf + 1.0) * p[n] + u * dp[n]);
            axial += jn * dp[n];
        }
        (rhat * radial - pole * axial) * (gm / (rn * rn))
    }

    /// The zonal part of the gravitational potential (J/kg): the potential
    /// is `U = μ/r + potential(…)`, and the acceleration is `∇U`.
    pub fn potential(&self, gm: f64, epoch: Epoch, r: Vec3) -> f64 {
        let rn = r.norm();
        let u = r.dot((self.pole)(epoch)) / rn;
        let k = self.equatorial_radius_m / rn;
        let (mut p0, mut p1, mut kn, mut sum) = (1.0, u, k, 0.0);
        for n in 2..=self.degree.clamp(2, 6) as usize {
            let nf = n as f64;
            let p2 = ((2.0 * nf - 1.0) * u * p1 - (nf - 1.0) * p0) / nf;
            kn *= k;
            sum += self.j[n - 2] * kn * p2;
            (p0, p1) = (p1, p2);
        }
        -gm / rn * sum
    }
}

/// Atmospheric drag on a satellite: `a = −½ ρ (Cd A / m) |v_rel| v_rel`,
/// with the atmosphere turning with the Earth.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    /// Drag coefficient × area / mass (m²/kg). The ISS is about 0.005;
    /// a small cubesat about 0.02.
    pub cd_area_m2_per_kg: f64,
    /// Density (kg/m³) at a height above the ellipsoid (m).
    pub density: fn(f64) -> f64,
}

impl Drag {
    /// Drag with the [`exponential_density`] atmosphere.
    pub const fn exponential(cd_area_m2_per_kg: f64) -> Drag {
        Drag {
            cd_area_m2_per_kg,
            density: exponential_density,
        }
    }
}

/// Vallado's exponential atmosphere (*Fundamentals of Astrodynamics and
/// Applications*, table 8-4): a static, mean-solar-activity density from
/// sea level to 1000 km, in kg/m³, for a height in metres. It ignores the
/// Sun's daily heating and the solar cycle, which change the density at
/// 400 km by a factor of ten, so decay times from it are typical, not
/// predictions.
pub fn exponential_density(height_m: f64) -> f64 {
    // (base height km, base density kg/m³, scale height km)
    const LAYERS: [(f64, f64, f64); 28] = [
        (0.0, 1.225, 7.249),
        (25.0, 3.899e-2, 6.349),
        (30.0, 1.774e-2, 6.682),
        (40.0, 3.972e-3, 7.554),
        (50.0, 1.057e-3, 8.382),
        (60.0, 3.206e-4, 7.714),
        (70.0, 8.770e-5, 6.549),
        (80.0, 1.905e-5, 5.799),
        (90.0, 3.396e-6, 5.382),
        (100.0, 5.297e-7, 5.877),
        (110.0, 9.661e-8, 7.263),
        (120.0, 2.438e-8, 9.473),
        (130.0, 8.484e-9, 12.636),
        (140.0, 3.845e-9, 16.149),
        (150.0, 2.070e-9, 22.523),
        (180.0, 5.464e-10, 29.740),
        (200.0, 2.789e-10, 37.105),
        (250.0, 7.248e-11, 45.546),
        (300.0, 2.418e-11, 53.628),
        (350.0, 9.518e-12, 53.298),
        (400.0, 3.725e-12, 58.515),
        (450.0, 1.585e-12, 60.828),
        (500.0, 6.967e-13, 63.822),
        (600.0, 1.454e-13, 71.835),
        (700.0, 3.614e-14, 88.667),
        (800.0, 1.170e-14, 124.64),
        (900.0, 5.245e-15, 181.05),
        (1000.0, 3.019e-15, 268.00),
    ];
    let h = height_m / 1e3;
    if h.is_nan() {
        return f64::NAN;
    }
    let layer = LAYERS.iter().rev().find(|l| h >= l.0).unwrap_or(&LAYERS[0]);
    layer.1 * libm::exp(-(h - layer.0) / layer.2)
}

/// Solar radiation pressure on a "cannonball":
/// `a = P☉ (AU/d)² Cr (A/m)` away from the Sun, switched off in the Earth's
/// cylindrical shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiationPressure {
    /// Reflectivity coefficient × area / mass (m²/kg). Cr is 1 for a
    /// black body and up to 2 for a mirror facing the Sun.
    pub cr_area_m2_per_kg: f64,
}

/// A distant body that pulls on the satellite and on the Earth alike, so
/// only the difference (the tide) moves the orbit about the Earth.
#[derive(Clone, Copy, Debug)]
pub struct ThirdBody {
    pub gm_m3_s2: f64,
    /// The body's geocentric position in GCRF (m).
    pub position_m: fn(Epoch) -> Vec3,
}

impl ThirdBody {
    /// The Sun, from [`analytic::sun_position_m`].
    pub const SUN: ThirdBody = ThirdBody {
        gm_m3_s2: gm::SUN,
        position_m: analytic::sun_position_m,
    };
    /// The Moon, from [`analytic::moon_position_m`].
    pub const MOON: ThirdBody = ThirdBody {
        gm_m3_s2: gm::MOON,
        position_m: analytic::moon_position_m,
    };

    /// The tidal acceleration at `r` (m, GCRF).
    pub fn acceleration(&self, epoch: Epoch, r: Vec3) -> Vec3 {
        let s = (self.position_m)(epoch);
        let d = s - r;
        let (dn, sn) = (d.norm(), s.norm());
        (d / (dn * dn * dn) - s / (sn * sn * sn)) * self.gm_m3_s2
    }
}

/// Everything that accelerates a satellite about the Earth, in GCRF.
///
/// Start from [`ForceModel::two_body`] or [`ForceModel::earth`] and switch
/// terms on or off:
///
/// ```
/// use heavenly_orbit::{Drag, ForceModel, RadiationPressure};
/// let iss = ForceModel {
///     drag: Some(Drag::exponential(0.005)),
///     radiation: Some(RadiationPressure { cr_area_m2_per_kg: 0.006 }),
///     ..ForceModel::earth()
/// };
/// # let _ = iss;
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ForceModel {
    /// The central body's gravitational parameter (m³/s²).
    pub gm_m3_s2: f64,
    pub zonal: Option<Zonal>,
    pub drag: Option<Drag>,
    pub radiation: Option<RadiationPressure>,
    pub sun: Option<ThirdBody>,
    pub moon: Option<ThirdBody>,
}

impl ForceModel {
    /// A point mass: Keplerian motion.
    pub const fn two_body(gm_m3_s2: f64) -> ForceModel {
        ForceModel {
            gm_m3_s2,
            zonal: None,
            drag: None,
            radiation: None,
            sun: None,
            moon: None,
        }
    }

    /// The Earth with J2 only: the largest perturbation by far, which turns
    /// orbit planes and apsides.
    pub const fn earth_j2() -> ForceModel {
        ForceModel {
            zonal: Some(Zonal::earth(2)),
            ..ForceModel::two_body(gm::EARTH)
        }
    }

    /// The Earth's gravity to J6 plus the Sun and the Moon. Add drag and
    /// radiation pressure for a particular satellite.
    pub const fn earth() -> ForceModel {
        ForceModel {
            zonal: Some(Zonal::earth(6)),
            sun: Some(ThirdBody::SUN),
            moon: Some(ThirdBody::MOON),
            ..ForceModel::two_body(gm::EARTH)
        }
    }

    /// The acceleration (m/s²) at position `r` (m) and velocity `v` (m/s)
    /// in GCRF.
    ///
    /// Fails with [`Error::InvalidInput`] inside the central body (for drag,
    /// below the Earth's surface), where the orbit has ended.
    pub fn acceleration(&self, epoch: Epoch, r: Vec3, v: Vec3) -> Result<Vec3, Error> {
        let rn = r.norm();
        if !r.is_finite() || !v.is_finite() || rn == 0.0 {
            return Err(Error::NonFinite);
        }
        let mut a = r * (-self.gm_m3_s2 / (rn * rn * rn));
        if let Some(z) = &self.zonal {
            if rn < z.equatorial_radius_m * (1.0 - EARTH_FLATTENING) {
                return Err(Error::InvalidInput("the orbit reached the surface"));
            }
            a += z.acceleration(self.gm_m3_s2, epoch, r);
        }
        if let Some(d) = &self.drag {
            let pole = analytic::earth_pole(epoch);
            let u = r.dot(pole) / rn;
            // Height above the ellipsoid, to first order in the flattening.
            let h = rn - EARTH_RADIUS_M * (1.0 - EARTH_FLATTENING * u * u);
            if h < 0.0 {
                return Err(Error::InvalidInput("the orbit reached the surface"));
            }
            let rel = v - (pole * EARTH_ROTATION_RAD_S).cross(r);
            a += rel * (-0.5 * (d.density)(h) * d.cd_area_m2_per_kg * rel.norm());
        }
        if let Some(b) = &self.sun {
            a += b.acceleration(epoch, r);
        }
        if let Some(b) = &self.moon {
            a += b.acceleration(epoch, r);
        }
        if let Some(p) = &self.radiation {
            let sun = self.sun.map_or_else(
                || analytic::sun_position_m(epoch),
                |s| (s.position_m)(epoch),
            );
            let away = r - sun;
            let d = away.norm();
            let shat = sun / sun.norm();
            let along = r.dot(shat);
            let in_shadow = along < 0.0 && (r - shat * along).norm() < EARTH_RADIUS_M;
            if !in_shadow {
                let k = SOLAR_PRESSURE_1AU_N_M2 * (analytic::AU_M / d) * (analytic::AU_M / d);
                a += away * (k * p.cr_area_m2_per_kg / d);
            }
        }
        Ok(a)
    }
}
