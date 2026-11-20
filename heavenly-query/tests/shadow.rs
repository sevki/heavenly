//! Sunlight and eclipses: the analytic Sun against DE440, the shadow
//! geometry against brute-force sampling of the solar disk, and eclipse
//! intervals against SPICE `gfoclt` (see `tests/reference/generate_shadow.py`).

#[rustfmt::skip]
#[path = "reference/shadow_data.rs"]
mod data;

use std::path::PathBuf;
use std::sync::Arc;

use heavenly_core::{Epoch, Error, FrameGraph, FrameId, Locator, Vec3};
use heavenly_geo::earth;
use heavenly_orbit::{ConicElements, TwoBodyLocator};
use heavenly_query::{eclipses, AnalyticSun, Occulter, Shadow, ShadowKind, Window, SUN_RADIUS_M};

const ARCSEC: f64 = std::f64::consts::PI / 180.0 / 3600.0;

fn et(e: f64) -> Epoch {
    Epoch::from_tdb_seconds_since_j2000(e).unwrap()
}

#[test]
fn analytic_sun_matches_de440() {
    let (mut worst_dir, mut worst_dist) = (0.0f64, 0.0f64);
    for (e, p) in data::SUN {
        let ours = AnalyticSun::position_m(et(*e));
        let theirs = Vec3::from(*p) * 1e3;
        let dir = libm::atan2(ours.cross(theirs).norm(), ours.dot(theirs));
        worst_dir = worst_dir.max(dir);
        worst_dist = worst_dist.max((ours.norm() / theirs.norm() - 1.0).abs());
    }
    eprintln!(
        "AnalyticSun vs DE440, 1950-2050: worst {:.1}\" direction, {:.1e} distance",
        worst_dir / ARCSEC,
        worst_dist
    );
    assert!(worst_dir < 35.0 * ARCSEC && worst_dist < 1e-4);
}

/// The visible fraction of the solar disk, by sampling it on a fine grid
/// and testing each point against the body's disk: independent of the
/// lens-area formula.
fn brute_force_fraction(s: &Shadow) -> f64 {
    let (a, b, c) = (s.sun_radius_rad, s.body_radius_rad, s.separation_rad);
    let n = 600;
    let (mut inside, mut visible) = (0u64, 0u64);
    for i in 0..n {
        for j in 0..n {
            let x = a * (2.0 * (i as f64 + 0.5) / n as f64 - 1.0);
            let y = a * (2.0 * (j as f64 + 0.5) / n as f64 - 1.0);
            if x * x + y * y > a * a {
                continue;
            }
            inside += 1;
            if (x - c) * (x - c) + y * y > b * b {
                visible += 1;
            }
        }
    }
    visible as f64 / inside as f64
}

#[test]
fn shadow_geometry() {
    let sun = 0.0046; // rad, about the Sun's apparent radius
    let mut worst = 0.0f64;
    for (b, c, kind) in [
        (0.3, 0.5, ShadowKind::Sunlit),
        (0.3, 0.3, ShadowKind::Penumbra),
        (0.3, 0.2, ShadowKind::Umbra),
        (0.002, 0.001, ShadowKind::Antumbra),
        (0.004, 0.003, ShadowKind::Penumbra),
        (0.3, 0.3 + sun * 0.9, ShadowKind::Penumbra),
        (0.3, 0.3 - sun * 0.9, ShadowKind::Penumbra),
    ] {
        let s = Shadow {
            sun_radius_rad: sun,
            body_radius_rad: b,
            separation_rad: c,
        };
        assert_eq!(s.kind(), kind, "{b} {c}");
        let d = (s.sunlit_fraction() - brute_force_fraction(&s)).abs();
        worst = worst.max(d);
        assert!(
            d < 2e-3,
            "{b} {c}: {} vs {}",
            s.sunlit_fraction(),
            brute_force_fraction(&s)
        );
        // The margins say the same as the kind.
        assert_eq!(s.partial_margin() > 0.0, kind != ShadowKind::Sunlit);
        assert_eq!(s.total_margin() > 0.0, kind == ShadowKind::Umbra);
    }
    eprintln!("sunlit fraction vs disk sampling: worst {worst:.1e}");

    // Through the body: a point behind the Earth on the Sun line is in
    // umbra, one beside it in sunlight, one in front of it in sunlight.
    let sun_pos = Vec3::new(1.496e11, 0.0, 0.0);
    let e = Occulter::EARTH;
    assert_eq!(
        e.shadow_at(Vec3::new(-7e6, 0.0, 0.0), sun_pos).kind(),
        ShadowKind::Umbra
    );
    assert_eq!(
        e.shadow_at(Vec3::new(-7e6, 7e6, 0.0), sun_pos).kind(),
        ShadowKind::Sunlit
    );
    assert_eq!(
        e.shadow_at(Vec3::new(7e6, 0.0, 0.0), sun_pos).kind(),
        ShadowKind::Sunlit
    );
    // Beyond the tip of the umbra (≈1.38 million km) the Earth is annular.
    assert_eq!(
        e.shadow_at(Vec3::new(-2e9, 0.0, 0.0), sun_pos).kind(),
        ShadowKind::Antumbra
    );
    // Apparent radii: the Sun's is asin(R/d) from the point.
    let p = Vec3::new(-7e6, 0.0, 0.0);
    let s = e.shadow_at(p, sun_pos);
    let expect = (SUN_RADIUS_M / (sun_pos - p).norm()).asin();
    assert!((s.sun_radius_rad - expect).abs() < 1e-15);
    assert!((s.body_radius_rad - (6_378_137.0f64 / 7e6).asin()).abs() < 1e-15);
}

