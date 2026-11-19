//! IAU Working Group on Cartographic Coordinates and Rotational Elements
//! (WGCCRE) body orientation models.
//!
//! A model gives the right ascension `α₀` and declination `δ₀` of a body's
//! north pole and the angle `W` of its prime meridian, all relative to the
//! ICRF, as functions of TDB:
//!
//! ```text
//! α₀ = a₀ + a₁T + a₂T² + Σ αᵢ sin θᵢ
//! δ₀ = d₀ + d₁T + d₂T² + Σ δᵢ cos θᵢ
//! W  = w₀ + w₁d + w₂d² + Σ wᵢ sin θᵢ        θᵢ = θᵢ₀ + θᵢ₁T + θᵢ₂T²
//! ```
//!
//! Here `d` is TDB days and `T` is TDB Julian centuries since J2000. The
//! body-fixed axes are `M = R₃(W) R₁(90° − δ₀) R₃(90° + α₀)` applied to ICRF
//! axes. This is the same layout and convention as SPICE text PCK files, and
//! the constants below are copied from NAIF's `pck00011.tpc`.
//!
//! These models are cartographic conventions, not dynamical ephemerides. For
//! the Moon, [`MOON`] only approximates the DE440 lunar frames. Measured with
//! SPICE over 1970–2049, it differs from `MOON_ME` by up to 18″ (≈150 m at
//! the surface) and from `MOON_PA` by up to 116″ (≈980 m). Use a SPICE binary
//! PCK for metre-level lunar work.

// Constants are transcribed digit-for-digit from their published sources.
#![allow(clippy::inconsistent_digit_grouping, clippy::excessive_precision)]

use heavenly_core::{Epoch, Error, Rotation, Transform, Vec3};

use crate::mat::{self, DEG};

/// Coefficients of an IAU rotation model. Angles are in degrees, `T` in TDB
/// centuries and `d` in TDB days, exactly as in a SPICE text PCK.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IauRotationModel {
    /// Pole right ascension: `[a₀, a₁ (per century), a₂ (per century²)]`.
    pub pole_ra_deg: [f64; 3],
    /// Pole declination: `[d₀, d₁, d₂]`.
    pub pole_dec_deg: [f64; 3],
    /// Prime meridian: `[w₀, w₁ (per day), w₂ (per day²)]`.
    pub prime_meridian_deg: [f64; 3],
    /// Nutation–precession angles `θᵢ`: `[θ₀, θ₁ (per century), θ₂ (per century²)]`.
    pub angles_deg: &'static [[f64; 3]],
    /// Coefficients of `sin θᵢ` in `α₀`. May be shorter than `angles_deg`.
    pub ra_sin_deg: &'static [f64],
    /// Coefficients of `cos θᵢ` in `δ₀`.
    pub dec_cos_deg: &'static [f64],
    /// Coefficients of `sin θᵢ` in `W`.
    pub pm_sin_deg: &'static [f64],
}

/// Pole direction, prime-meridian angle and their rates at one epoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyOrientation {
    pub pole_ra_rad: f64,
    pub pole_dec_rad: f64,
    pub prime_meridian_rad: f64,
    pub pole_ra_rate_rad_s: f64,
    pub pole_dec_rate_rad_s: f64,
    pub prime_meridian_rate_rad_s: f64,
}

const DAY_S: f64 = 86_400.0;
const CENTURY_S: f64 = 36_525.0 * DAY_S;

