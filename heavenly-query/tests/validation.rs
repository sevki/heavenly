//! Validation: light time against SPICE (`spkezr` with CN / XCN), line of
//! sight against analytic geometry and elevation, and window search against
//! brute-force sampling.

#[rustfmt::skip]
#[path = "reference/data.rs"]
mod data;

use std::path::PathBuf;
use std::sync::Arc;

use heavenly_core::{Epoch, Error, FixedLocator, FrameGraph, FrameId, Namespace, Vec3};
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_orbit::{gm, ConicElements, TwoBodyLocator};
use heavenly_query::{find_windows, light_time, look_angles, Direction, Occulter};
use heavenly_spice::{SpiceError, Spk, SpkLocator};

/// (target, observer, ET, correction, SPICE state [km, km/s], SPICE light time s)
type Row = (i32, i32, f64, &'static str, [f64; 6], f64);

fn check_light_time(spk: Arc<Spk>, refs: &[Row], pos_tol_m: f64) {
    let graph = FrameGraph::new(); // SSB_ICRF → SSB_ICRF needs no edges
    let mut worst = (0.0f64, 0.0f64, 0.0f64);
    for (target, observer, et, corr, st, lt) in refs {
        let t = SpkLocator {
            spk: Arc::clone(&spk),
            body: *target,
        };
        let o = SpkLocator {
            spk: Arc::clone(&spk),
            body: *observer,
        };
        let epoch = Epoch::from_tdb_seconds_since_j2000(*et).unwrap();
        let dir = if *corr == "CN" {
            Direction::Reception
        } else {
            Direction::Transmission
        };
        let got = light_time::<_, _, _, SpiceError>(&o, &t, epoch, FrameId::SSB_ICRF, &graph, dir)
            .unwrap();
        let p = Vec3::new(st[0], st[1], st[2]) * 1e3;
        let v = Vec3::new(st[3], st[4], st[5]) * 1e3;
        let range_rate = p.dot(v) / p.norm();
        let e = (
            (got.relative_position_m - p).norm(),
            (got.light_time_s - lt).abs(),
            (got.range_rate_m_s - range_rate).abs(),
        );
        worst = (worst.0.max(e.0), worst.1.max(e.1), worst.2.max(e.2));
        // SPICE forms `et ∓ lt` in f64, which resolves |et|·2⁻⁵² seconds
        // (0.6 µs around 1909), while Epoch is exact to 1 ns. Allow for that
        // quantisation times the speed of the bodies (≤ 60 km/s here).
        let spice_time_ulp = et.abs() * f64::EPSILON;
        let tol = pos_tol_m + 2.0 * spice_time_ulp * 60e3;
        assert!(
            e.0 < tol && e.1 < 1e-9 && e.2 < 1e-6,
            "{target} from {observer} {corr} at {et}: {e:?} (tol {tol:.2e})"
        );
    }
    eprintln!(
        "worst: {:.2e} m, {:.2e} s, {:.2e} m/s",
        worst.0, worst.1, worst.2
    );
}

#[test]
fn light_time_matches_spice_on_synthetic_kernel() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../heavenly-spice/tests/data/synthetic.bsp");
    check_light_time(Arc::new(Spk::open(path).unwrap()), data::SYNTHETIC, 1e-3);
}

/// Run with `HEAVENLY_KERNELS=<dir with de440s.bsp> cargo test -- --ignored`.
#[test]
#[ignore = "needs JPL kernels; set HEAVENLY_KERNELS"]
fn light_time_matches_spice_on_de440() {
    let dir = PathBuf::from(std::env::var("HEAVENLY_KERNELS").expect("set HEAVENLY_KERNELS"));
    check_light_time(
        Arc::new(Spk::open(dir.join("de440s.bsp")).unwrap()),
        data::DE440S,
        1e-3,
    );
}

#[test]
fn clearance_geometry() {
    let sphere = Occulter {
        body_fixed: FrameId::ITRF,
        equatorial_radius_m: 1.0,
        polar_radius_m: 1.0,
    };
    let c = |a: [f64; 3], b: [f64; 3]| sphere.clearance_m(Vec3::from(a), Vec3::from(b));
    assert!((c([-2.0, 0.5, 0.0], [2.0, 0.5, 0.0]) + 0.5).abs() < 1e-15); // passes 0.5 inside
    assert!((c([-2.0, 1.5, 0.0], [2.0, 1.5, 0.0]) - 0.5).abs() < 1e-15); // misses by 0.5
    assert!((c([2.0, 0.0, 0.0], [3.0, 0.0, 0.0]) - 1.0).abs() < 1e-15); // closest is an endpoint
    assert!(c([-2.0, 1.0, 0.0], [2.0, 1.0, 0.0]).abs() < 1e-15); // grazing
                                                                 // Oblate: a ray past the pole at 1.0 clears a body with polar radius 0.9.
    let oblate = Occulter {
        body_fixed: FrameId::ITRF,
        equatorial_radius_m: 1.0,
        polar_radius_m: 0.9,
    };
    assert!(oblate.clearance_m(Vec3::new(-2.0, 0.0, 0.95), Vec3::new(2.0, 0.0, 0.95)) > 0.0);
    assert!(oblate.clearance_m(Vec3::new(-2.0, 0.0, 0.85), Vec3::new(2.0, 0.0, 0.85)) < 0.0);
}

struct Scene {
    graph: FrameGraph,
    site: FrameId,
    station: FixedLocator,
    sat: TwoBodyLocator,
    start: Epoch,
}

