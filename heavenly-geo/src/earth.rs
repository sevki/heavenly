//! Earth orientation: the rotation between the terrestrial frame
//! ([`FrameId::ITRF`](heavenly_core::FrameId::ITRF)) and the geocentric celestial frame
//! ([`FrameId::GCRF`](heavenly_core::FrameId::GCRF)).
//!
//! # Model
//!
//! This uses the IERS 2010 CIO-based procedure, following SOFA's `c2t06a`,
//! with a truncated nutation series:
//!
//! * frame bias + precession: IAU 2006 Fukushima–Williams angles (full);
//! * nutation: the largest terms of IAU 2000B;
//! * CIO locator `s`: IAU 2006 polynomial plus its two largest periodic terms;
//! * Earth Rotation Angle: IAU 2000, from UT1;
//! * polar motion: from caller-supplied [`EarthOrientation`] parameters,
//!   including the TIO locator `s′`.
//!
//! Against SOFA `c2t06a` (full IAU 2006/2000A) the rotation agrees to better
//! than [`MODEL_ACCURACY_RAD`] (15 mas, ≈0.5 m at the Earth's surface) over
//! 1980–2050, given the same EOP. The tests check this against committed
//! reference values; the measured worst case is ≈8 mas. Without real EOP (polar motion of up
//! to ~0.5″ and UT1 − UTC of up to 0.9 s) errors grow to ~15 m and ~400 m
//! respectively. Obtain EOP from IERS Bulletin A / C04 for anything better.
//!
//! Angular velocity is the Earth-rotation rate about the Celestial
//! Intermediate Pole. The far smaller precession–nutation and polar-motion
//! rates (< 1e-11 rad/s) are neglected.

// Constants are transcribed digit-for-digit from their published sources.
#![allow(clippy::inconsistent_digit_grouping, clippy::excessive_precision)]

use heavenly_core::{Epoch, Error, Rotation, Transform, Vec3};

use crate::mat::{self, Mat3, ARCSEC, TAU};

/// Upper bound on the angular difference between this model and SOFA
/// `c2t06a`, in radians (15 milliarcseconds), for 1980–2050.
pub const MODEL_ACCURACY_RAD: f64 = 15e-3 * ARCSEC;

/// Earth rotation rate in radians per second of UT1 (IAU 2000 ERA rate).
pub const EARTH_ROTATION_RATE_RAD_S: f64 = TAU * 1.002_737_811_911_354_48 / 86_400.0;

/// Earth orientation parameters (EOP) at one epoch, as published by IERS.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EarthOrientation {
    /// UT1 − UTC in seconds.
    pub ut1_minus_utc_s: f64,
    /// Polar-motion x coordinate of the CIP, in radians.
    pub xp_rad: f64,
    /// Polar-motion y coordinate of the CIP, in radians.
    pub yp_rad: f64,
}

impl EarthOrientation {
    /// All parameters zero, which gives the ~400 m accuracy described in the
    /// [module documentation](self).
    pub const ZERO: EarthOrientation = EarthOrientation {
        ut1_minus_utc_s: 0.0,
        xp_rad: 0.0,
        yp_rad: 0.0,
    };

    /// Builds EOP from the units IERS bulletins use: seconds and arcseconds.
    pub fn from_bulletin(ut1_minus_utc_s: f64, xp_arcsec: f64, yp_arcsec: f64) -> EarthOrientation {
        EarthOrientation {
            ut1_minus_utc_s,
            xp_rad: xp_arcsec * ARCSEC,
            yp_rad: yp_arcsec * ARCSEC,
        }
    }
}

/// Supplies EOP for an epoch, for example by interpolating an IERS series
/// ([`crate::eop::EopTable`] does that for the standard IERS files).
pub trait EopProvider {
    fn earth_orientation(&self, epoch: Epoch) -> Result<EarthOrientation, Error>;
}

/// A constant set of EOP, adequate over days to weeks.
impl EopProvider for EarthOrientation {
    fn earth_orientation(&self, _: Epoch) -> Result<EarthOrientation, Error> {
        Ok(*self)
    }
}

/// The rotation matrix from GCRS to ITRS (SOFA's `rc2t`): `p_itrs = M · p_gcrs`.
pub fn gcrs_to_itrs_matrix(epoch: Epoch, eop: &EarthOrientation) -> Result<[[f64; 3]; 3], Error> {
    Ok(compute(epoch, eop)?.c2t)
}