impl IauRotationModel {
    /// Evaluates the model at `epoch`.
    pub fn orientation(&self, epoch: Epoch) -> BodyOrientation {
        let s = epoch.tdb_seconds_since_j2000();
        let (d, t) = (s / DAY_S, s / CENTURY_S);

        let poly = |c: [f64; 3], x: f64| c[0] + x * (c[1] + x * c[2]);
        let poly_rate = |c: [f64; 3], x: f64| c[1] + 2.0 * c[2] * x;

        let (mut ra, mut dec, mut w) = (
            poly(self.pole_ra_deg, t),
            poly(self.pole_dec_deg, t),
            poly(self.prime_meridian_deg, d),
        );
        // Rates: RA/Dec in degrees per century, W in degrees per day.
        let (mut ra_dot, mut dec_dot, mut w_dot) = (
            poly_rate(self.pole_ra_deg, t),
            poly_rate(self.pole_dec_deg, t),
            poly_rate(self.prime_meridian_deg, d),
        );

        for (i, th) in self.angles_deg.iter().enumerate() {
            let theta = poly(*th, t) * DEG;
            let theta_dot = poly_rate(*th, t) * DEG; // rad per century
            let (st, ct) = (libm::sin(theta), libm::cos(theta));
            if let Some(&a) = self.ra_sin_deg.get(i) {
                ra += a * st;
                ra_dot += a * ct * theta_dot;
            }
            if let Some(&a) = self.dec_cos_deg.get(i) {
                dec += a * ct;
                dec_dot -= a * st * theta_dot;
            }
            if let Some(&a) = self.pm_sin_deg.get(i) {
                w += a * st;
                w_dot += a * ct * theta_dot * (DAY_S / CENTURY_S);
            }
        }

        BodyOrientation {
            pole_ra_rad: ra * DEG,
            pole_dec_rad: dec * DEG,
            prime_meridian_rad: (w % 360.0) * DEG,
            pole_ra_rate_rad_s: ra_dot * DEG / CENTURY_S,
            pole_dec_rate_rad_s: dec_dot * DEG / CENTURY_S,
            prime_meridian_rate_rad_s: w_dot * DEG / DAY_S,
        }
    }

    /// Rotation matrix from ICRF axes to body-fixed axes: `p_body = M · p_icrf`.
    pub fn icrf_to_body_matrix(&self, epoch: Epoch) -> [[f64; 3]; 3] {
        let o = self.orientation(epoch);
        let half_pi = core::f64::consts::FRAC_PI_2;
        mat::rz(
            o.prime_meridian_rad,
            &mat::rx(
                half_pi - o.pole_dec_rad,
                &mat::rz(half_pi + o.pole_ra_rad, &mat::IDENTITY),
            ),
        )
    }

    /// The kinematic transform from the body-fixed frame to the body-centred
    /// ICRF-aligned frame. The two frames share an origin.
    pub fn body_fixed_to_icrf(&self, epoch: Epoch) -> Result<Transform, Error> {
        let o = self.orientation(epoch);
        let m = self.icrf_to_body_matrix(epoch);
        let rotation = Rotation::from_matrix(mat::transpose(&m)).ok_or(Error::NonFinite)?;
        // ω = α̇ ẑ − δ̇ n̂ + Ẇ p̂, with n̂ the node of the equator on the ICRF
        // xy-plane and p̂ the pole.
        let (sa, ca) = (libm::sin(o.pole_ra_rad), libm::cos(o.pole_ra_rad));
        let (sd, cd) = (libm::sin(o.pole_dec_rad), libm::cos(o.pole_dec_rad));
        let pole = Vec3::new(cd * ca, cd * sa, sd);
        let node = Vec3::new(-sa, ca, 0.0);
        let omega = Vec3::Z * o.pole_ra_rate_rad_s - node * o.pole_dec_rate_rad_s
            + pole * o.prime_meridian_rate_rad_s;
        Ok(Transform::new(rotation, Vec3::ZERO, Vec3::ZERO, omega))
    }
}

