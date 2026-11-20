//! The engine behind the book's interactive playground, compiled to
//! WebAssembly (`wasm32-unknown-unknown`).
//!
//! Everything is computed with the heavenly crates themselves: two-body
//! propagation and SGP4 from a TLE (`heavenly-orbit`), Earth orientation and
//! geodetic conversion (`heavenly-geo`), frame resolution through a
//! `FrameGraph` (`heavenly-core`), and the Sun, shadows, look angles and
//! pass search (`heavenly-query`). The JavaScript only draws.
//!
//! The interface to JavaScript is a handful of `extern "C"` functions taking
//! and returning numbers. Array results are written to an output buffer whose
//! address JS reads with [`out_ptr`], so no binding generator is needed.
//! [`Model`] holds the logic and is tested natively.

use std::cell::RefCell;

use heavenly_core::{
    Epoch, Error, FrameGraph, FrameId, FrameResolver, Locator, Namespace, StateVector, TimeScale,
    Vec3,
};
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_orbit::{gm, ConicElements, Gravity, OpsMode, Sgp4, Tle, TwoBodyLocator};
use heavenly_query::{find_windows, look_angles, AnalyticSun, Occulter, ShadowKind};

/// What the playground propagates: a two-body conic from the sliders, or a
/// TLE through SGP4.
#[derive(Clone, Debug)]
pub enum Satellite {
    TwoBody(TwoBodyLocator),
    Tle(Box<Sgp4>, Tle),
}

impl Locator for Satellite {
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
        match self {
            Satellite::TwoBody(l) => l.state_at(epoch, frame, resolver),
            Satellite::Tle(s, _) => s.state_at(epoch, frame, resolver),
        }
    }
}

/// An ISS element set, used by the playground's "ISS (TLE)" preset.
pub const ISS_TLE: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992
2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

/// Equatorial radius used for altitudes (WGS84), in metres.
pub const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// The London ground station shown in the playground.
pub const LONDON: (f64, f64, f64) = (51.5072, -0.1276, 35.0);

/// Demo ground sites drawn on the map: Apple's own data centres, at
/// city-level (approximate) coordinates, as in the `orbital_datacenter`
/// example. `(name, latitude °, longitude °)`.
pub const SITES: [(&str, f64, f64); 6] = [
    ("Maiden", 35.5757, -81.2115),
    ("Prineville", 44.2999, -120.8345),
    ("Reno", 39.5296, -119.8138),
    ("Mesa", 33.4152, -111.8315),
    ("Waukee", 41.6117, -93.8852),
    ("Viborg", 56.4532, 9.4020),
];

