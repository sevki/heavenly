//! The engine behind the book's interactive playground, compiled to
//! WebAssembly (`wasm32-unknown-unknown`).
//!
//! Everything is computed with the heavenly crates themselves: two-body
//! propagation (`heavenly-orbit`), Earth orientation and geodetic conversion
//! (`heavenly-geo`), and frame resolution through a `FrameGraph`
//! (`heavenly-core`).
//!
//! The interface to JavaScript is a handful of `extern "C"` functions taking
//! and returning numbers. Array results are written to an output buffer whose
//! address JS reads with [`out_ptr`], so no binding generator is needed.
//! [`Model`] holds the logic and is tested natively.

use std::cell::RefCell;

use heavenly_core::{Epoch, Error, FrameGraph, FrameId, Locator, Namespace, TimeScale};
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_orbit::{gm, ConicElements, TwoBodyLocator};

/// Equatorial radius used for altitudes (WGS84), in metres.
pub const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// The London ground station shown in the playground.
pub const LONDON: (f64, f64, f64) = (51.5072, -0.1276, 35.0);

/// Playground state: an epoch, an Earth orbit, and the frames to resolve it in.
#[derive(Debug)]
pub struct Model {
    epoch: Epoch,
    orbit: TwoBodyLocator,
    graph: FrameGraph,
    station: FrameId,
}

impl Model {
    pub fn new() -> Result<Model, Error> {
        let epoch = Epoch::from_gregorian(TimeScale::Utc, 2026, 11, 19, 17, 0, 0, 0)?;
        let mut graph = FrameGraph::new();
        earth::register(&mut graph, earth::EarthOrientation::ZERO)?;
        let station = FrameId::custom(Namespace(1), 1).expect("custom namespace");
        let site = Geodetic::from_degrees(LONDON.0, LONDON.1, LONDON.2)?;
        graph.add_fixed(
            station,
            FrameId::ITRF,
            site.enu_to_body_fixed(&Ellipsoid::WGS84),
        )?;
        let mut m = Model {
            epoch,
            orbit: TwoBodyLocator {
                frame: FrameId::GCRF,
                elements: ConicElements {
                    periapsis_m: EARTH_RADIUS_M + 420e3,
                    eccentricity: 0.0005,
                    inclination_rad: 51.64f64.to_radians(),
                    ascending_node_rad: 0.0,
                    argument_of_periapsis_rad: 0.0,
                    mean_anomaly_rad: 0.0,
                    epoch,
                    gm_m3_s2: gm::EARTH,
                },
            },
            graph,
            station,
        };
        m.set_orbit(420.0, 0.0005, 51.64, 0.0, 0.0, 0.0)?;
        Ok(m)
    }

    /// Sets the epoch from UTC calendar fields (seconds may be fractional).
    pub fn set_epoch(
        &mut self,
        y: i32,
        mo: u32,
        d: u32,
        h: u32,
        mi: u32,
        s: f64,
    ) -> Result<(), Error> {
        if !(0.0..61.0).contains(&s) {
            return Err(Error::InvalidCalendar);
        }
        let whole = s.floor();
        let ns = ((s - whole) * 1e9).round() as u32;
        let narrow = |v: u32| u8::try_from(v).map_err(|_| Error::InvalidCalendar);
        self.epoch = Epoch::from_gregorian(
            TimeScale::Utc,
            y,
            narrow(mo)?,
            narrow(d)?,
            narrow(h)?,
            narrow(mi)?,
            whole as u8,
            ns.min(999_999_999),
        )?;
        self.orbit.elements.epoch = self.epoch;
        Ok(())
    }

    /// Sets the orbit: periapsis altitude (km), eccentricity, and angles in
    /// degrees. The mean anomaly applies at the model epoch.
    pub fn set_orbit(
        &mut self,
        periapsis_alt_km: f64,
        e: f64,
        i: f64,
        raan: f64,
        argp: f64,
        m0: f64,
    ) -> Result<(), Error> {
        let el = ConicElements {
            periapsis_m: EARTH_RADIUS_M + periapsis_alt_km * 1e3,
            eccentricity: e,
            inclination_rad: i.to_radians(),
            ascending_node_rad: raan.to_radians(),
            argument_of_periapsis_rad: argp.to_radians(),
            mean_anomaly_rad: m0.to_radians(),
            epoch: self.epoch,
            gm_m3_s2: gm::EARTH,
        };
        el.validate()?;
        if e >= 1.0 {
            return Err(Error::InvalidInput(
                "the playground shows closed orbits (e < 1)",
            ));
        }
        self.orbit.elements = el;
        Ok(())
    }

    fn at(&self, t_s: f64) -> Result<Epoch, Error> {
        self.epoch.add_seconds(t_s)
    }