/// The kinematic transform from [`FrameId::ITRF`](heavenly_core::FrameId::ITRF)
/// to [`FrameId::GCRF`](heavenly_core::FrameId::GCRF).
pub fn itrf_to_gcrf(epoch: Epoch, eop: &EarthOrientation) -> Result<Transform, Error> {
    let o = compute(epoch, eop)?;
    let rotation = Rotation::from_matrix(mat::transpose(&o.c2t)).ok_or(Error::NonFinite)?;
    Ok(Transform::new(
        rotation,
        Vec3::ZERO,
        Vec3::ZERO,
        o.cip_gcrs * EARTH_ROTATION_RATE_RAD_S,
    ))
}

/// The Earth Rotation Angle in radians, in `[0, 2π)`.
pub fn earth_rotation_angle(epoch: Epoch, eop: &EarthOrientation) -> Result<f64, Error> {
    Ok(era(ut1_days(epoch, eop)?))
}

/// Greenwich Mean Sidereal Time (IAU 1982) in radians, in `[0, 2π)`, and
/// its rate in radians per second.
///
/// This is the angle between the [`FrameId::TEME`](heavenly_core::FrameId::TEME)
/// equinox and the Greenwich meridian, as defined for SGP4 (Vallado et al.
/// 2006, AIAA 2006-6753, appendix C). It is not used for GCRF ↔ ITRF, which
/// uses the Earth Rotation Angle.
pub fn gmst82(epoch: Epoch, eop: &EarthOrientation) -> Result<(f64, f64), Error> {
    const DAY_S: f64 = 86_400.0;
    let (whole, frac) = ut1_days(epoch, eop)?;
    let t = (whole + frac) / 36_525.0;
    let g = 67_310.548_41 + (8_640_184.812_866 + (0.093_104 + (-6.2e-6) * t) * t) * t;
    let dg = 8_640_184.812_866 + (0.093_104 * 2.0 + (-6.2e-6 * 3.0) * t) * t;
    // JD 2451545.0 is an integer, so the JD fraction of UT1 is `frac`. The
    // whole days are left out of the sum to keep precision.
    let theta = libm::fmod(frac + libm::fmod(g / DAY_S, 1.0) + 2.0, 1.0) * TAU;
    let theta_dot = (1.0 + dg / (DAY_S * 36_525.0)) * TAU / DAY_S;
    Ok((theta, theta_dot))
}

/// The kinematic transform from [`FrameId::TEME`](heavenly_core::FrameId::TEME)
/// (the frame of SGP4 output) to [`FrameId::ITRF`](heavenly_core::FrameId::ITRF):
/// `R = W · R3(GMST82)`, with `W` the polar-motion matrix (without the tiny
/// TIO locator `s′`, as in Vallado et al.).
///
/// The Earth's rotation rate is applied; the precession and nutation of
/// TEME itself are neglected, as in the reference implementation.
pub fn teme_to_itrf(epoch: Epoch, eop: &EarthOrientation) -> Result<Transform, Error> {
    if !(eop.xp_rad.is_finite() && eop.yp_rad.is_finite()) {
        return Err(Error::NonFinite);
    }
    let (theta, theta_dot) = gmst82(epoch, eop)?;
    let w = mat::rx(-eop.yp_rad, &mat::ry(-eop.xp_rad, &mat::IDENTITY));
    let m = mat::mul(&w, &mat::rz(theta, &mat::IDENTITY));
    let rotation = Rotation::from_matrix(m).ok_or(Error::NonFinite)?;
    // A point fixed in TEME turns westward, at −θ̇ about the pole (W ẑ).
    let pole = Vec3::new(w[0][2], w[1][2], w[2][2]);
    Ok(Transform::new(
        rotation,
        Vec3::ZERO,
        Vec3::ZERO,
        pole * -theta_dot,
    ))
}

/// Registers the ITRF → GCRF and TEME → ITRF edges on a frame graph.
#[cfg(feature = "alloc")]
pub fn register<P>(graph: &mut heavenly_core::FrameGraph, eop: P) -> Result<(), Error>
where
    P: EopProvider + Send + Sync + 'static,
{
    use alloc::sync::Arc;
    use heavenly_core::FrameId;
    let eop = Arc::new(eop);
    let e = Arc::clone(&eop);
    graph.add_edge(FrameId::ITRF, FrameId::GCRF, move |epoch| {
        itrf_to_gcrf(epoch, &e.earth_orientation(epoch)?)
    })?;
    graph.add_edge(FrameId::TEME, FrameId::ITRF, move |epoch| {
        teme_to_itrf(epoch, &eop.earth_orientation(epoch)?)
    })
}

struct Orientation {
    c2t: Mat3,
    /// Unit vector of the CIP in GCRS.
    cip_gcrs: Vec3,
}