/// IAU Moon model (pck00011; Archinal et al. 2011).
pub const MOON: IauRotationModel = IauRotationModel {
    pole_ra_deg: [269.9949, 0.0031, 0.0],
    pole_dec_deg: [66.5392, 0.0130, 0.0],
    prime_meridian_deg: [38.3213, 13.176_358_15, -1.4e-12],
    angles_deg: &[
        [125.045, -1935.536_452_5, 0.0],
        [250.089, -3871.072_905_0, 0.0],
        [260.008, 475_263.332_872_5, 0.0],
        [176.625, 487_269.629_985_0, 0.0],
        [357.529, 35_999.050_957_5, 0.0],
        [311.589, 964_468.499_310_0, 0.0],
        [134.963, 477_198.869_325_0, 0.0],
        [276.617, 12_006.300_765_0, 0.0],
        [34.226, 63_863.513_242_5, 0.0],
        [15.134, -5806.609_357_5, 0.0],
        [119.743, 131.840_640_0, 0.0],
        [239.961, 6003.150_382_5, 0.0],
        [25.053, 473_327.796_420_0, 0.0],
    ],
    ra_sin_deg: &[
        -3.8787, -0.1204, 0.0700, -0.0172, 0.0, 0.0072, 0.0, 0.0, 0.0, -0.0052, 0.0, 0.0, 0.0043,
    ],
    dec_cos_deg: &[
        1.5419, 0.0239, -0.0278, 0.0068, 0.0, -0.0029, 0.0009, 0.0, 0.0, 0.0008, 0.0, 0.0, -0.0009,
    ],
    pm_sin_deg: &[
        3.5610, 0.1208, -0.0642, 0.0158, 0.0252, -0.0066, -0.0047, -0.0046, 0.0028, 0.0052, 0.0040,
        0.0019, -0.0044,
    ],
};

/// IAU 2015 Mars model (pck00011; Archinal et al. 2018).
pub const MARS: IauRotationModel = IauRotationModel {
    pole_ra_deg: [317.269_202, -0.109_275_47, 0.0],
    pole_dec_deg: [54.432_516, -0.058_271_05, 0.0],
    prime_meridian_deg: [176.049_863, 350.891_982_443_297, 0.0],
    angles_deg: &[
        [190.726_466_43, 15_917.108_186_95, 0.0],
        [21.468_924_70, 31_834.279_340_54, 0.0],
        [332.860_827_93, 19_139.896_947_42, 0.0],
        [394.932_564_37, 38_280.796_318_35, 0.0],
        [189.632_715_60, 41_215_158.184_200_50, 12.711_923_222],
        [121.468_936_64, 660.224_034_74, 0.0],
        [231.050_285_81, 660.991_235_40, 0.0],
        [251.373_140_25, 1320.501_452_45, 0.0],
        [217.986_359_55, 38_279.961_255_50, 0.0],
        [196.197_294_02, 19_139.836_286_08, 0.0],
        [198.991_226, 19_139.481_998_5, 0.0],
        [226.292_679, 38_280.851_128_1, 0.0],
        [249.663_391, 57_420.725_159_3, 0.0],
        [266.183_510, 76_560.636_795_0, 0.0],
        [79.398_797, 0.504_261_5, 0.0],
        [122.433_576, 19_139.940_747_6, 0.0],
        [43.058_401, 38_280.875_327_2, 0.0],
        [57.663_379, 57_420.751_720_5, 0.0],
        [79.476_401, 76_560.649_500_4, 0.0],
        [166.325_722, 0.504_261_5, 0.0],
        [129.071_773, 19_140.032_824_4, 0.0],
        [36.352_167, 38_281.047_359_1, 0.0],
        [56.668_646, 57_420.929_536_0, 0.0],
        [67.364_003, 76_560.255_221_5, 0.0],
        [104.792_680, 95_700.438_757_8, 0.0],
        [95.391_654, 0.504_261_5, 0.0],
    ],
    ra_sin_deg: &[
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.000_068, 0.000_238, 0.000_052,
        0.000_009, 0.419_057,
    ],
    dec_cos_deg: &[
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.000_051,
        0.000_141, 0.000_031, 0.000_005, 1.591_274,
    ],
    pm_sin_deg: &[
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.000_145, 0.000_157, 0.000_040, 0.000_001, 0.000_001, 0.584_542,
    ],
};

/// Registers `MOON_IAU → MOON_ICRF` and `MARS_IAU → MARS_ICRF` on a graph.
#[cfg(feature = "alloc")]
pub fn register(graph: &mut heavenly_core::FrameGraph) -> Result<(), Error> {
    use heavenly_core::FrameId;
    graph.add_edge(FrameId::MOON_IAU, FrameId::MOON_ICRF, |e| {
        MOON.body_fixed_to_icrf(e)
    })?;
    graph.add_edge(FrameId::MARS_IAU, FrameId::MARS_ICRF, |e| {
        MARS.body_fixed_to_icrf(e)
    })
}