    /// `[period s, perigee alt km, apogee alt km, perigee speed km/s, apogee speed km/s]`.
    pub fn info(&self) -> Vec<f64> {
        let el = &self.orbit.elements;
        let q = el.periapsis_m;
        let e = el.eccentricity;
        let a = q / (1.0 - e);
        let apo = a * (1.0 + e);
        let speed = |r: f64| (gm::EARTH * (2.0 / r - 1.0 / a)).sqrt() / 1e3;
        vec![
            el.period_s().unwrap_or(f64::NAN),
            (q - EARTH_RADIUS_M) / 1e3,
            (apo - EARTH_RADIUS_M) / 1e3,
            speed(q),
            speed(apo),
        ]
    }

    /// One full orbit in GCRF, as `n` points `[x, y, z]` in km.
    pub fn orbit_path(&self, n: usize) -> Result<Vec<f64>, Error> {
        let period = self
            .orbit
            .elements
            .period_s()
            .ok_or(Error::InvalidInput("open orbit"))?;
        let mut out = Vec::with_capacity(3 * n);
        for k in 0..n {
            let s = self
                .orbit
                .state(self.at(period * k as f64 / (n - 1).max(1) as f64)?)?;
            out.extend_from_slice(&(s.position_m / 1e3).to_array());
        }
        Ok(out)
    }

    /// The state `t_s` seconds after the epoch:
    /// `[x, y, z (km, GCRF), vx, vy, vz (km/s), lat °, lon °, alt km,
    ///   london elevation °, london azimuth °, london range km,
    ///   earth rotation angle °, periapsis x, y, z (km, GCRF)]`.
    pub fn state(&self, t_s: f64) -> Result<Vec<f64>, Error> {
        let epoch = self.at(t_s)?;
        let s = self.orbit.state(epoch)?;
        let ground = s.to_frame(FrameId::ITRF, &self.graph)?;
        let g = Geodetic::from_cartesian(ground.position_m, &Ellipsoid::WGS84)?;
        let local = self
            .orbit
            .state_at(epoch, self.station, &self.graph)?
            .position_m;
        let range = local.norm();
        let elevation = (local.z / range).asin().to_degrees();
        let azimuth = local.x.atan2(local.y).to_degrees().rem_euclid(360.0);
        let era = earth::earth_rotation_angle(epoch, &earth::EarthOrientation::ZERO)?;
        let (peri, _) = ConicElements {
            mean_anomaly_rad: 0.0,
            ..self.orbit.elements
        }
        .state_at(self.orbit.elements.epoch)?;
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(&(s.position_m / 1e3).to_array());
        out.extend_from_slice(&(s.velocity_m_s / 1e3).to_array());
        out.extend_from_slice(&[
            g.latitude_rad.to_degrees(),
            g.longitude_rad.to_degrees(),
            g.height_m / 1e3,
            elevation,
            azimuth,
            range / 1e3,
            era.to_degrees(),
        ]);
        out.extend_from_slice(&(peri / 1e3).to_array());
        Ok(out)
    }

    /// Sub-satellite points from `t0_s` to `t1_s` (seconds after the epoch),
    /// `n` samples of `[lat °, lon °, london elevation °]`.
    pub fn ground_track(&self, t0_s: f64, t1_s: f64, n: usize) -> Result<Vec<f64>, Error> {
        let mut out = Vec::with_capacity(3 * n);
        for k in 0..n {
            let t = t0_s + (t1_s - t0_s) * k as f64 / (n - 1).max(1) as f64;
            let epoch = self.at(t)?;
            let s = self.orbit.state_at(epoch, FrameId::ITRF, &self.graph)?;
            let g = Geodetic::from_cartesian(s.position_m, &Ellipsoid::WGS84)?;
            let local = self
                .orbit
                .state_at(epoch, self.station, &self.graph)?
                .position_m;
            out.extend_from_slice(&[
                g.latitude_rad.to_degrees(),
                g.longitude_rad.to_degrees(),
                (local.z / local.norm()).asin().to_degrees(),
            ]);
        }
        Ok(out)
    }

    /// Earth's rotation axis direction in GCRF, and the GCRF → ITRF matrix
    /// (row-major), at `t_s`: lets the JS draw a rotating Earth grid.
    pub fn earth_frame(&self, t_s: f64) -> Result<Vec<f64>, Error> {
        let m = earth::gcrs_to_itrs_matrix(self.at(t_s)?, &earth::EarthOrientation::ZERO)?;
        Ok(m.iter().flatten().copied().collect())
    }

    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
}

// --- WebAssembly interface ---------------------------------------------------