fn compute(epoch: Epoch, eop: &EarthOrientation) -> Result<Orientation, Error> {
    if !(eop.ut1_minus_utc_s.is_finite() && eop.xp_rad.is_finite() && eop.yp_rad.is_finite()) {
        return Err(Error::NonFinite);
    }
    // Julian centuries of TT since J2000 (TT is used in place of TDB, as in SOFA).
    let t = epoch.tt_seconds_since_j2000() / (36_525.0 * 86_400.0);

    let (gamb, phib, psib, epsa) = fukushima_williams(t);
    let (dpsi, deps) = nutation(t);
    // NPB matrix (iauFw2m): Rx(−ε)·Rz(−ψ)·Rx(φ)·Rz(γ).
    let npb = mat::rx(
        -(epsa + deps),
        &mat::rz(
            -(psib + dpsi),
            &mat::rx(phib, &mat::rz(gamb, &mat::IDENTITY)),
        ),
    );
    let (x, y) = (npb[2][0], npb[2][1]);
    let s = cio_locator(t, x, y);

    // Celestial-to-intermediate matrix (iauC2ixys).
    let r2 = x * x + y * y;
    let e = if r2 > 0.0 { libm::atan2(y, x) } else { 0.0 };
    let d = libm::atan(libm::sqrt(r2 / (1.0 - r2)));
    let c2i = mat::rz(-(e + s), &mat::ry(d, &mat::rz(e, &mat::IDENTITY)));

    // Polar motion (iauPom00), with s′ = −47 µas · t.
    let sp = -47e-6 * ARCSEC * t;
    let pom = mat::rx(
        -eop.yp_rad,
        &mat::ry(-eop.xp_rad, &mat::rz(sp, &mat::IDENTITY)),
    );

    let theta = era(ut1_days(epoch, eop)?);
    let c2t = mat::mul(&pom, &mat::rz(theta, &c2i));
    Ok(Orientation {
        c2t,
        cip_gcrs: Vec3::new(x, y, libm::sqrt(1.0 - r2)),
    })
}

/// Days of UT1 since J2000.0 (JD 2451545.0 UT1), split as (whole, fraction).
fn ut1_days(epoch: Epoch, eop: &EarthOrientation) -> Result<(f64, f64), Error> {
    const NS: i128 = 1_000_000_000;
    const DAY: i128 = 86_400 * NS;
    let dat = epoch.tai_minus_utc_seconds()? as i128;
    let dut1 = libm::round(eop.ut1_minus_utc_s * 1e9) as i128;
    let ut1_ns = epoch.tt_nanoseconds_since_j2000() - 32_184_000_000 - dat * NS + dut1;
    let whole = ut1_ns.div_euclid(DAY);
    let frac = ut1_ns.rem_euclid(DAY) as f64 / DAY as f64;
    Ok((whole as f64, frac))
}

/// IAU 2000 Earth Rotation Angle (iauEra00).
fn era((whole, frac): (f64, f64)) -> f64 {
    let du = whole + frac;
    let theta = TAU * (frac + 0.779_057_273_264_0 + 0.002_737_811_911_354_48 * du);
    let r = libm::fmod(theta, TAU);
    if r < 0.0 {
        r + TAU
    } else {
        r
    }
}

/// IAU 2006 Fukushima–Williams bias-precession angles and mean obliquity, in
/// radians (iauPfw06).
fn fukushima_williams(t: f64) -> (f64, f64, f64, f64) {
    let poly = |c: [f64; 6]| {
        (c[0] + t * (c[1] + t * (c[2] + t * (c[3] + t * (c[4] + t * c[5]))))) * ARCSEC
    };
    let gamb = poly([
        -0.052928,
        10.556378,
        0.4932044,
        -0.00031238,
        -0.000002788,
        0.0000000260,
    ]);
    let phib = poly([
        84381.412819,
        -46.811016,
        0.0511268,
        0.00053289,
        -0.000000440,
        -0.0000000176,
    ]);
    let psib = poly([
        -0.041775,
        5038.481484,
        1.5584175,
        -0.00018522,
        -0.000026452,
        -0.0000000148,
    ]);
    let epsa = poly([
        84381.406,
        -46.836769,
        -0.0001831,
        0.00200340,
        -0.000000576,
        -0.0000000434,
    ]);
    (gamb, phib, psib, epsa)
}