/// Playground state: an epoch, an Earth orbit, and the frames to resolve it in.
#[derive(Debug)]
pub struct Model {
    epoch: Epoch,
    orbit: TwoBodyLocator,
    sat: Satellite,
    graph: FrameGraph,
    station: FrameId,
    sites: Vec<FrameId>,
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
        let mut sites = Vec::new();
        for (k, (_, lat, lon)) in SITES.iter().enumerate() {
            let f = FrameId::custom(Namespace(1), 100 + k as i64).expect("custom namespace");
            let g = Geodetic::from_degrees(*lat, *lon, 0.0)?;
            graph.add_fixed(f, FrameId::ITRF, g.enu_to_body_fixed(&Ellipsoid::WGS84))?;
            sites.push(f);
        }
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
            sat: Satellite::TwoBody(TwoBodyLocator {
                frame: FrameId::GCRF,
                elements: ConicElements {
                    periapsis_m: EARTH_RADIUS_M + 420e3,
                    eccentricity: 0.0,
                    inclination_rad: 0.0,
                    ascending_node_rad: 0.0,
                    argument_of_periapsis_rad: 0.0,
                    mean_anomaly_rad: 0.0,
                    epoch,
                    gm_m3_s2: gm::EARTH,
                },
            }),
            graph,
            station,
            sites,
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
        if let Satellite::TwoBody(l) = &mut self.sat {
            l.elements.epoch = self.epoch;
        }
        Ok(())
    }

    /// Switches to a TLE propagated with SGP4, and moves the epoch to the
    /// TLE's. Accepts two lines, or three with a name line first.
    pub fn set_tle(&mut self, text: &str) -> Result<(), Error> {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim_end)
            .filter(|l| l.starts_with("1 ") || l.starts_with("2 "))
            .collect();
        let [l1, l2] = lines[..] else {
            return Err(Error::Parse("expected the two lines of a TLE"));
        };
        let tle = Tle::parse(l1, l2)?;
        let sgp4 = Sgp4::new(&tle, Gravity::Wgs72, OpsMode::Improved)?;
        self.epoch = sgp4.epoch();
        self.sat = Satellite::Tle(Box::new(sgp4), tle);
        Ok(())
    }

    /// Elevation (°) of the satellite from each of [`SITES`] at `t_s`.
    pub fn site_view(&self, t_s: f64) -> Result<Vec<f64>, Error> {
        let epoch = self.at(t_s)?;
        self.sites
            .iter()
            .map(|f| {
                look_angles::<_, _, Error>(*f, &self.sat, epoch, &self.graph)
                    .map(|a| a.elevation_rad.to_degrees())
            })
            .collect()
    }

    /// A dawn–dusk sun-synchronous orbit at `alt_km` for the model epoch:
    /// `[inclination °, ascending node °]`. The node is 90° east of the
    /// Sun's right ascension, so the orbit plane faces the Sun.
    pub fn dawn_dusk(&self, alt_km: f64) -> Result<Vec<f64>, Error> {
        let a = EARTH_RADIUS_M + alt_km * 1e3;
        let i = heavenly_orbit::design::sun_synchronous_inclination_rad(a, 0.0).ok_or(
            Error::InvalidInput("no sun-synchronous orbit at this altitude"),
        )?;
        let sun = AnalyticSun::position_m(self.epoch);
        let node = (sun.y.atan2(sun.x).to_degrees() + 90.0).rem_euclid(360.0);
        Ok(vec![i.to_degrees(), node])
    }

    /// The model epoch as UTC `[year, month, day, hour, minute, second]`.
    pub fn epoch_fields(&self) -> Result<Vec<f64>, Error> {
        let c = self.epoch.to_calendar(TimeScale::Utc)?;
        Ok(vec![
            c.year as f64,
            c.month as f64,
            c.day as f64,
            c.hour as f64,
            c.minute as f64,
            c.second as f64 + c.nanosecond as f64 * 1e-9,
        ])
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
        self.sat = Satellite::TwoBody(self.orbit);
        Ok(())
    }

    fn at(&self, t_s: f64) -> Result<Epoch, Error> {
        self.epoch.add_seconds(t_s)
    }

    /// `[period s, perigee alt km, apogee alt km, perigee speed km/s, apogee speed km/s]`.
    pub fn info(&self) -> Vec<f64> {
        let (a, e, period) = match &self.sat {
            Satellite::TwoBody(l) => {
                let el = &l.elements;
                (
                    el.periapsis_m / (1.0 - el.eccentricity),
                    el.eccentricity,
                    el.period_s().unwrap_or(f64::NAN),
                )
            }
            Satellite::Tle(_, tle) => {
                // Mean semi-major axis from the mean motion.
                let n = tle.mean_motion_rev_day * std::f64::consts::TAU / 86_400.0;
                (
                    (gm::EARTH / (n * n)).cbrt(),
                    tle.eccentricity,
                    86_400.0 / tle.mean_motion_rev_day,
                )
            }
        };
        let q = a * (1.0 - e);
        let apo = a * (1.0 + e);
        let speed = |r: f64| (gm::EARTH * (2.0 / r - 1.0 / a)).sqrt() / 1e3;
        vec![
            period,
            (q - EARTH_RADIUS_M) / 1e3,
            (apo - EARTH_RADIUS_M) / 1e3,
            speed(q),
            speed(apo),
        ]
    }

    /// One full orbit in GCRF, as `n` points `[x, y, z (km), sunlit fraction]`.
    pub fn orbit_path(&self, n: usize) -> Result<Vec<f64>, Error> {
        let period = self.info()[0];
        if !period.is_finite() {
            return Err(Error::InvalidInput("open orbit"));
        }
        let mut out = Vec::with_capacity(4 * n);
        for k in 0..n {
            let t = self.at(period * k as f64 / (n - 1).max(1) as f64)?;
            let s = self.sat.state_at(t, FrameId::GCRF, &self.graph)?;
            out.extend_from_slice(&(s.position_m / 1e3).to_array());
            out.push(
                Occulter::EARTH
                    .shadow::<_, _, _, Error>(&self.sat, &AnalyticSun, t, &self.graph)?
                    .sunlit_fraction(),
            );
        }
        Ok(out)
    }

    /// The state `t_s` seconds after the epoch:
    /// `[x, y, z (km, GCRF), vx, vy, vz (km/s), lat °, lon °, alt km,
    ///   london elevation °, london azimuth °, london range km,
    ///   earth rotation angle °, periapsis x, y, z (km, GCRF),
    ///   sunlit fraction, shadow (0 sunlit, 1 penumbra, 2 umbra, 3 antumbra),
    ///   Sun elevation at London °, subsolar latitude °, subsolar longitude °,
    ///   Sun direction x, y, z (unit, GCRF)]`.
    pub fn state(&self, t_s: f64) -> Result<Vec<f64>, Error> {
        let epoch = self.at(t_s)?;
        let s = self.sat.state_at(epoch, FrameId::GCRF, &self.graph)?;
        let ground = s.to_frame(FrameId::ITRF, &self.graph)?;
        let g = Geodetic::from_cartesian(ground.position_m, &Ellipsoid::WGS84)?;
        let local = self
            .sat
            .state_at(epoch, self.station, &self.graph)?
            .position_m;
        let range = local.norm();
        let elevation = (local.z / range).asin().to_degrees();
        let azimuth = local.x.atan2(local.y).to_degrees().rem_euclid(360.0);
        let era = earth::earth_rotation_angle(epoch, &earth::EarthOrientation::ZERO)?;
        let peri = match &self.sat {
            Satellite::TwoBody(l) => {
                ConicElements {
                    mean_anomaly_rad: 0.0,
                    ..l.elements
                }
                .state_at(l.elements.epoch)?
                .0
            }
            // SGP4 has no fixed periapsis; show the nearest point of the
            // current orbit instead.
            Satellite::Tle(..) => self.nearest_point(epoch)?,
        };
        let shadow = Occulter::EARTH.shadow::<_, _, _, Error>(
            &self.sat,
            &AnalyticSun,
            epoch,
            &self.graph,
        )?;
        let sun_el = look_angles::<_, _, Error>(self.station, &AnalyticSun, epoch, &self.graph)?
            .elevation_rad;
        let (sub_lat, sub_lon) = self.subsolar(epoch)?;
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
        out.extend_from_slice(&[
            shadow.sunlit_fraction(),
            match shadow.kind() {
                ShadowKind::Sunlit => 0.0,
                ShadowKind::Penumbra => 1.0,
                ShadowKind::Umbra => 2.0,
                ShadowKind::Antumbra => 3.0,
            },
            sun_el.to_degrees(),
            sub_lat,
            sub_lon,
        ]);
        let sun_dir = AnalyticSun::position_m(epoch)
            .normalized()
            .ok_or(Error::NonFinite)?;
        out.extend_from_slice(&sun_dir.to_array());
        Ok(out)
    }

    /// The lowest point over the next orbit, in GCRF (metres).
    fn nearest_point(&self, epoch: Epoch) -> Result<Vec3, Error> {
        let period = self.info()[0];
        let mut best = (f64::INFINITY, Vec3::ZERO);
        for k in 0..180 {
            let p = self
                .sat
                .state_at(
                    epoch.add_seconds(period * k as f64 / 180.0)?,
                    FrameId::GCRF,
                    &self.graph,
                )?
                .position_m;
            if p.norm() < best.0 {
                best = (p.norm(), p);
            }
        }
        Ok(best.1)
    }

    /// The point where the Sun is overhead: geocentric latitude, longitude (°).
    fn subsolar(&self, epoch: Epoch) -> Result<(f64, f64), Error> {
        let sun = AnalyticSun
            .state_at(epoch, FrameId::ITRF, &self.graph)?
            .position_m;
        Ok((
            (sun.z / sun.norm()).asin().to_degrees(),
            sun.y.atan2(sun.x).to_degrees(),
        ))
    }

    /// The day–night line at `t_s`, ready to fill on a longitude–latitude
    /// map: the subsolar point `[lat °, lon °]`, then `n` points
    /// `[lat °, lon °]` from longitude −180° to 180°, each at the latitude
    /// where the Sun is on the horizon. The night side lies between this
    /// line and the pole opposite the subsolar point.
    pub fn terminator(&self, t_s: f64, n: usize) -> Result<Vec<f64>, Error> {
        let (lat0, lon0) = self.subsolar(self.at(t_s)?)?;
        // At an equinox the line runs along two meridians; a tiny tilt keeps
        // tan(lat0) finite and makes it a steep step instead.
        let t0 = lat0.to_radians().tan();
        let t0 = if t0.abs() < 1e-9 {
            1e-9f64.copysign(t0)
        } else {
            t0
        };
        let mut out = Vec::with_capacity(2 * n + 2);
        out.extend_from_slice(&[lat0, lon0]);
        for k in 0..n {
            let lon = -180.0 + 360.0 * k as f64 / (n - 1).max(1) as f64;
            let lat = (-(lon - lon0).to_radians().cos() / t0).atan();
            out.extend_from_slice(&[lat.to_degrees(), lon]);
        }
        Ok(out)
    }

    /// Passes over London above 10° between `t0_s` and `t1_s`, as rows
    /// `[start s, end s, max elevation °, visible s]`. A moment is visible
    /// when the satellite is sunlit and the Sun is more than 6° below
    /// London's horizon.
    pub fn passes(&self, t0_s: f64, t1_s: f64) -> Result<Vec<f64>, Error> {
        let (t0, t1) = (self.at(t0_s)?, self.at(t1_s)?);
        let mask = 10f64.to_radians();
        let el = |t: Epoch| {
            look_angles::<_, _, Error>(self.station, &self.sat, t, &self.graph)
                .map(|a| a.elevation_rad)
        };
        let step = (self.info()[0] / 100.0).clamp(10.0, 300.0);
        let windows = find_windows(t0, t1, step, 1e-2, |t| Ok::<f64, Error>(el(t)? - mask))?;
        let mut out = Vec::with_capacity(4 * windows.len());
        for w in windows {
            let (mut top, mut visible) = (f64::MIN, 0.0);
            let n = ((w.duration_s() / 5.0).ceil() as usize).max(2);
            let dt = w.duration_s() / n as f64;
            for k in 0..=n {
                let t = w.start.add_seconds(dt * k as f64)?;
                top = top.max(el(t)?);
                let lit = Occulter::EARTH.shadow::<_, _, _, Error>(
                    &self.sat,
                    &AnalyticSun,
                    t,
                    &self.graph,
                )?;
                let sun = look_angles::<_, _, Error>(self.station, &AnalyticSun, t, &self.graph)?;
                if k < n && lit.sunlit_fraction() > 0.5 && sun.elevation_rad < -6f64.to_radians() {
                    visible += dt;
                }
            }
            out.extend_from_slice(&[
                w.start.seconds_since(self.epoch),
                w.end.seconds_since(self.epoch),
                top.to_degrees(),
                visible,
            ]);
        }
        Ok(out)
    }

    /// Sub-satellite points from `t0_s` to `t1_s` (seconds after the epoch),
    /// `n` samples of `[lat °, lon °, london elevation °, sunlit fraction]`.
    pub fn ground_track(&self, t0_s: f64, t1_s: f64, n: usize) -> Result<Vec<f64>, Error> {
        let mut out = Vec::with_capacity(4 * n);
        for k in 0..n {
            let t = t0_s + (t1_s - t0_s) * k as f64 / (n - 1).max(1) as f64;
            let epoch = self.at(t)?;
            let s = self.sat.state_at(epoch, FrameId::ITRF, &self.graph)?;
            let g = Geodetic::from_cartesian(s.position_m, &Ellipsoid::WGS84)?;
            let local = self
                .sat
                .state_at(epoch, self.station, &self.graph)?
                .position_m;
            let sun = AnalyticSun
                .state_at(epoch, FrameId::ITRF, &self.graph)?
                .position_m;
            out.extend_from_slice(&[
                g.latitude_rad.to_degrees(),
                g.longitude_rad.to_degrees(),
                (local.z / local.norm()).asin().to_degrees(),
                Occulter::EARTH
                    .shadow_at(s.position_m, sun)
                    .sunlit_fraction(),
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

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// A buffer of `len` bytes for JS to write text into (for [`set_tle`]).
#[no_mangle]
pub extern "C" fn input_ptr(len: usize) -> *mut u8 {
    INPUT.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

/// Loads the TLE that JS wrote into the input buffer.
#[no_mangle]
pub extern "C" fn set_tle(len: usize) -> i32 {
    let text = INPUT
        .with(|b| String::from_utf8_lossy(&b.borrow()[..len.min(b.borrow().len())]).into_owned());
    status(with_model(|m| m.set_tle(&text)))
}

/// `[lat °, lon °]` of each demo site.
#[no_mangle]
pub extern "C" fn sites() -> usize {
    publish(Ok(SITES
        .iter()
        .flat_map(|(_, la, lo)| [*la, *lo])
        .collect()))
}

/// The demo sites' names, newline-separated, one UTF-8 byte per f64.
#[no_mangle]
pub extern "C" fn site_names() -> usize {
    let names = SITES.iter().map(|s| s.0).collect::<Vec<_>>().join("\n");
    publish(Ok(names.bytes().map(f64::from).collect()))
}

#[no_mangle]
pub extern "C" fn site_view(t_s: f64) -> usize {
    publish(with_model(|m| m.site_view(t_s)))
}

#[no_mangle]
pub extern "C" fn dawn_dusk(alt_km: f64) -> usize {
    publish(with_model(|m| m.dawn_dusk(alt_km)))
}

#[no_mangle]
pub extern "C" fn epoch_fields() -> usize {
    publish(with_model(|m| m.epoch_fields()))
}

#[no_mangle]
pub extern "C" fn terminator(t_s: f64, n: usize) -> usize {
    publish(with_model(|m| m.terminator(t_s, n)))
}

#[no_mangle]
pub extern "C" fn passes(t0_s: f64, t1_s: f64) -> usize {
    publish(with_model(|m| m.passes(t0_s, t1_s)))
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
        let max_lat = track.chunks(4).map(|c| c[0].abs()).fold(0.0, f64::max);
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
    fn tle_mode() {
        let mut m = Model::new().unwrap();
        m.set_tle(&format!("ISS (ZARYA)\n{ISS_TLE}\n")).unwrap();
        assert_eq!(
            &m.epoch_fields().unwrap()[..5],
            &[2020.0, 7.0, 12.0, 21.0, 16.0]
        );
        let info = m.info();
        assert!((info[0] / 60.0 - 92.9).abs() < 0.2, "period {}", info[0]);
        let s = m.state(0.0).unwrap();
        assert_eq!(s.len(), 24);
        assert!((0.0..=1.0).contains(&s[16]) && (0.0..=3.0).contains(&s[17]));
        assert!((s[8] - 420.0).abs() < 40.0, "altitude {}", s[8]);
        // The track and the shadow agree with heavenly-query directly.
        let track = m.ground_track(0.0, 5400.0, 200).unwrap();
        let dark = track.chunks(4).filter(|c| c[3] < 0.5).count();
        // July: the ISS is in its high-sun season, in shadow ~15 % of each orbit.
        assert!((20..60).contains(&dark), "{dark} of 200 samples in shadow");
        // Back to the sliders.
        assert_eq!(set_orbit(400.0, 0.0, 10.0, 0.0, 0.0, 0.0), 0);
        assert!(m.set_tle("not a tle").is_err());
    }

    #[test]
    fn passes_match_the_visible_passes_example() {
        let mut m = Model::new().unwrap();
        m.set_tle(ISS_TLE).unwrap();
        let rows = m.passes(0.0, 2.0 * 86_400.0).unwrap();
        let visible: Vec<_> = rows.chunks(4).filter(|r| r[3] > 0.0).collect();
        // `cargo run --example visible_passes` finds these seven, with these
        // durations (s); passes() samples every 5 s.
        let expect = [234.0, 405.0, 404.0, 350.0, 404.0, 405.0, 377.0];
        assert_eq!(visible.len(), expect.len(), "{visible:?}");
        for (r, d) in visible.iter().zip(expect) {
            assert!((r[3] - d).abs() <= 6.0, "{r:?} vs {d}");
        }
    }

    #[test]
    fn terminator_is_where_the_sun_sets() {
        let m = Model::new().unwrap();
        let t = m.terminator(3600.0, 90).unwrap();
        let sun = AnalyticSun
            .state_at(m.at(3600.0).unwrap(), FrameId::ITRF, &m.graph)
            .unwrap()
            .position_m
            .normalized()
            .unwrap();
        let unit = |lat: f64, lon: f64| {
            let (la, lo) = (lat.to_radians(), lon.to_radians());
            Vec3::new(la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin())
        };
        assert!((unit(t[0], t[1]).dot(sun) - 1.0).abs() < 1e-12);
        for p in t[2..].chunks(2) {
            assert!(unit(p[0], p[1]).dot(sun).abs() < 1e-9, "{p:?}");
        }
        // Points run west to east, one per longitude.
        let lons: Vec<f64> = t[2..].chunks(2).map(|p| p[1]).collect();
        assert!(lons.windows(2).all(|w| w[1] > w[0]) && lons[0] == -180.0 && lons[89] == 180.0);
    }

    #[test]
    fn dawn_dusk_orbit_stays_in_sunlight() {
        let mut m = Model::new().unwrap();
        let dd = m.dawn_dusk(550.0).unwrap();
        assert!((dd[0] - 97.59).abs() < 0.02, "{dd:?}");
        m.set_orbit(550.0, 0.0, dd[0], dd[1], 0.0, 0.0).unwrap();
        // As in the orbital_datacenter example: no shadow all day.
        let track = m.ground_track(0.0, 86_400.0, 600).unwrap();
        assert!(track.chunks(4).all(|c| c[3] > 0.99));
        // Some site sees it at some point, and elevations are sane.
        let seen = (0..288).any(|k| {
            m.site_view(k as f64 * 300.0)
                .unwrap()
                .iter()
                .any(|e| *e > 10.0)
        });
        assert!(seen);
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
        assert!(orbit_path(100) == 400);
    }
}
