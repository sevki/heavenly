//! Catalogue-scale searches, each against an independent method: contacts
//! against `find_windows` + `look_angles` per pair, conjunctions against
//! brute-force sampling every second, links against direct geometry.

use heavenly_core::{Epoch, Error, FrameGraph, FrameId, Locator, Namespace, Vec3};
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_orbit::{design, TwoBodyLocator};
use heavenly_query::catalog::{conjunctions, contacts, links};
use heavenly_query::{find_windows, look_angles, Occulter};

fn start() -> Epoch {
    Epoch::parse("2026-11-20T00:00:00Z").unwrap()
}

fn shell(
    total: usize,
    planes: usize,
    alt_km: f64,
    inc_deg: f64,
    node0_deg: f64,
) -> Vec<TwoBodyLocator> {
    design::walker_delta(
        total,
        planes,
        1,
        inc_deg.to_radians(),
        6_378_137.0 + alt_km * 1e3,
        node0_deg.to_radians(),
        start(),
    )
    .unwrap()
    .map(|elements| TwoBodyLocator {
        frame: FrameId::GCRF,
        elements,
    })
    .collect()
}

fn graph_with_sites(sites: &[(f64, f64)]) -> (FrameGraph, Vec<FrameId>) {
    let mut g = FrameGraph::new();
    earth::register(&mut g, earth::EarthOrientation::ZERO).unwrap();
    let frames = sites
        .iter()
        .enumerate()
        .map(|(k, (lat, lon))| {
            let f = FrameId::custom(Namespace(4), k as i64).unwrap();
            let p = Geodetic::from_degrees(*lat, *lon, 0.0).unwrap();
            g.add_fixed(f, FrameId::ITRF, p.enu_to_body_fixed(&Ellipsoid::WGS84))
                .unwrap();
            f
        })
        .collect();
    (g, frames)
}

#[test]
fn contacts_match_per_pair_window_search() {
    let sats = shell(12, 3, 550.0, 53.0, 0.0);
    let (g, sites) = graph_with_sites(&[(51.5, -0.13), (35.58, -81.21), (56.45, 9.40)]);
    let end = start().add_seconds(6.0 * 3600.0).unwrap();
    let mask = 10f64.to_radians();
    let pos = |t: Epoch, k: usize| {
        sats[k]
            .state_at(t, FrameId::ITRF, &g)
            .map(|s| Some(s.position_m))
    };
    let found = contacts::<_, Error>(
        &sites,
        FrameId::ITRF,
        &g,
        start(),
        end,
        30.0,
        1e-3,
        mask,
        |t| (0..sats.len()).map(|k| pos(t, k)).collect(),
        pos,
    )
    .unwrap();
    // The independent way: one window search per (site, satellite).
    let mut expected = Vec::new();
    for (s, site) in sites.iter().enumerate() {
        for (o, sat) in sats.iter().enumerate() {
            let w = find_windows::<Error>(start(), end, 30.0, 1e-3, |t| {
                Ok(look_angles::<_, _, Error>(*site, sat, t, &g)?.elevation_rad - mask)
            })
            .unwrap();
            expected.extend(w.into_iter().map(|w| (s, o, w)));
        }
    }
    assert!(found.len() > 10, "{}", found.len());
    assert_eq!(found.len(), expected.len());
    for c in &found {
        let (_, _, w) = expected
            .iter()
            .find(|(s, o, w)| {
                *s == c.site && *o == c.object && w.start.seconds_since(c.window.start).abs() < 5.0
            })
            .unwrap_or_else(|| panic!("{c:?} not found by the per-pair search"));
        assert!(
            w.start.seconds_since(c.window.start).abs() < 3e-3,
            "{c:?} vs {w:?}"
        );
        assert!(
            w.end.seconds_since(c.window.end).abs() < 3e-3,
            "{c:?} vs {w:?}"
        );
        assert!(c.max_elevation_rad > mask);
    }
}

