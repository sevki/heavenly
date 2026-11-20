//! An orbital data centre and Apple's data centres on the ground.
//!
//! Ground sites: Apple's own data centres as Apple lists them (Maiden,
//! Prineville, Reno, Mesa, Waukee, Viborg), at **city-level** coordinates.
//! These are approximate, not the facilities' exact positions.
//!
//! The orbital data centre is **illustrative**. Reports of SpaceX's "AI1"
//! data-centre satellite give only a range, "500 to 2,000 kilometers" and
//! "30 degrees to Sun-Synchronous Orbit", with no published orbit. This
//! picks one option from that range: a dawn–dusk sun-synchronous orbit at
//! 550 km, which rides the day–night line and stays almost always in
//! sunlight, which suits a solar-powered data centre. A 53° orbit at the
//! same height is shown for comparison. Both are two-body orbits over one
//! day.
use std::sync::Arc;

use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::id::{Attitude, Entity, IdError, Kind, MemoryRegistry, Mount, Registry};
use heavenly::orbit::{design, gm, ConicElements};
use heavenly::query::{
    eclipses, find_windows, look_angles, AnalyticSun, Occulter, SPEED_OF_LIGHT_M_S,
};
use heavenly::*;

/// (alias, name, latitude °, longitude °): city centres, approximate.
const APPLE_DATA_CENTRES: [(&str, &str, f64, f64); 6] = [
    (
        "dc:apple:maiden",
        "Maiden, North Carolina",
        35.5757,
        -81.2115,
    ),
    (
        "dc:apple:prineville",
        "Prineville, Oregon",
        44.2999,
        -120.8345,
    ),
    ("dc:apple:reno", "Reno, Nevada", 39.5296, -119.8138),
    ("dc:apple:mesa", "Mesa, Arizona", 33.4152, -111.8315),
    ("dc:apple:waukee", "Waukee, Iowa", 41.6117, -93.8852),
    ("dc:apple:viborg", "Viborg, Denmark", 56.4532, 9.4020),
];

fn main() -> Result<(), IdError> {
    let epoch = Epoch::parse("2026-11-20T00:00:00Z")?;
    let day = epoch.add_seconds(86_400.0)?;

    // ANCHOR: orbit
    // A dawn–dusk orbit: the ascending node 90° east of the Sun, so the
    // orbit plane faces the Sun, at the sun-synchronous inclination.
    let a = 6_378_137.0 + 550e3;
    let sun = AnalyticSun::position_m(epoch);
    let sun_ra = sun.y.atan2(sun.x);
    let orbit = |inclination_rad: f64, node_rad: f64| ConicElements {
        periapsis_m: a,
        eccentricity: 0.0,
        inclination_rad,
        ascending_node_rad: node_rad,
        argument_of_periapsis_rad: 0.0,
        mean_anomaly_rad: 0.0,
        epoch,
        gm_m3_s2: gm::EARTH,
    };
    let sso = orbit(
        design::sun_synchronous_inclination_rad(a, 0.0).expect("550 km has one"),
        sun_ra + std::f64::consts::FRAC_PI_2,
    );
    let shell = orbit(53f64.to_radians(), sun_ra);
    // ANCHOR_END: orbit

    // ANCHOR: registry
    let mut reg = MemoryRegistry::new(Namespace(7))?;
    let mut sites = Vec::new();
    for (k, (alias, name, lat, lon)) in APPLE_DATA_CENTRES.iter().enumerate() {
        let id = reg.add(Entity::new(
            LocationId(0x000A_991E_0000 + k as u128),
            Kind::Facility,
            *name,
            Mount::Geodetic {
                body_fixed: FrameId::ITRF,
                ellipsoid: Ellipsoid::WGS84,
                position: Geodetic::from_degrees(*lat, *lon, 0.0)?,
            },
        ))?;
        reg.alias(alias, id)?;
        sites.push(id);
    }
    for (alias, el, id) in [("sat:orbital-dc", sso, 1u128), ("sat:53-degree", shell, 2)] {
        let craft = reg.add(Entity::new(
            LocationId(0x5A7E_0000 + id),
            Kind::Spacecraft,
            alias,
            Mount::Trajectory {
                inertial: FrameId::GCRF,
                trajectory: Arc::new(move |t| el.state_at(t)),
                attitude: Attitude::Lvlh,
            },
        ))?;
        reg.alias(alias, craft)?;
    }
    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    reg.register_frames(&mut frames)?;
    // ANCHOR_END: registry

    println!(
        "Orbital data centre (illustrative): 550 km, dawn–dusk sun-synchronous, i = {:.2}°\n",
        sso.inclination_rad.to_degrees()
    );
    println!(
        "Sunlight over 24 hours from {}:",
        epoch.to_calendar(TimeScale::Utc)?
    );
    for alias in ["sat:orbital-dc", "sat:53-degree"] {
        let sat = reg.locator(alias)?;
        let found = eclipses::<_, _, _, IdError>(
            &Occulter::EARTH,
            &sat,
            &AnalyticSun,
            epoch,
            day,
            30.0,
            0.1,
            &frames,
        )?;
        let dark = found.total.iter().fold(0.0, |s, w| s + w.duration_s());
        println!(
            "  {alias:<16} {:2} eclipses, in the Earth's shadow {:4.1} % of the day",
            found.total.len(),
            100.0 * dark / 86_400.0
        );
    }

    // ANCHOR: contacts
    let sat = reg.locator("sat:orbital-dc")?;
    println!("\nContacts with the orbital data centre above 10°, over 24 hours:");
    println!(
        "  {:<24} {:>8} {:>10} {:>12} {:>14}",
        "data centre", "passes", "in view", "longest gap", "best one-way"
    );
    for id in &sites {
        let site = reg.frame_of(*id)?;
        let name = reg.resolve(*id)?.name.clone();
        let el = |t: Epoch| look_angles::<_, _, IdError>(site, &sat, t, &frames);
        let passes = find_windows(epoch, day, 20.0, 1e-2, |t| {
            Ok::<_, IdError>(el(t)?.elevation_rad - 10f64.to_radians())
        })?;
        let in_view = passes.iter().fold(0.0, |s, w| s + w.duration_s());
        let mut gaps = vec![passes
            .first()
            .map_or(86_400.0, |w| w.start.seconds_since(epoch))];
        gaps.extend(
            passes
                .windows(2)
                .map(|w| w[1].start.seconds_since(w[0].end)),
        );
        gaps.push(passes.last().map_or(0.0, |w| day.seconds_since(w.end)));
        let mut nearest = f64::INFINITY;
        for w in &passes {
            for k in 0..=60 {
                let t = w.start.add_seconds(w.duration_s() * k as f64 / 60.0)?;
                nearest = nearest.min(el(t)?.range_m);
            }
        }
        println!(
            "  {name:<24} {:>8} {:>8.0} m {:>10.1} h {:>11.2} ms",
            passes.len(),
            in_view / 60.0,
            gaps.iter().cloned().fold(0.0, f64::max) / 3600.0,
            nearest / SPEED_OF_LIGHT_M_S * 1e3
        );
    }
    // ANCHOR_END: contacts
    Ok(())
}