struct Case {
    name: &'static str,
    sat: TwoBodyLocator,
    start: Epoch,
    end: Epoch,
    any: Vec<(Epoch, Epoch)>,
    full: Vec<(Epoch, Epoch)>,
}

fn cases() -> Vec<Case> {
    data::ECLIPSES
        .iter()
        .map(|(name, el, s, e, any, full)| Case {
            name,
            sat: TwoBodyLocator {
                frame: FrameId::GCRF,
                elements: ConicElements {
                    periapsis_m: el[0] * 1e3,
                    eccentricity: el[1],
                    inclination_rad: el[2],
                    ascending_node_rad: el[3],
                    argument_of_periapsis_rad: el[4],
                    mean_anomaly_rad: el[5],
                    epoch: et(el[6]),
                    gm_m3_s2: el[7] * 1e9,
                },
            },
            start: et(*s),
            end: et(*e),
            any: any.iter().map(|(a, b)| (et(*a), et(*b))).collect(),
            full: full.iter().map(|(a, b)| (et(*a), et(*b))).collect(),
        })
        .collect()
}

/// The pck00011 Earth, as SPICE used it.
fn earth_like_spice() -> Occulter {
    Occulter {
        body_fixed: FrameId::ITRF,
        equatorial_radius_m: data::EARTH_RADII_KM.0 * 1e3,
        polar_radius_m: data::EARTH_RADII_KM.1 * 1e3,
    }
}

/// Compares eclipse edges with SPICE's, returning the worst difference in
/// seconds.
fn compare(label: &str, ours: &[Window], theirs: &[(Epoch, Epoch)], tol_s: f64) -> f64 {
    assert_eq!(
        ours.len(),
        theirs.len(),
        "{label}: {} intervals vs SPICE's {}",
        ours.len(),
        theirs.len()
    );
    let mut worst = 0.0f64;
    for (w, (a, b)) in ours.iter().zip(theirs) {
        let d = w
            .start
            .seconds_since(*a)
            .abs()
            .max(w.end.seconds_since(*b).abs());
        worst = worst.max(d);
        assert!(d < tol_s, "{label}: {:?} vs SPICE {a} – {b}: {d} s", w);
    }
    worst
}

fn check_eclipses<S>(sun: &S, graph: &FrameGraph, tol_s: f64, label: &str)
where
    S: Locator<Error = Error>,
{
    let body = earth_like_spice();
    for c in cases() {
        let step = if c.name == "GEO" { 60.0 } else { 30.0 };
        let found =
            eclipses::<_, _, _, Error>(&body, &c.sat, sun, c.start, c.end, step, 1e-4, graph)
                .unwrap();
        let p = compare(
            &format!("{label} {} partial", c.name),
            &found.partial,
            &c.any,
            tol_s,
        );
        let t = compare(
            &format!("{label} {} total", c.name),
            &found.total,
            &c.full,
            tol_s,
        );
        eprintln!(
            "{label} {}: {} partial, {} total eclipses; worst edge vs SPICE {:.2e} s, {:.2e} s",
            c.name,
            found.partial.len(),
            found.total.len(),
            p,
            t
        );
    }
}

#[test]
fn eclipses_with_the_analytic_sun_match_spice() {
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    // 32″ of Sun direction moves a shadow edge by ~0.1 s in low orbits and
    // ~2 s at geostationary distance, where the satellite crosses it slowly.
    check_eclipses(&AnalyticSun, &graph, 2.5, "AnalyticSun");
}

/// Run with `HEAVENLY_KERNELS=<dir with de440s.bsp> cargo test -- --ignored`.
#[test]
#[ignore = "needs JPL kernels; set HEAVENLY_KERNELS"]
fn eclipses_with_de440_match_spice() {
    use heavenly_spice::{frames, Spk, SpkLocator};
    let dir = PathBuf::from(std::env::var("HEAVENLY_KERNELS").expect("set HEAVENLY_KERNELS"));
    let spk = Arc::new(Spk::open(dir.join("de440s.bsp")).unwrap());
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    frames::register_ephemeris(&mut graph, Arc::clone(&spk), &[(FrameId::GCRF, 399)]).unwrap();
    let sun = SunInSsb(SpkLocator { spk, body: 10 });
    check_eclipses(&sun, &graph, 0.05, "DE440");
}

/// The SPK Sun, with errors as `heavenly_core::Error` like the other locators.
struct SunInSsb(heavenly_spice::SpkLocator);

impl Locator for SunInSsb {
    type Error = Error;
    fn state_at<R: heavenly_core::FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<heavenly_core::StateVector, Error>
    where
        Error: From<R::Error>,
    {
        let s = self
            .0
            .state_at(epoch, FrameId::SSB_ICRF, &FrameGraph::new())
            .map_err(|_| Error::InvalidInput("SPK lookup failed"))?;
        Ok(s.to_frame(frame, resolver)?)
    }
}