#[test]
fn links_follow_range_and_the_earth() {
    let sats = shell(24, 2, 550.0, 53.0, 0.0);
    let g = graph_with_sites(&[]).0;
    let p: Vec<Option<Vec3>> = sats
        .iter()
        .map(|s| Some(s.state_at(start(), FrameId::ITRF, &g).unwrap().position_m))
        .collect();
    let earth = Occulter::EARTH;
    let found = links(&p, &earth, 5_000e3, 80e3);
    for a in 0..p.len() {
        for b in a + 1..p.len() {
            let (pa, pb) = (p[a].unwrap(), p[b].unwrap());
            let ok = (pb - pa).norm() <= 5_000e3 && earth.clearance_m(pa, pb) > 80e3;
            assert_eq!(ok, found.iter().any(|l| l.a == a && l.b == b), "{a}-{b}");
        }
    }
    // Neighbours in a plane (30° apart, ≈3 600 km) see each other; a
    // satellite and the one opposite it in its plane never do.
    assert!(found.iter().any(|l| l.a == 0 && l.b == 1));
    assert!(!found.iter().any(|l| l.a == 0 && l.b == 6));
    let mut with_gap = p.clone();
    with_gap[1] = None;
    assert!(!links(&with_gap, &earth, 5_000e3, 80e3)
        .iter()
        .any(|l| l.a == 1 || l.b == 1));
}

#[test]
fn conjunctions_match_brute_force() {
    // Two crossing shells one kilometre apart in height, as mega-
    // constellations have: they meet where their planes cross.
    let mut sats = shell(24, 4, 550.0, 53.0, 0.0);
    sats.extend(shell(24, 4, 551.0, 97.6, 25.0));
    let g = graph_with_sites(&[]).0;
    let end = start().add_seconds(6.0 * 3600.0).unwrap();
    let threshold = 100e3;
    let pos = |t: Epoch, k: usize| sats[k].state(t).map(|s| Some(s.position_m));
    let found = conjunctions::<Error>(
        start(),
        end,
        60.0,
        1e-3,
        threshold,
        |t| (0..sats.len()).map(|k| pos(t, k)).collect(),
        pos,
    )
    .unwrap();
    let _ = g;

    // Brute force: every pair every second; local minima under the threshold.
    let n = sats.len();
    let steps = 6 * 3600;
    let snap = |k: usize| -> Vec<Vec3> {
        let t = start().add_seconds(k as f64).unwrap();
        (0..n)
            .map(|i| sats[i].state(t).unwrap().position_m)
            .collect()
    };
    let (mut s0, mut s1) = (snap(0), snap(1));
    let mut brute = Vec::new();
    for k in 2..=steps {
        let s2 = snap(k);
        for a in 0..n {
            for b in a + 1..n {
                let d = |s: &Vec<Vec3>| (s[b] - s[a]).norm();
                let (d0, d1, d2) = (d(&s0), d(&s1), d(&s2));
                if d1 < d0 && d1 <= d2 && d1 < threshold {
                    brute.push((a, b, (k - 1) as f64, d1));
                }
            }
        }
        (s0, s1) = (s1, s2);
    }
    assert!(!brute.is_empty(), "the test needs some close approaches");
    assert_eq!(found.len(), brute.len(), "{found:?}\nvs\n{brute:?}");
    let mut worst = (0.0f64, 0.0f64);
    for (a, b, t, d) in &brute {
        let c = found
            .iter()
            .find(|c| c.a == *a && c.b == *b && (c.epoch.seconds_since(start()) - t).abs() <= 1.0)
            .unwrap_or_else(|| panic!("missed {a}-{b} at +{t} s ({d} m)"));
        // The refined miss is the true minimum: never above a 1 s sample
        // (beyond the 1 ms search tolerance, when a sample happens to land
        // on the minimum), and below it by at most the relative motion in
        // half a second.
        assert!(
            c.miss_distance_m <= d + 1e-2,
            "{c:?}: refined {} m vs brute-force sample {d} m",
            c.miss_distance_m
        );
        assert!(d - c.miss_distance_m < 0.5 * 16e3, "{c:?} vs {d}");
        worst = (
            worst.0.max((c.epoch.seconds_since(start()) - t).abs()),
            worst.1.max(d - c.miss_distance_m),
        );
    }
    eprintln!(
        "{} conjunctions under {} km; vs 1 s brute force: ≤ {:.2} s, refined ≤ {:.0} m closer",
        found.len(),
        threshold / 1e3,
        worst.0,
        worst.1
    );
}
