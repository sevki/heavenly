use heavenly_core::{Error, Rotation, Transform, Vec3};

use crate::mat::DEG;
use crate::Ellipsoid;

/// A position given by geodetic (ellipsoid-normal) latitude, east-positive
/// longitude and height above a reference [`Ellipsoid`].
///
/// For bodies other than Earth this is *planetodetic* latitude. It is not
/// planetocentric latitude (the angle from the centre) and it is not the
/// west-positive "planetographic" longitude used in some older Mars products.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geodetic {
    pub latitude_rad: f64,
    pub longitude_rad: f64,
    pub height_m: f64,
}

impl Geodetic {
    /// Builds a position, checking that latitude is within ±90° and all
    /// values are finite. Longitude may be any finite angle.
    pub fn new(latitude_rad: f64, longitude_rad: f64, height_m: f64) -> Result<Geodetic, Error> {
        if !(latitude_rad.is_finite() && longitude_rad.is_finite() && height_m.is_finite()) {
            return Err(Error::NonFinite);
        }
        if latitude_rad.abs() > core::f64::consts::FRAC_PI_2 + 1e-15 {
            return Err(Error::InvalidInput("latitude must be within ±90°"));
        }
        Ok(Geodetic {
            latitude_rad,
            longitude_rad,
            height_m,
        })
    }

    /// Like [`Geodetic::new`] with angles in degrees.
    pub fn from_degrees(
        latitude_deg: f64,
        longitude_deg: f64,
        height_m: f64,
    ) -> Result<Geodetic, Error> {
        Geodetic::new(latitude_deg * DEG, longitude_deg * DEG, height_m)
    }

    /// Body-fixed Cartesian position in metres.
    pub fn to_cartesian(&self, e: &Ellipsoid) -> Vec3 {
        let (sp, cp) = (libm::sin(self.latitude_rad), libm::cos(self.latitude_rad));
        let (sl, cl) = (libm::sin(self.longitude_rad), libm::cos(self.longitude_rad));
        let e2 = e.eccentricity_squared();
        // Prime-vertical radius of curvature.
        let n = e.semi_major_axis_m / libm::sqrt(1.0 - e2 * sp * sp);
        Vec3::new(
            (n + self.height_m) * cp * cl,
            (n + self.height_m) * cp * sl,
            (n * (1.0 - e2) + self.height_m) * sp,
        )
    }

    /// Converts a body-fixed Cartesian position to geodetic coordinates
    /// with Heikkinen's closed-form solution.
    ///
    /// Longitude is returned in `(−π, π]`. The solution is exact to rounding
    /// for any point more than a few kilometres from the body's centre. It
    /// fails with [`Error::InvalidInput`] within ~`a·e²` of the centre, where
    /// geodetic coordinates are not unique.
    pub fn from_cartesian(p: Vec3, e: &Ellipsoid) -> Result<Geodetic, Error> {
        if !p.is_finite() {
            return Err(Error::NonFinite);
        }
        let a = e.semi_major_axis_m;
        let b = e.semi_minor_axis_m();
        let e2 = e.eccentricity_squared();
        let rho = libm::hypot(p.x, p.y);
        let longitude_rad = libm::atan2(p.y, p.x);

        if e2 == 0.0 {
            let r = p.norm();
            if r == 0.0 {
                return Err(Error::InvalidInput("position is at the centre of the body"));
            }
            return Ok(Geodetic {
                latitude_rad: libm::atan2(p.z, rho),
                longitude_rad,
                height_m: r - a,
            });
        }

        let ep2 = (a * a - b * b) / (b * b);
        let z = p.z;
        let f = 54.0 * b * b * z * z;
        let g = rho * rho + (1.0 - e2) * z * z - e2 * (a * a - b * b);
        if g <= 0.0 {
            return Err(Error::InvalidInput(
                "position is too close to the centre of the body",
            ));
        }
        let c = e2 * e2 * f * rho * rho / (g * g * g);
        let s = libm::cbrt(1.0 + c + libm::sqrt(c * c + 2.0 * c));
        let k = s + 1.0 + 1.0 / s;
        let pp = f / (3.0 * k * k * g * g);
        let q = libm::sqrt(1.0 + 2.0 * e2 * e2 * pp);
        let r0 = -(pp * e2 * rho) / (1.0 + q)
            + libm::sqrt(
                (0.5 * a * a * (1.0 + 1.0 / q)
                    - pp * (1.0 - e2) * z * z / (q * (1.0 + q))
                    - 0.5 * pp * rho * rho)
                    .max(0.0),
            );
        let t = rho - e2 * r0;
        let u = libm::hypot(t, z);
        let v = libm::sqrt(t * t + (1.0 - e2) * z * z);
        let z0 = b * b * z / (a * v);
        Ok(Geodetic {
            latitude_rad: libm::atan2(z + ep2 * z0, rho),
            longitude_rad,
            height_m: u * (1.0 - b * b / (a * v)),
        })
    }

    /// The transform from a local **east-north-up** frame at this position
    /// to the body-fixed frame.
    ///
    /// The local frame's origin is this point. Its x axis points east, y
    /// north and z along the outward ellipsoid normal. The frames do not move
    /// relative to each other. Use this to attach facility, building or
    /// antenna frames to a body.
    pub fn enu_to_body_fixed(&self, e: &Ellipsoid) -> Transform {
        let (sp, cp) = (libm::sin(self.latitude_rad), libm::cos(self.latitude_rad));
        let (sl, cl) = (libm::sin(self.longitude_rad), libm::cos(self.longitude_rad));
        // Columns: east, north, up expressed in body-fixed axes.
        let m = [
            [-sl, -sp * cl, cp * cl],
            [cl, -sp * sl, cp * sl],
            [0.0, cp, sp],
        ];
        let rotation = Rotation::from_matrix(m).unwrap_or(Rotation::IDENTITY);
        Transform::fixed(rotation, self.to_cartesian(e))
    }
}
