//! Choosing a replica for a London rack: shared failure domains (policy)
//! and signal light time / visibility (physics) from one estate.
use std::sync::Arc;

use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::id::{Attitude, Entity, Kind, Mount, Registry};
use heavenly::orbit::{gm, ConicElements};
use heavenly::query::{find_windows, light_time, look_angles, Direction};
use heavenly::topology::{DomainKind, Topology};
use heavenly::*;

fn site(lat: f64, lon: f64) -> Result<Mount, Error> {
    Ok(Mount::Geodetic {
        body_fixed: FrameId::ITRF,
        ellipsoid: Ellipsoid::WGS84,
        position: Geodetic::from_degrees(lat, lon, 30.0)?,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;

    // ANCHOR: placement
    // Where things are: identities and frames (heavenly-id).
    let mut reg = Registry::new(Namespace(7))?;
    let lhr_a = reg.add(Entity::new(
        LocationId(1),
        Kind::Rack,
        "LHR zone A rack",
        site(51.507, -0.128)?,
    ))?;
    let lhr_b = reg.add(Entity::new(
        LocationId(2),
        Kind::Rack,
        "LHR zone B rack",
        site(51.47, -0.45)?,
    ))?;
    let fra = reg.add(Entity::new(
        LocationId(3),
        Kind::Rack,
        "Frankfurt rack",
        site(50.11, 8.68)?,
    ))?;
    let orbit = ConicElements {
        periapsis_m: 6_798_137.0,
        eccentricity: 0.0005,
        inclination_rad: 51.64f64.to_radians(),
        ascending_node_rad: 3.0,
        argument_of_periapsis_rad: 0.0,
        mean_anomaly_rad: 0.0,
        epoch,
        gm_m3_s2: gm::EARTH,
    };
    let sat = reg.add(Entity::new(
        LocationId(4),
        Kind::Spacecraft,
        "sat-42 rack",
        Mount::Trajectory {
            inertial: FrameId::GCRF,
            trajectory: Arc::new(move |t| orbit.state_at(t)),
            attitude: Attitude::Lvlh,
        },
    ))?;
    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    reg.register_frames(&mut frames)?;

    // What they share a fate with: policy (heavenly-topology).
    let mut topo = Topology::new();
    for (id, kind) in [
        ("zone:lhr-a", DomainKind::Zone),
        ("zone:lhr-b", DomainKind::Zone),
        ("zone:fra-1", DomainKind::Zone),
        ("grid:uk-south", DomainKind::Power),
        ("grid:de-hesse", DomainKind::Power),
        ("net:ground-uk", DomainKind::Network),
        ("constellation:example", DomainKind::Constellation),
        ("law:gb", DomainKind::Jurisdiction),
        ("law:eu", DomainKind::Jurisdiction),
    ] {
        topo.add_domain(id, kind, id)?;
    }
    for (a, b) in [
        ("zone:lhr-a", "grid:uk-south"),
        ("zone:lhr-b", "grid:uk-south"),
        ("zone:fra-1", "grid:de-hesse"),
        ("net:ground-uk", "grid:uk-south"),
        ("constellation:example", "net:ground-uk"),
        ("zone:lhr-a", "law:gb"),
        ("zone:lhr-b", "law:gb"),
        ("zone:fra-1", "law:eu"),
    ] {
        topo.depends_on(a, b)?;
    }
    topo.assign(lhr_a, "zone:lhr-a")?;
    topo.assign(lhr_b, "zone:lhr-b")?;
    topo.assign(fra, "zone:fra-1")?;
    topo.assign(sat, "constellation:example")?;

    // How fast a signal gets there: physics (heavenly-query).
    let primary = reg.locator(lhr_a)?;
    for candidate in [lhr_b, fra, sat] {
        let shared = topo.shared_failure_domains(lhr_a, candidate);
        let independent = topo.independent(lhr_a, candidate, &[DomainKind::Jurisdiction]);
        let lt = light_time::<_, _, _, Error>(
            &primary,
            &reg.locator(candidate)?,
            epoch,
            FrameId::GCRF,
            &frames,
            Direction::Transmission,
        )?;
        println!(
            "{:<16} independent: {:<5} shared: {:<40} one-way light time now: {:.3} ms",
            reg.resolve(candidate)?.name,
            independent,
            format!("{shared:?}"),
            lt.light_time_s * 1e3
        );
    }
    // ANCHOR_END: placement

    // The satellite is only reachable from London while it is above the horizon.
    let site_frame = reg.frame_of(lhr_a)?;
    let sat_loc = reg.locator(sat)?;
    let passes = find_windows(epoch, epoch.add_seconds(86400.0)?, 30.0, 1e-3, |t| {
        look_angles::<_, _, Error>(site_frame, &sat_loc, t, &frames)
            .map(|a| a.elevation_rad - 10f64.to_radians())
    })?;
    println!(
        "sat-42 passes above 10° from London in the next 24 h: {}",
        passes.len()
    );
    for w in passes.iter().take(3) {
        println!(
            "  {} for {:.0} s",
            w.start.to_calendar(TimeScale::Utc)?,
            w.duration_s()
        );
    }
    Ok(())
}
