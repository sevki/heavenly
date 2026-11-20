//! Look angles, line of sight and visibility windows.

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, Vec3};

/// Azimuth, elevation and range of a target seen from a site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookAngles {
    /// Clockwise from north, in `[0, 2π)`.
    pub azimuth_rad: f64,
    /// Above the local horizontal plane, in `[−π/2, π/2]`.
    pub elevation_rad: f64,
    pub range_m: f64,
    /// Rate of change of range, in m/s (positive when receding).
    pub range_rate_m_s: f64,
}

/// Look angles from the origin of `site_frame`, which must be an
/// east-north-up frame such as `Geodetic::enu_to_body_fixed` or a
/// `heavenly-id` geodetic mount. These are geometric angles: no refraction,
/// no light time.
pub fn look_angles<T, R, E>(
    site_frame: FrameId,
    target: &T,
    epoch: Epoch,
    resolver: &R,
) -> Result<LookAngles, E>
where
    T: Locator + ?Sized,
    R: FrameResolver + ?Sized,
    T::Error: From<R::Error>,
    E: From<T::Error> + From<Error>,
{
    let s = target.state_at(epoch, site_frame, resolver)?;
    let p = s.position_m;
    let range_m = p.norm();
    if range_m == 0.0 {
        return Err(Error::InvalidInput("target is at the site").into());
    }
    let mut azimuth_rad = libm::atan2(p.x, p.y);
    if azimuth_rad < 0.0 {
        azimuth_rad += core::f64::consts::TAU;
    }
    Ok(LookAngles {
        azimuth_rad,
        elevation_rad: libm::asin((p.z / range_m).clamp(-1.0, 1.0)),
        range_m,
        range_rate_m_s: p.dot(s.velocity_m_s) / range_m,
    })
}

/// A body that can block a line of sight: an oblate ellipsoid with its
/// symmetry axis along the z axis of a body-fixed frame centred on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occulter {
    pub body_fixed: FrameId,
    pub equatorial_radius_m: f64,
    pub polar_radius_m: f64,
}

impl Occulter {
    /// The Earth (WGS84) in the ITRF.
    pub const EARTH: Occulter = Occulter {
        body_fixed: FrameId::ITRF,
        equatorial_radius_m: 6_378_137.0,
        polar_radius_m: 6_356_752.314_245,
    };
    /// The Moon (IAU mean sphere) in `MOON_ME`.
    pub const MOON: Occulter = Occulter {
        body_fixed: FrameId::MOON_ME,
        equatorial_radius_m: 1_737_400.0,
        polar_radius_m: 1_737_400.0,
    };

    /// How far the straight segment between two points (in this occulter's
    /// body-fixed frame) clears the body.
    ///
    /// The value is the closest approach of the segment to the body's
    /// centre, minus the radius, measured after scaling the polar axis so
    /// the body becomes a sphere. It is positive when the line of sight is
    /// clear and negative when it is blocked. It is exact in sign. Its size
    /// equals the true clearance for a sphere, and is within the flattening
    /// (0.3 % for the Earth) of it otherwise. Because it is continuous, it
    /// works as a [`find_windows`] function.
    pub fn clearance_m(&self, a: Vec3, b: Vec3) -> f64 {
        let k = self.equatorial_radius_m / self.polar_radius_m;
        let (a, b) = (Vec3::new(a.x, a.y, a.z * k), Vec3::new(b.x, b.y, b.z * k));
        let d = b - a;
        let len2 = d.norm_squared();
        let t = if len2 == 0.0 {
            0.0
        } else {
            (-a.dot(d) / len2).clamp(0.0, 1.0)
        };
        (a + d * t).norm() - self.equatorial_radius_m
    }

    /// The clearance of the line of sight between two objects at `epoch`
    /// (geometric: both positions at the same instant). Endpoints should
    /// not be inside the body; points on its surface (sites at zero height)
    /// are fine.
    pub fn line_of_sight<A, B, R, E>(
        &self,
        a: &A,
        b: &B,
        epoch: Epoch,
        resolver: &R,
    ) -> Result<f64, E>
    where
        A: Locator + ?Sized,
        B: Locator + ?Sized,
        R: FrameResolver + ?Sized,
        A::Error: From<R::Error>,
        B::Error: From<R::Error>,
        E: From<A::Error> + From<B::Error>,
    {
        let pa = a.state_at(epoch, self.body_fixed, resolver)?.position_m;
        let pb = b.state_at(epoch, self.body_fixed, resolver)?.position_m;
        Ok(self.clearance_m(pa, pb))
    }
}

/// A time interval during which a condition holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub start: Epoch,
    pub end: Epoch,
}

impl Window {
    pub fn duration_s(&self) -> f64 {
        self.end.seconds_since(self.start)
    }
}

/// Finds the intervals in `[start, end]` where `f(epoch) > 0`.
///
/// `f` is sampled every `step_s` seconds, and each sign change is refined
/// by bisection to within `tolerance_s`. Typical choices for `f` are
/// elevation minus a mask angle, or a line-of-sight clearance.
///
/// Windows shorter than `step_s` can be missed, so choose a step well under
/// the shortest event of interest. For a low-orbit pass, that is about a
/// minute or less. Windows still open at `start` or `end` are clipped to
/// them.
#[cfg(feature = "alloc")]
pub fn find_windows<E>(
    start: Epoch,
    end: Epoch,
    step_s: f64,
    tolerance_s: f64,
    mut f: impl FnMut(Epoch) -> Result<f64, E>,
) -> Result<Vec<Window>, E>
where
    E: From<Error>,
{
    if step_s.is_nan() || step_s <= 0.0 || tolerance_s.is_nan() || tolerance_s <= 0.0 {
        return Err(Error::InvalidInput("step and tolerance must be positive").into());
    }
    let span = end.seconds_since(start);
    if span < 0.0 {
        return Err(Error::InvalidInput("end is before start").into());
    }
    let mut windows = Vec::new();
    let mut t0 = start;
    let mut v0 = f(t0)?;
    let mut open = if v0 > 0.0 { Some(start) } else { None };
    let mut s = 0.0;
    while s < span {
        let next_s = (s + step_s).min(span);
        let t1 = start.add_seconds(next_s)?;
        let v1 = f(t1)?;
        if (v0 > 0.0) != (v1 > 0.0) {
            // Bisect for the crossing.
            let (mut lo, mut hi) = (t0, t1);
            let rising = v1 > 0.0;
            while hi.seconds_since(lo) > tolerance_s {
                let mid = lo.add_seconds(hi.seconds_since(lo) / 2.0)?;
                if (f(mid)? > 0.0) == rising {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let crossing = if rising { hi } else { lo };
            if rising {
                open = Some(crossing);
            } else if let Some(w) = open.take() {
                windows.push(Window {
                    start: w,
                    end: crossing,
                });
            }
        }
        t0 = t1;
        v0 = v1;
        s = next_s;
    }
    if let Some(w) = open {
        windows.push(Window { start: w, end });
    }
    Ok(windows)
}