fn scene(height_m: f64) -> Scene {
    let start = Epoch::parse("2026-11-19T00:00:00Z").unwrap();
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    let site = FrameId::custom(Namespace(3), 1).unwrap();
    let g = Geodetic::from_degrees(51.5072, -0.1276, height_m).unwrap();
    graph
        .add_fixed(site, FrameId::ITRF, g.enu_to_body_fixed(&Ellipsoid::WGS84))
        .unwrap();
    let sat = TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_798_137.0,
            eccentricity: 0.0005,
            inclination_rad: 51.64f64.to_radians(),
            ascending_node_rad: 3.0,
            argument_of_periapsis_rad: 0.0,
            mean_anomaly_rad: 0.0,
            epoch: start,
            gm_m3_s2: gm::EARTH,
        },
    };
    Scene {
        graph,
        site,
        station: FixedLocator {
            frame: site,
            position_m: Vec3::ZERO,
        },
        sat,
        start,
    }
}

#[test]
fn line_of_sight_agrees_with_elevation_for_a_site_on_the_surface() {
    // For a point on the ellipsoid, the geodetic horizon is the tangent
    // plane, so the Earth blocks the line of sight exactly when elevation < 0.
    let s = scene(0.0);
    let mut checked = 0;
    for k in 0..2880 {
        let t = s.start.add_seconds(k as f64 * 30.0).unwrap();
        let el = look_angles::<_, _, Error>(s.site, &s.sat, t, &s.graph)
            .unwrap()
            .elevation_rad;
        let clear = Occulter::EARTH
            .line_of_sight::<_, _, _, Error>(&s.station, &s.sat, t, &s.graph)
            .unwrap();
        if el.abs() > 1e-4 {
            assert_eq!(el > 0.0, clear > 0.0, "t={k} el={el} clearance={clear}");
            checked += 1;
        }
    }
    assert!(checked > 2800);
}

#[test]
fn windows_match_brute_force_sampling() {
    let s = scene(35.0);
    let mask = 10f64.to_radians();
    let elev = |t: Epoch| {
        look_angles::<_, _, Error>(s.site, &s.sat, t, &s.graph).map(|a| a.elevation_rad - mask)
    };
    let end = s.start.add_seconds(2.0 * 86400.0).unwrap();
    let windows = find_windows(s.start, end, 30.0, 1e-3, elev).unwrap();

    // Brute force: 1 s sampling.
    let mut brute = Vec::new();
    let mut open: Option<f64> = None;
    for k in 0..=(2 * 86400) {
        let up = elev(s.start.add_seconds(k as f64).unwrap()).unwrap() > 0.0;
        match (up, open) {
            (true, None) => open = Some(k as f64),
            (false, Some(a)) => {
                brute.push((a, k as f64));
                open = None;
            }
            _ => {}
        }
    }
    assert!(!windows.is_empty());
    assert_eq!(windows.len(), brute.len(), "{windows:?} vs {brute:?}");
    for (w, (a, b)) in windows.iter().zip(&brute) {
        let (ws, we) = (w.start.seconds_since(s.start), w.end.seconds_since(s.start));
        assert!(ws <= *a && *a - ws < 1.0 + 1e-3, "rise {ws} vs {a}");
        assert!(we < *b && *b - we <= 1.0 + 1e-3, "set {we} vs {b}");
        // Elevation is at the mask at both ends.
        assert!(elev(w.start).unwrap().abs() < 1e-5 && elev(w.end).unwrap().abs() < 1e-5);
    }
    eprintln!(
        "{} passes above 10° in 2 days, longest {:.0} s",
        windows.len(),
        windows.iter().map(|w| w.duration_s()).fold(0.0, f64::max)
    );

    assert!(find_windows(s.start, end, 0.0, 1e-3, elev).is_err());
    assert!(find_windows(end, s.start, 30.0, 1e-3, elev).is_err());
    // Always-true conditions give one clipped window.
    let all = find_windows::<Error>(s.start, end, 600.0, 1.0, |_| Ok(1.0)).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].start, all[0].end), (s.start, end));
}

#[test]
fn light_time_between_a_site_and_a_satellite() {
    let s = scene(35.0);
    let t = s.start.add_seconds(3600.0).unwrap();
    let rx = light_time::<_, _, _, Error>(
        &s.station,
        &s.sat,
        t,
        FrameId::GCRF,
        &s.graph,
        Direction::Reception,
    )
    .unwrap();
    let tx = light_time::<_, _, _, Error>(
        &s.station,
        &s.sat,
        t,
        FrameId::GCRF,
        &s.graph,
        Direction::Transmission,
    )
    .unwrap();
    let station = s.station.state_at_epoch(t, &s.graph);
    let sat = s.sat.state(t).unwrap();
    let geometric = station.separation_m(&sat).unwrap();
    // Light time is range / c. The observer end is fixed at t, so to first
    // order the corrected range differs from the geometric one by the
    // *target's* motion along the line of sight during τ.
    let u = (sat.position_m - station.position_m).normalized().unwrap();
    let along = u.dot(sat.velocity_m_s);
    assert!((rx.light_time_s - rx.range_m / heavenly_query::SPEED_OF_LIGHT_M_S).abs() < 1e-15);
    assert!(((rx.range_m - geometric) + along * rx.light_time_s).abs() < 1e-2);
    assert!(((tx.range_m - geometric) - along * tx.light_time_s).abs() < 1e-2);
    assert!(rx.iterations <= 5 && tx.iterations <= 5);
    assert_eq!(
        rx.target_epoch,
        t.add_tdb_seconds(-rx.light_time_s).unwrap()
    );
}

trait StateAt {
    fn state_at_epoch(&self, t: Epoch, graph: &FrameGraph) -> heavenly_core::StateVector;
}

impl StateAt for FixedLocator {
    fn state_at_epoch(&self, t: Epoch, graph: &FrameGraph) -> heavenly_core::StateVector {
        use heavenly_core::Locator;
        self.state_at(t, FrameId::GCRF, graph).unwrap()
    }
}