/// Largest terms of the IAU 2000B luni-solar nutation series (iauNut00b).
///
/// Columns: multipliers of l, l′, F, D, Ω; then, in units of 0.1 µas,
/// ψ sin, ψ sin·t, ψ cos, ε cos, ε cos·t, ε sin.
#[rustfmt::skip]
const NUTATION: [([i8; 5], [f64; 6]); 20] = [
    ([ 0, 0, 0, 0, 1], [-172064161.0, -174666.0,  33386.0, 92052331.0,  9086.0,  15377.0]),
    ([ 0, 0, 2,-2, 2], [ -13170906.0,   -1675.0, -13696.0,  5730336.0, -3015.0,  -4587.0]),
    ([ 0, 0, 2, 0, 2], [  -2276413.0,    -234.0,   2796.0,   978459.0,  -485.0,   1374.0]),
    ([ 0, 0, 0, 0, 2], [   2074554.0,     207.0,   -698.0,  -897492.0,   470.0,   -291.0]),
    ([ 0, 1, 0, 0, 0], [   1475877.0,   -3633.0,  11817.0,    73871.0,  -184.0,  -1924.0]),
    ([ 0, 1, 2,-2, 2], [   -516821.0,    1226.0,   -524.0,   224386.0,  -677.0,   -174.0]),
    ([ 1, 0, 0, 0, 0], [    711159.0,      73.0,   -872.0,    -6750.0,     0.0,    358.0]),
    ([ 0, 0, 2, 0, 1], [   -387298.0,    -367.0,    380.0,   200728.0,    18.0,    318.0]),
    ([ 1, 0, 2, 0, 2], [   -301461.0,     -36.0,    816.0,   129025.0,   -63.0,    367.0]),
    ([ 0,-1, 2,-2, 2], [    215829.0,    -494.0,    111.0,   -95929.0,   299.0,    132.0]),
    ([ 0, 0, 2,-2, 1], [    128227.0,     137.0,    181.0,   -68982.0,    -9.0,     39.0]),
    ([-1, 0, 2, 0, 2], [    123457.0,      11.0,     19.0,   -53311.0,    32.0,     -4.0]),
    ([-1, 0, 0, 2, 0], [    156994.0,      10.0,   -168.0,    -1235.0,     0.0,     82.0]),
    ([ 1, 0, 0, 0, 1], [     63110.0,      63.0,     27.0,   -33228.0,     0.0,     -9.0]),
    ([-1, 0, 0, 0, 1], [    -57976.0,     -63.0,   -189.0,    31429.0,     0.0,    -75.0]),
    ([-1, 0, 2, 2, 2], [    -59641.0,     -11.0,    149.0,    25543.0,   -11.0,     66.0]),
    ([ 1, 0, 2, 0, 1], [    -51613.0,     -42.0,    129.0,    26366.0,     0.0,     78.0]),
    ([-2, 0, 2, 0, 1], [     45893.0,      50.0,     31.0,   -24236.0,   -10.0,     20.0]),
    ([ 0, 0, 0, 2, 0], [     63384.0,      11.0,   -150.0,    -1220.0,     0.0,     29.0]),
    ([ 0, 0, 2, 2, 2], [    -38571.0,      -1.0,    158.0,    16452.0,   -11.0,     68.0]),
];

/// Nutation in longitude and obliquity, in radians.
fn nutation(t: f64) -> (f64, f64) {
    const TURN_AS: f64 = 1_296_000.0;
    let arg = |c0: f64, c1: f64| ((c0 + c1 * t) % TURN_AS) * ARCSEC;
    // Delaunay arguments (IERS 2003, linear terms as in iauNut00b).
    let fa = [
        arg(485_868.249_036, 1_717_915_923.217_8),
        arg(1_287_104.793_05, 129_596_581.048_1),
        arg(335_779.526_232, 1_739_527_262.847_8),
        arg(1_072_260.703_69, 1_602_961_601.209_0),
        arg(450_160.398_036, -6_962_890.543_1),
    ];
    let (mut dp, mut de) = (0.0, 0.0);
    for (n, c) in NUTATION.iter().rev() {
        let a: f64 = n.iter().zip(fa.iter()).map(|(&k, &f)| k as f64 * f).sum();
        let (sa, ca) = (libm::sin(a), libm::cos(a));
        dp += (c[0] + c[1] * t) * sa + c[2] * ca;
        de += (c[3] + c[4] * t) * ca + c[5] * sa;
    }
    let u2r = ARCSEC / 1e7;
    // Fixed offsets standing in for the planetary terms (iauNut00b).
    (dp * u2r - 0.135e-3 * ARCSEC, de * u2r + 0.388e-3 * ARCSEC)
}

/// CIO locator `s` (IAU 2006), keeping the polynomial and two largest
/// periodic terms.
fn cio_locator(t: f64, x: f64, y: f64) -> f64 {
    let om = (450_160.398_036 - 6_962_890.543_1 * t) * ARCSEC;
    let poly = 94.0 + t * (3808.65 + t * (-122.68 + t * (-72574.11 + t * (27.98 + t * 15.62))));
    let periodic = -2640.73 * libm::sin(om) - 63.53 * libm::sin(2.0 * om);
    (poly + periodic) * 1e-6 * ARCSEC - 0.5 * x * y
}
