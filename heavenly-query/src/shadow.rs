//! Sunlight and eclipses: how much of the Sun an object sees past a body.

use heavenly_core::{Epoch, FrameResolver, Locator, Vec3};

use crate::sun::SUN_RADIUS_M;
use crate::Occulter;

/// Which part of a body's shadow a point is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowKind {
    /// The whole solar disk is visible.
    Sunlit,
    /// Part of the disk is hidden.
    Penumbra,
    /// The disk is hidden completely.
    Umbra,
    /// The body is inside the disk and hides its middle (an annular
    /// eclipse), which happens beyond the tip of the umbra.
    Antumbra,
}

/// The Sun and an occulting body as seen from one point: their apparent
/// radii and the angle between their centres.
///
/// Everything else follows from these three angles: [`Shadow::kind`],
/// [`Shadow::sunlit_fraction`], and the two signed margins that
/// [`find_windows`](crate::find_windows) uses to find eclipse edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Apparent radius of the Sun, radians.
    pub sun_radius_rad: f64,
    /// Apparent radius of the body, radians.
    pub body_radius_rad: f64,
    /// Angle between the centres of the Sun and the body, radians.
    pub separation_rad: f64,
}

impl Shadow {
    /// Positive while any part of the Sun is hidden (penumbra, umbra or
    /// antumbra), negative in full sunlight; continuous, in radians.
    pub fn partial_margin(&self) -> f64 {
        self.sun_radius_rad + self.body_radius_rad - self.separation_rad
    }

    /// Positive while the whole Sun is hidden (umbra), negative otherwise;
    /// continuous, in radians.
    pub fn total_margin(&self) -> f64 {
        self.body_radius_rad - self.sun_radius_rad - self.separation_rad
    }

    pub fn kind(&self) -> ShadowKind {
        let (a, b, c) = (
            self.sun_radius_rad,
            self.body_radius_rad,
            self.separation_rad,
        );
        if c >= a + b {
            ShadowKind::Sunlit
        } else if c <= b - a {
            ShadowKind::Umbra
        } else if c <= a - b {
            ShadowKind::Antumbra
        } else {
            ShadowKind::Penumbra
        }
    }

    /// The fraction of the solar disk's area that is visible, from 0 (umbra)
    /// to 1 (sunlit). The disk is taken as uniformly bright (no limb
    /// darkening), and the two disks as flat circles, which is exact to the
    /// square of their angular sizes.
    pub fn sunlit_fraction(&self) -> f64 {
        let (a, b, c) = (
            self.sun_radius_rad,
            self.body_radius_rad,
            self.separation_rad,
        );
        match self.kind() {
            ShadowKind::Sunlit => 1.0,
            ShadowKind::Umbra => 0.0,
            ShadowKind::Antumbra => 1.0 - (b * b) / (a * a),
            ShadowKind::Penumbra => {
                // Area of the lens where the two disks overlap.
                let x = ((c * c + a * a - b * b) / (2.0 * c * a)).clamp(-1.0, 1.0);
                let y = ((c * c + b * b - a * a) / (2.0 * c * b)).clamp(-1.0, 1.0);
                let k = (-c + a + b) * (c + a - b) * (c - a + b) * (c + a + b);
                let lens =
                    a * a * libm::acos(x) + b * b * libm::acos(y) - 0.5 * libm::sqrt(k.max(0.0));
                (1.0 - lens / (core::f64::consts::PI * a * a)).clamp(0.0, 1.0)
            }
        }
    }
}

impl Occulter {
    /// The shadow geometry at a point, given the point and the Sun in this
    /// body's body-fixed frame (metres).
    ///
    /// An oblate body is handled as in [`Occulter::clearance_m`]: the polar
    /// axis is stretched until the body is a sphere. That maps straight
    /// lines to straight lines, so which rays towards the Sun the body
    /// blocks is exact; only the Sun's apparent radius picks up an error of
    /// the order of the flattening (≈1″ for the Earth).
    pub fn shadow_at(&self, point: Vec3, sun: Vec3) -> Shadow {
        let k = self.equatorial_radius_m / self.polar_radius_m;
        let scale = |v: Vec3| Vec3::new(v.x, v.y, v.z * k);
        let (p, s) = (scale(point), scale(sun));
        let to_sun = s - p;
        let to_body = -p;
        let angle = |r: f64, d: f64| libm::asin((r / d).min(1.0));
        Shadow {
            sun_radius_rad: angle(SUN_RADIUS_M, to_sun.norm()),
            body_radius_rad: angle(self.equatorial_radius_m, to_body.norm()),
            // atan2 of |a×b| and a·b keeps small angles precise.
            separation_rad: libm::atan2(to_sun.cross(to_body).norm(), to_sun.dot(to_body)),
        }
    }

    /// The shadow geometry for an object at `epoch`, with the Sun given by
    /// any [`Locator`]: [`AnalyticSun`](crate::AnalyticSun), or a JPL
    /// ephemeris through `heavenly::spice`. Both positions are geometric (at
    /// the same instant).
    pub fn shadow<T, S, R, E>(
        &self,
        target: &T,
        sun: &S,
        epoch: Epoch,
        resolver: &R,
    ) -> Result<Shadow, E>
    where
        T: Locator + ?Sized,
        S: Locator + ?Sized,
        R: FrameResolver + ?Sized,
        T::Error: From<R::Error>,
        S::Error: From<R::Error>,
        E: From<T::Error> + From<S::Error>,
    {
        let p = target
            .state_at(epoch, self.body_fixed, resolver)?
            .position_m;
        let s = sun.state_at(epoch, self.body_fixed, resolver)?.position_m;
        Ok(self.shadow_at(p, s))
    }
}

/// Eclipse intervals found by [`eclipses`].
#[cfg(feature = "alloc")]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Eclipses {
    /// Intervals during which any part of the Sun is hidden. Each contains
    /// the umbral intervals that fall in it.
    pub partial: alloc::vec::Vec<crate::Window>,
    /// Intervals during which the whole Sun is hidden.
    pub total: alloc::vec::Vec<crate::Window>,
}

/// Finds when `target` is in `body`'s shadow between `start` and `end`.
///
/// `step_s` must be shorter than the shortest eclipse to be found (see
/// [`find_windows`](crate::find_windows)); edges are refined to
/// `tolerance_s`.
#[cfg(feature = "alloc")]
#[allow(clippy::too_many_arguments)]
pub fn eclipses<T, S, R, E>(
    body: &Occulter,
    target: &T,
    sun: &S,
    start: Epoch,
    end: Epoch,
    step_s: f64,
    tolerance_s: f64,
    resolver: &R,
) -> Result<Eclipses, E>
where
    T: Locator + ?Sized,
    S: Locator + ?Sized,
    R: FrameResolver + ?Sized,
    T::Error: From<R::Error>,
    S::Error: From<R::Error>,
    E: From<T::Error> + From<S::Error> + From<heavenly_core::Error>,
{
    let shadow = |t: Epoch| body.shadow::<T, S, R, E>(target, sun, t, resolver);
    Ok(Eclipses {
        partial: crate::find_windows(start, end, step_s, tolerance_s, |t| {
            shadow(t).map(|s| s.partial_margin())
        })?,
        total: crate::find_windows(start, end, step_s, tolerance_s, |t| {
            shadow(t).map(|s| s.total_margin())
        })?,
    })
}
