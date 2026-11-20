//! A conic whose plane and apsides turn at the Earth's J2 secular rates.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector};

use crate::forces::{EARTH_RADIUS_M, EARTH_ZONALS};
use crate::ConicElements;

/// An orbit with the secular (steady) effects of a body's oblateness, J2:
/// the node turns at
/// `Ω̇ = −(3/2) n J2 (R/p)² cos i`, the periapsis at
/// `ω̇ = (3/4) n J2 (R/p)² (5 cos² i − 1)`, and the mean anomaly runs at
/// `n [1 + (3/4) J2 (R/p)² √(1−e²) (3 cos² i − 1)]`.
///
/// It is as cheap as [`TwoBodyLocator`](crate::TwoBodyLocator) and has
/// the effects that dominate over days: sun-synchronous planes that keep
/// facing the Sun, the westward drift of a low orbit's node, and the
/// "frozen" apsides of a Molniya orbit at 63.4°. It leaves out J2's
/// short-period wobble (kilometres in a low orbit), the other harmonics,
/// drag and the Sun and Moon. For those, use
/// [`NumericalPropagator`](crate::NumericalPropagator). The elements are
/// treated as mean elements, and the body's equator is the frame's x–y
/// plane: GCRF's equator is the Earth's to 0.35° in 2026.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecularJ2Locator {
    pub frame: FrameId,
    pub elements: ConicElements,
    pub j2: f64,
    pub equatorial_radius_m: f64,
}

impl SecularJ2Locator {
    /// An Earth orbit (EGM2008 J2) in GCRF.
    pub fn earth(elements: ConicElements) -> Result<SecularJ2Locator, Error> {
        elements.validate()?;
        if elements.eccentricity >= 1.0 {
            return Err(Error::InvalidInput("secular J2 rates need a closed orbit"));
        }
        Ok(SecularJ2Locator {
            frame: FrameId::GCRF,
            elements,
            j2: EARTH_ZONALS[0],
            equatorial_radius_m: EARTH_RADIUS_M,
        })
    }

    /// An Earth orbit from osculating elements, such as those of a state
    /// vector: the semi-major axis is converted to its mean value by
    /// removing J2's first-order short-period term (Kozai 1959),
    ///
    /// `a_osc − a_mean = (J2 R²/a) [ (3cos²i − 1)/2 · ((a/r)³ − (1−e²)^−3/2)
    ///                               + (3/2) sin²i (a/r)³ cos 2u ]`,
    ///
    /// with `u` the argument of latitude. Without this, the mean motion
    /// would be off by about `(3/2) Δa/a`, around 2e-3 in a low orbit, and
    /// the satellite would drift along its orbit by hundreds of kilometres a
    /// day. The other elements are kept: their short-period terms are of the
    /// same kilometre size but do not accumulate. The orbit therefore starts
    /// within a few kilometres of the osculating state, not on it.
    pub fn earth_from_osculating(elements: ConicElements) -> Result<SecularJ2Locator, Error> {
        let mut l = SecularJ2Locator::earth(elements)?;
        let (r, _) = elements.state_at(elements.epoch)?;
        let el = &elements;
        let e = el.eccentricity;
        let a = el.periapsis_m / (1.0 - e);
        let (si, ci) = (libm::sin(el.inclination_rad), libm::cos(el.inclination_rad));
        let (sn, cn) = (
            libm::sin(el.ascending_node_rad),
            libm::cos(el.ascending_node_rad),
        );
        // Argument of latitude: from the ascending node, in the orbit plane.
        let node = heavenly_core::Vec3::new(cn, sn, 0.0);
        let in_plane = heavenly_core::Vec3::new(-sn * ci, cn * ci, si);
        let u = libm::atan2(r.dot(in_plane), r.dot(node));
        let ar3 = libm::pow(a / r.norm(), 3.0);
        let eta3 = libm::pow(1.0 - e * e, -1.5);
        let k = l.j2 * l.equatorial_radius_m * l.equatorial_radius_m / a;
        let da = k
            * ((3.0 * ci * ci - 1.0) / 2.0 * (ar3 - eta3)
                + 1.5 * si * si * ar3 * libm::cos(2.0 * u));
        let a_mean = a - da;
        l.elements.periapsis_m = a_mean * (1.0 - e);
        Ok(l)
    }

    /// The secular rates (rad/s) of the node, the argument of periapsis
    /// and the mean anomaly.
    pub fn rates(&self) -> (f64, f64, f64) {
        let el = &self.elements;
        let e = el.eccentricity;
        let a = el.periapsis_m / (1.0 - e);
        let n = libm::sqrt(el.gm_m3_s2 / (a * a * a));
        let p = a * (1.0 - e * e);
        let k = self.j2 * (self.equatorial_radius_m / p) * (self.equatorial_radius_m / p);
        let c = libm::cos(el.inclination_rad);
        (
            -1.5 * n * k * c,
            0.75 * n * k * (5.0 * c * c - 1.0),
            n * (1.0 + 0.75 * k * libm::sqrt(1.0 - e * e) * (3.0 * c * c - 1.0)),
        )
    }

    /// The elements at `epoch`, with the node, periapsis and mean anomaly
    /// advanced at the secular rates.
    pub fn elements_at(&self, epoch: Epoch) -> ConicElements {
        let dt = epoch.seconds_since(self.elements.epoch);
        let (node, peri, mean) = self.rates();
        let wrap = |x: f64| {
            let tau = core::f64::consts::TAU;
            let m = libm::fmod(x, tau);
            if m < 0.0 {
                m + tau
            } else {
                m
            }
        };
        ConicElements {
            ascending_node_rad: wrap(self.elements.ascending_node_rad + node * dt),
            argument_of_periapsis_rad: wrap(self.elements.argument_of_periapsis_rad + peri * dt),
            mean_anomaly_rad: wrap(self.elements.mean_anomaly_rad + mean * dt),
            epoch,
            ..self.elements
        }
    }

    /// The state in the locator's own frame.
    pub fn state(&self, epoch: Epoch) -> Result<StateVector, Error> {
        let (p, v) = self.elements_at(epoch).state_at(epoch)?;
        Ok(StateVector::new(self.frame, epoch, p, v))
    }
}

impl Locator for SecularJ2Locator {
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
        Ok(self.state(epoch)?.to_frame(frame, resolver)?)
    }
}
