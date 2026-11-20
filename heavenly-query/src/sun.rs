//! An analytic position of the Sun, for when no ephemeris is loaded.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};

/// The astronomical unit, in metres (IAU 2012).
pub const AU_M: f64 = 149_597_870_700.0;

/// The Sun's radius in metres, as in NAIF's `pck00011.tpc`, so shadow
/// results compare directly with SPICE. (The IAU 2015 nominal value is
/// 695 700 km; the difference moves shadow edges by well under a second.)
pub const SUN_RADIUS_M: f64 = 696_000_000.0;

/// The geocentric Sun from a short analytic series (Meeus, *Astronomical
/// Algorithms*, ch. 25), referred to the J2000 ecliptic and equator, in
/// [`FrameId::GCRF`].
///
/// It is a [`Locator`] for the Sun that needs no kernels, which is enough
/// for day and night, Sun elevation, and eclipses of Earth satellites.
/// Against DE440 over 1950–2050 its direction is good to 32″ and its
/// distance to 7e-5. Eclipse edges then agree with SPICE to about 0.1 s
/// in low orbits and 2 s at geostationary distance. For anything more
/// precise, use the Sun from a JPL ephemeris (`heavenly::spice`).
///
/// The position is geometric: where the Sun is at `epoch`, not where it
/// appears after the 8.3 minutes its light takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalyticSun;

impl AnalyticSun {
    /// The Sun's geocentric position in GCRF, in metres.
    pub fn position_m(epoch: Epoch) -> Vec3 {
        const DEG: f64 = core::f64::consts::PI / 180.0;
        // Julian centuries of TT since J2000.
        let t = epoch.tt_seconds_since_j2000() / (36_525.0 * 86_400.0);
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
        let lon = (l0 + c) * DEG - precession_arcsec / 3_600.0 * DEG;
        // J2000 ecliptic → equator (IAU 2006 obliquity at J2000).
        let eps = 84_381.406 / 3_600.0 * DEG;
        let (x, y) = (libm::cos(lon), libm::sin(lon));
        Vec3::new(x, y * libm::cos(eps), y * libm::sin(eps)) * (r_au * AU_M)
    }
}

impl Locator for AnalyticSun {
    type Error = Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Error>
    where
        Error: From<R::Error>,
    {
        // Velocity by central difference: the series is smooth over minutes.
        const H: f64 = 60.0;
        let p = AnalyticSun::position_m(epoch);
        let v = (AnalyticSun::position_m(epoch.add_seconds(H)?)
            - AnalyticSun::position_m(epoch.add_seconds(-H)?))
            * (0.5 / H);
        Ok(StateVector::new(FrameId::GCRF, epoch, p, v).to_frame(frame, resolver)?)
    }
}
