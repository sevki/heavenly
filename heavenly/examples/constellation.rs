//! A small constellation, at catalogue scale: coverage of Apple's data
//! centres, the inter-satellite link network, and close approaches with a
//! crossing shell.
//!
//! The shells are illustrative Walker constellations, not real satellites.
//! The data centres are at city-level, approximate coordinates, as in the
//! `orbital_datacenter` example.
use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::orbit::{design, TwoBodyLocator};
use heavenly::query::catalog::{conjunctions, contacts, links};
use heavenly::query::Occulter;
use heavenly::*;

const SITES: [(&str, f64, f64); 6] = [
    ("Maiden", 35.5757, -81.2115),
    ("Prineville", 44.2999, -120.8345),
    ("Reno", 39.5296, -119.8138),
    ("Mesa", 33.4152, -111.8315),
    ("Waukee", 41.6117, -93.8852),
    ("Viborg", 56.4532, 9.4020),
];

fn main() -> Result<(), Error> {
    let start = Epoch::parse("2026-11-20T00:00:00Z")?;
    let day = start.add_seconds(86_400.0)?;
    let a = 6_378_137.0 + 550e3;
    let shell = |total, planes, inc_deg: f64, a: f64, node_deg: f64| -> Vec<TwoBodyLocator> {
        design::walker_delta(
            total,
            planes,
            1,
            inc_deg.to_radians(),
            a,
            node_deg.to_radians(),
            start,
        )
        .expect("valid Walker pattern")
        .map(|elements| TwoBodyLocator {
            frame: FrameId::GCRF,
            elements,
        })
        .collect()
    };
    // ANCHOR: snapshot
    // 53°: 24/4/1 at 550 km.
    let sats = shell(24, 4, 53.0, a, 0.0);
    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    // Ground sites as east-north-up frames.
    let mut site_frames = Vec::new();
    for (k, (_, lat, lon)) in SITES.iter().enumerate() {
        let f = FrameId::custom(Namespace(2), k as i64).unwrap();
        let g = Geodetic::from_degrees(*lat, *lon, 0.0)?;
        frames.add_fixed(f, FrameId::ITRF, g.enu_to_body_fixed(&Ellipsoid::WGS84))?;
        site_frames.push(f);
    }
    // Every satellite at once in ITRF: propagate each in GCRF, then move
    // them all with one Earth-orientation transform (SIMD with `simd`).
    let snapshot = |t: Epoch| -> Result<Vec<Option<Vec3>>, Error> {
        let mut states = sats
            .iter()
            .map(|s| s.state(t))
            .collect::<Result<Vec<_>, _>>()?;
        StateVector::to_frame_batch(&mut states, FrameId::ITRF, &frames)?;
        Ok(states.into_iter().map(|s| Some(s.position_m)).collect())
    };
    let one = |t: Epoch, k: usize| {
        sats[k]
            .state_at(t, FrameId::ITRF, &frames)
            .map(|s| Some(s.position_m))
    };
    // ANCHOR_END: snapshot

    println!("Shell: 24 satellites, 4 planes, 53°, 550 km (Walker 24/4/1)\n");

    // ANCHOR: links
    let isl = links(&snapshot(start)?, &Occulter::EARTH, 5_000e3, 80e3);
    let mut degree = [0usize; 24];
    for l in &isl {
        degree[l.a] += 1;
        degree[l.b] += 1;
    }
    println!(
        "Inter-satellite links at {} (≤ 5 000 km, clearing the Earth by 80 km): {} links, {}–{} per satellite",
        start.to_calendar(TimeScale::Utc)?,
        isl.len(),
        degree.iter().min().unwrap(),
        degree.iter().max().unwrap()
    );
    // ANCHOR_END: links

    // ANCHOR: coverage
    let mask = 25f64.to_radians();
    let found = contacts::<_, Error>(
        &site_frames,
        FrameId::ITRF,
        &frames,
        start,
        day,
        30.0,
        0.1,
        mask,
        snapshot,
        one,
    )?;
    println!("\nCoverage of Apple's data centres over 24 h (a satellite above 25°):");
    for (s, (name, ..)) in SITES.iter().enumerate() {
        // Merge the windows of every satellite over this site.
        let mut w: Vec<(f64, f64)> = found
            .iter()
            .filter(|c| c.site == s)
            .map(|c| {
                (
                    c.window.start.seconds_since(start),
                    c.window.end.seconds_since(start),
                )
            })
            .collect();
        w.sort_by(|x, y| x.0.total_cmp(&y.0));
        let (mut covered, mut longest_gap, mut reach) = (0.0f64, 0.0f64, 0.0f64);
        for (a, b) in w {
            longest_gap = longest_gap.max(a - reach);
            covered += (b - reach.max(a)).max(0.0);
            reach = reach.max(b);
        }
        longest_gap = longest_gap.max(86_400.0 - reach);
        println!(
            "  {name:<11} {:5.1} % of the day, longest gap {:5.1} min",
            100.0 * covered / 86_400.0,
            longest_gap / 60.0
        );
    }
    // ANCHOR_END: coverage

    // ANCHOR: conjunctions
    // A crossing shell: 24 satellites in sun-synchronous planes 1 km higher.
    let mut both = sats.clone();
    let sso = design::sun_synchronous_inclination_rad(a + 1e3, 0.0)
        .unwrap()
        .to_degrees();
    both.extend(shell(24, 4, sso, a + 1e3, 25.0));
    let pos = |t: Epoch, k: usize| both[k].state(t).map(|s| Some(s.position_m));
    let six_hours = start.add_seconds(6.0 * 3600.0)?;
    let close = conjunctions::<Error>(
        start,
        six_hours,
        60.0,
        1e-3,
        100e3,
        |t| (0..both.len()).map(|k| pos(t, k)).collect(),
        pos,
    )?;
    // Pairs within one shell, and pairs across the two.
    let (inner, polar, across): (Vec<_>, Vec<_>, Vec<_>) = close.iter().fold(
        (Vec::new(), Vec::new(), Vec::new()),
        |(mut i, mut p, mut x), c| {
            match (c.a < 24, c.b < 24) {
                (true, true) => i.push(c),
                (false, false) => p.push(c),
                _ => x.push(c),
            }
            (i, p, x)
        },
    );
    println!(
        "\nClose approaches under 100 km over 6 h, with a crossing {sso:.1}° shell 1 km higher:"
    );
    println!("  within the 53° shell:                {}", inner.len());
    println!(
        "  within the {sso:.1}° shell:              {} (its planes all cross near the poles, and a\n    phasing of 1 brings satellites of different planes {:.1} km apart there)",
        polar.len(),
        polar.iter().map(|c| c.miss_distance_m).fold(f64::INFINITY, f64::min) / 1e3
    );
    println!(
        "  between the two shells:              {}, the closest:",
        across.len()
    );
    let mut across = across;
    across.sort_by(|x, y| x.miss_distance_m.total_cmp(&y.miss_distance_m));
    for c in across.iter().take(3) {
        println!(
            "  sat {:2} and sat {:2} at {}: {:6.0} m",
            c.a,
            c.b,
            c.epoch.to_calendar(TimeScale::Utc)?,
            c.miss_distance_m
        );
    }
    // ANCHOR_END: conjunctions
    Ok(())
}