thread_local! {
    static MODEL: RefCell<Option<Model>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with_model<T>(f: impl FnOnce(&mut Model) -> Result<T, Error>) -> Result<T, Error> {
    MODEL.with(|m| {
        let mut m = m.borrow_mut();
        if m.is_none() {
            *m = Some(Model::new()?);
        }
        f(m.as_mut().expect("initialised above"))
    })
}

/// Stores an array result and returns its length; on error stores the message
/// and returns 0.
fn publish(r: Result<Vec<f64>, Error>) -> usize {
    match r {
        Ok(v) => {
            let n = v.len();
            OUT.with(|o| *o.borrow_mut() = v);
            n
        }
        Err(e) => {
            set_error(e);
            0
        }
    }
}

fn set_error(e: Error) {
    let msg = e.to_string();
    ERROR.with(|s| *s.borrow_mut() = msg);
}

fn status(r: Result<(), Error>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            set_error(e);
            1
        }
    }
}

/// Address of the last array result (f64s).
#[no_mangle]
pub extern "C" fn out_ptr() -> *const f64 {
    OUT.with(|o| o.borrow().as_ptr())
}

/// Writes the last error message into the output buffer as UTF-8 bytes
/// stored one per f64, and returns its length.
#[no_mangle]
pub extern "C" fn last_error() -> usize {
    let bytes: Vec<f64> = ERROR.with(|s| s.borrow().bytes().map(f64::from).collect());
    publish(Ok(bytes))
}

#[no_mangle]
pub extern "C" fn set_epoch(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: f64) -> i32 {
    status(with_model(|m| m.set_epoch(y, mo, d, h, mi, s)))
}

#[no_mangle]
pub extern "C" fn set_orbit(alt_km: f64, e: f64, i: f64, raan: f64, argp: f64, m0: f64) -> i32 {
    status(with_model(|m| m.set_orbit(alt_km, e, i, raan, argp, m0)))
}

#[no_mangle]
pub extern "C" fn info() -> usize {
    publish(with_model(|m| Ok(m.info())))
}

#[no_mangle]
pub extern "C" fn orbit_path(n: usize) -> usize {
    publish(with_model(|m| m.orbit_path(n)))
}

#[no_mangle]
pub extern "C" fn state(t_s: f64) -> usize {
    publish(with_model(|m| m.state(t_s)))
}

#[no_mangle]
pub extern "C" fn ground_track(t0_s: f64, t1_s: f64, n: usize) -> usize {
    publish(with_model(|m| m.ground_track(t0_s, t1_s, n)))
}

#[no_mangle]
pub extern "C" fn earth_frame(t_s: f64) -> usize {
    publish(with_model(|m| m.earth_frame(t_s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use heavenly_core::Vec3;

    #[test]
    fn iss_like_orbit() {
        let m = Model::new().unwrap();
        let info = m.info();
        assert!((info[0] / 60.0 - 92.8).abs() < 0.3, "period {}", info[0]);
        assert!((info[1] - 420.0).abs() < 1e-6);
        let track = m.ground_track(0.0, 86400.0, 500).unwrap();
        let max_lat = track.chunks(3).map(|c| c[0].abs()).fold(0.0, f64::max);
        // Geodetic latitude of a 51.64° orbit peaks slightly above 51.64°.
        assert!((51.5..52.2).contains(&max_lat), "{max_lat}");
        let s = m.state(0.0).unwrap();
        assert!((Vec3::new(s[0], s[1], s[2]).norm() - 6798.137).abs() < 5.0);
        assert!((s[8] - 420.0).abs() < 25.0, "altitude {}", s[8]);
        assert!((-90.0..=90.0).contains(&s[9]) && s[11] > 0.0);
    }

    #[test]
    fn london_elevation_matches_direct_geometry() {
        let m = Model::new().unwrap();
        let s = m.state(1234.0).unwrap();
        // Recompute elevation from ITRF positions directly.
        let site = Geodetic::from_degrees(LONDON.0, LONDON.1, LONDON.2).unwrap();
        let sat = Geodetic::from_degrees(s[6], s[7], s[8] * 1e3)
            .unwrap()
            .to_cartesian(&Ellipsoid::WGS84);
        let d = sat - site.to_cartesian(&Ellipsoid::WGS84);
        let up = site
            .enu_to_body_fixed(&Ellipsoid::WGS84)
            .rotation
            .apply(Vec3::Z);
        let el = (d.dot(up) / d.norm()).asin().to_degrees();
        assert!((el - s[9]).abs() < 1e-6, "{el} vs {}", s[9]);
    }

    #[test]
    fn invalid_inputs_report_errors() {
        assert_eq!(set_orbit(400.0, 1.2, 10.0, 0.0, 0.0, 0.0), 1);
        let n = last_error();
        let msg: String = OUT.with(|o| o.borrow()[..n].iter().map(|b| *b as u8 as char).collect());
        assert!(msg.contains("closed orbits"), "{msg}");
        assert_eq!(set_epoch(2026, 2, 30, 0, 0, 0.0), 1);
        assert_eq!(set_epoch(2026, 11, 19, 17, 0, 0.0), 0);
        assert_eq!(set_orbit(400.0, 0.1, 10.0, 0.0, 0.0, 0.0), 0);
        assert!(orbit_path(100) == 300);
    }
}
