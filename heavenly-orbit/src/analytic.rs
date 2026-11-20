//! Short analytic series for the bodies a perturbed Earth orbit needs, with
//! no ephemeris or Earth orientation data: the Sun, the Moon, and the
//! direction of the Earth's rotation axis.
//!
//! Each is checked against the reference it approximates in the tests (DE440
//! through SPICE; the IAU 2006/2000A pole through SOFA), and its measured
//! accuracy is in its documentation. They are good enough for the forces
//! on a satellite: with all three in place of DE440 and the full IAU pole,
//! a low orbit moves by 0.35 m in a day and a geostationary one by 300 m in
//! a month. For more, use JPL ephemerides (`heavenly::spice`) and pass your
//! own positions to [`crate::ThirdBody`].

use heavenly_core::{Epoch, Vec3};

const DEG: f64 = core::f64::consts::PI / 180.0;
const ARCSEC: f64 = DEG / 3_600.0;

/// The astronomical unit, in metres (IAU 2012).
pub const AU_M: f64 = 149_597_870_700.0;

/// IAU 2006 obliquity of the ecliptic at J2000.
const OBLIQUITY_J2000: f64 = 84_381.406 * ARCSEC;

/// Julian centuries of TT since J2000.
fn centuries(epoch: Epoch) -> f64 {
    epoch.tt_seconds_since_j2000() / (36_525.0 * 86_400.0)
}

/// J2000 ecliptic → GCRF (J2000 equator).
fn ecliptic_to_equator(v: Vec3) -> Vec3 {
    let (s, c) = (libm::sin(OBLIQUITY_J2000), libm::cos(OBLIQUITY_J2000));
    Vec3::new(v.x, c * v.y - s * v.z, s * v.y + c * v.z)
}

/// The Sun's geocentric position in GCRF, in metres, from a short series
/// (Meeus, *Astronomical Algorithms*, ch. 25) referred to the J2000 ecliptic
/// and equator.
///
/// Against DE440 over 1950–2050 its direction is good to 32″ and its
/// distance to 7e-5. The position is geometric (where the Sun is, not where
/// it appears). `heavenly::query::AnalyticSun` is this as a `Locator`.
pub fn sun_position_m(epoch: Epoch) -> Vec3 {
    let t = centuries(epoch);
    let l0 = 280.466_46 + 36_000.769_83 * t + 0.000_303_2 * t * t;
    let m = (357.529_11 + 35_999.050_29 * t - 0.000_153_7 * t * t) * DEG;
    let c = (1.914_602 - 0.004_817 * t - 0.000_014 * t * t) * libm::sin(m)
        + (0.019_993 - 0.000_101 * t) * libm::sin(2.0 * m)
        + 0.000_289 * libm::sin(3.0 * m);
    let e = 0.016_708_634 - 0.000_042_037 * t - 0.000_000_126_7 * t * t;
    let nu = m + c * DEG;
    let r_au = 1.000_001_018 * (1.0 - e * e) / (1.0 + e * libm::cos(nu));
    // True longitude, mean equinox of date → J2000, removing the general
    // precession in longitude.
    let precession_arcsec = 5_029.096_6 * t + 1.111_13 * t * t;
    let lon = (l0 + c) * DEG - precession_arcsec * ARCSEC;
    let (x, y) = (libm::cos(lon), libm::sin(lon));
    ecliptic_to_equator(Vec3::new(x, y, 0.0)) * (r_au * AU_M)
}

