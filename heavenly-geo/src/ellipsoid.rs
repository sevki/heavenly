use heavenly_core::Error;

/// A reference ellipsoid of revolution (oblate spheroid), or a sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipsoid {
    /// Equatorial radius in metres.
    pub semi_major_axis_m: f64,
    /// Flattening `(a − b) / a`. Zero for a sphere.
    pub flattening: f64,
}

impl Ellipsoid {
    /// WGS 84 (GPS), `a = 6 378 137 m`, `1/f = 298.257 223 563`.
    pub const WGS84: Ellipsoid = Ellipsoid {
        semi_major_axis_m: 6_378_137.0,
        flattening: 1.0 / 298.257_223_563,
    };
    /// GRS 80 (ITRF realisations), `a = 6 378 137 m`, `1/f = 298.257 222 101`.
    pub const GRS80: Ellipsoid = Ellipsoid {
        semi_major_axis_m: 6_378_137.0,
        flattening: 1.0 / 298.257_222_101,
    };
    /// IAU mean lunar sphere, `R = 1 737 400 m`.
    pub const MOON: Ellipsoid = Ellipsoid {
        semi_major_axis_m: 1_737_400.0,
        flattening: 0.0,
    };
    /// IAU 2015 Mars ellipsoid, `a = 3 396 190 m`, `b = 3 376 200 m`.
    pub const MARS: Ellipsoid = Ellipsoid {
        semi_major_axis_m: 3_396_190.0,
        flattening: (3_396_190.0 - 3_376_200.0) / 3_396_190.0,
    };

    /// Builds an ellipsoid, validating `a > 0` and `0 ≤ f < 1`.
    pub fn new(semi_major_axis_m: f64, flattening: f64) -> Result<Ellipsoid, Error> {
        if !(semi_major_axis_m.is_finite() && flattening.is_finite()) {
            return Err(Error::NonFinite);
        }
        if semi_major_axis_m <= 0.0 || !(0.0..1.0).contains(&flattening) {
            return Err(Error::InvalidInput(
                "ellipsoid needs a > 0 and 0 <= flattening < 1",
            ));
        }
        Ok(Ellipsoid {
            semi_major_axis_m,
            flattening,
        })
    }

    /// Polar radius `b = a (1 − f)` in metres.
    pub fn semi_minor_axis_m(&self) -> f64 {
        self.semi_major_axis_m * (1.0 - self.flattening)
    }

    /// First eccentricity squared, `e² = f (2 − f)`.
    pub fn eccentricity_squared(&self) -> f64 {
        self.flattening * (2.0 - self.flattening)
    }
}
