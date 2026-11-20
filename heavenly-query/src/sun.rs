//! Analytic positions of the Sun and the Moon, for when no ephemeris is
//! loaded.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};
use heavenly_orbit::analytic;

/// The astronomical unit, in metres (IAU 2012).
pub const AU_M: f64 = analytic::AU_M;

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
    /// The Sun's geocentric position in GCRF, in metres
    /// ([`heavenly_orbit::analytic::sun_position_m`]).
    pub fn position_m(epoch: Epoch) -> Vec3 {
        analytic::sun_position_m(epoch)
    }
}

/// The geocentric Moon from the low-precision series of Montenbruck & Gill,
/// in [`FrameId::GCRF`] ([`heavenly_orbit::analytic::moon_position_m`]).
///
/// Against DE440 over 1950–2050 its direction is good to 0.08° and its
/// distance to 0.13 %. That places the Moon in the sky and gives its
/// pull on a satellite, but it is not precise enough for occultations or
/// eclipses by the Moon (its disc is 0.5° across): use a JPL ephemeris for
/// those.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalyticMoon;

impl AnalyticMoon {
    /// The Moon's geocentric position in GCRF, in metres.
    pub fn position_m(epoch: Epoch) -> Vec3 {
        analytic::moon_position_m(epoch)
    }
}

// Velocity by central difference: both series are smooth over minutes.
fn locate<R: FrameResolver + ?Sized>(
    position: fn(Epoch) -> Vec3,
    epoch: Epoch,
    frame: FrameId,
    resolver: &R,
) -> Result<StateVector, Error>
where
    Error: From<R::Error>,
{
    const H: f64 = 60.0;
    let p = position(epoch);
    let v = (position(epoch.add_seconds(H)?) - position(epoch.add_seconds(-H)?)) * (0.5 / H);
    Ok(StateVector::new(FrameId::GCRF, epoch, p, v).to_frame(frame, resolver)?)
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
        locate(analytic::sun_position_m, epoch, frame, resolver)
    }
}

impl Locator for AnalyticMoon {
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
        locate(analytic::moon_position_m, epoch, frame, resolver)
    }
}