/// The Moon's geocentric position in GCRF, in metres, from the
/// low-precision series of Montenbruck & Gill (*Satellite Orbits*, §3.3.2)
/// referred to the J2000 ecliptic and equator.
///
/// Against DE440 over 1950–2050 its direction is good to 0.08° and its
/// distance to 0.13 % (the tests measure both). That is ample for the
/// Moon's pull on a satellite, but not for lunar eclipses or occultations:
/// use a JPL ephemeris for those.
pub fn moon_position_m(epoch: Epoch) -> Vec3 {
    let t = centuries(epoch);
    // Mean arguments (degrees). The −1.3972 T term refers the mean
    // longitude to the J2000 equinox.
    let l0 = 218.316_17 + 481_267.880_88 * t - 1.397_2 * t;
    let l = (134.962_92 + 477_198.867_53 * t) * DEG;
    let lp = (357.525_43 + 35_999.049_44 * t) * DEG;
    let f = (93.272_83 + 483_202.018_73 * t) * DEG;
    let d = (297.850_27 + 445_267.111_35 * t) * DEG;
    let s = libm::sin;
    let c = libm::cos;
    let dlon = 22_640.0 * s(l) + 769.0 * s(2.0 * l) - 4_586.0 * s(l - 2.0 * d)
        + 2_370.0 * s(2.0 * d)
        - 668.0 * s(lp)
        - 412.0 * s(2.0 * f)
        - 212.0 * s(2.0 * l - 2.0 * d)
        - 206.0 * s(l + lp - 2.0 * d)
        + 192.0 * s(l + 2.0 * d)
        - 165.0 * s(lp - 2.0 * d)
        + 148.0 * s(l - lp)
        - 125.0 * s(d)
        - 110.0 * s(l + lp)
        - 55.0 * s(2.0 * f - 2.0 * d);
    let lon = l0 * DEG + dlon * ARCSEC;
    let lat = (18_520.0 * s(f + dlon * ARCSEC + (412.0 * s(2.0 * f) + 541.0 * s(lp)) * ARCSEC)
        - 526.0 * s(f - 2.0 * d)
        + 44.0 * s(l + f - 2.0 * d)
        - 31.0 * s(-l + f - 2.0 * d)
        - 25.0 * s(-2.0 * l + f)
        - 23.0 * s(lp + f - 2.0 * d)
        + 21.0 * s(-l + f)
        + 11.0 * s(-lp + f - 2.0 * d))
        * ARCSEC;
    let r_km = 385_000.0
        - 20_905.0 * c(l)
        - 3_699.0 * c(2.0 * d - l)
        - 2_956.0 * c(2.0 * d)
        - 570.0 * c(2.0 * l)
        + 246.0 * c(2.0 * l - 2.0 * d)
        - 205.0 * c(lp - 2.0 * d)
        - 171.0 * c(l + 2.0 * d)
        - 152.0 * c(l + lp - 2.0 * d);
    let ecl = Vec3::new(c(lat) * c(lon), c(lat) * s(lon), s(lat));
    ecliptic_to_equator(ecl) * (r_km * 1e3)
}

/// The direction of the Earth's rotation axis (the Celestial Intermediate
/// Pole) in GCRF, as a unit vector.
///
/// IAU 2006 precession (the polynomial part of the CIP coordinates X, Y)
/// plus the five largest IAU 2000A nutation terms. Against the full IAU
/// 2006/2000A model (SOFA `xy06`) it is good to 0.17″ over 1950–2050.
/// The zonal gravity field and the co-rotating atmosphere are aligned with
/// it; ignoring precession altogether would tilt the field by 0.35° in 2026.
pub fn earth_pole(epoch: Epoch) -> Vec3 {
    let t = centuries(epoch);
    // Delaunay arguments (IERS Conventions 2010, eq. 5.43), radians.
    let lp = (357.529_109_18 * 3_600.0 + 129_596_581.048_1 * t) * ARCSEC;
    let f = (93.272_090_62 * 3_600.0 + 1_739_527_262.847_8 * t) * ARCSEC;
    let d = (297.850_195_47 * 3_600.0 + 1_602_961_601.209_0 * t) * ARCSEC;
    let om = (125.044_555_01 * 3_600.0 - 6_962_890.543_1 * t) * ARCSEC;
    let arg = 2.0 * f - 2.0 * d + 2.0 * om;
    // Nutation in longitude and obliquity: the five largest terms, arcsec.
    let dpsi = -17.206_416_1 * libm::sin(om)
        - 1.317_090_7 * libm::sin(arg)
        - 0.227_641_3 * libm::sin(2.0 * f + 2.0 * om)
        + 0.207_455_4 * libm::sin(2.0 * om)
        + 0.147_587_7 * libm::sin(lp);
    let deps = 9.205_233_1 * libm::cos(om)
        + 0.573_033_6 * libm::cos(arg)
        + 0.097_845_9 * libm::cos(2.0 * f + 2.0 * om)
        - 0.089_749_2 * libm::cos(2.0 * om);
    // CIP X, Y: IAU 2006 precession polynomials (IERS 2010, eq. 5.16, the
    // non-periodic part), plus nutation to first order.
    let x = (-0.016_617 + 2_004.191_898 * t - 0.429_782_9 * t * t - 0.198_618_34 * t * t * t
        + dpsi * libm::sin(OBLIQUITY_J2000))
        * ARCSEC;
    let y = (-0.006_951 - 0.025_896 * t - 22.407_274_7 * t * t + 0.001_900_59 * t * t * t + deps)
        * ARCSEC;
    Vec3::new(x, y, libm::sqrt(1.0 - x * x - y * y))
}
