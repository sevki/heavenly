//! The design proposal's example: a rack in a London data centre and a rack
//! on an orbiting satellite, resolved by alias and compared.
use std::sync::Arc;

use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::id::{Attitude, Entity, IdError, Kind, MemoryRegistry, Mount, Registry};
use heavenly::orbit::{gm, ConicElements};
use heavenly::*;

fn main() -> Result<(), IdError> {
    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;

    // ANCHOR: registry
    let mut reg = MemoryRegistry::new(Namespace(7))?;

    // A data centre on the ground: site (ENU) → hall → rack.
    let site = reg.add(Entity::new(
        LocationId::parse("6f1c0b4e-2a43-4f8e-9d1b-5c0e7a3b9f12")?,
        Kind::Facility,
        "London LHR1",
        Mount::Geodetic {
            body_fixed: FrameId::ITRF,
            ellipsoid: Ellipsoid::WGS84,
            position: Geodetic::from_degrees(51.5072, -0.1276, 35.0)?,
        },
    ))?;
    let hall = reg.add(Entity::new(
        LocationId::parse("0d7e3f2a-9b14-4c6d-8e5f-1a2b3c4d5e6f")?,
        Kind::Room,
        "Hall 2",
        Mount::in_entity(
            site,
            Rotation::from_axis_angle(Vec3::Z, 0.52),
            Vec3::new(120.0, -40.0, 4.5),
        ),
    ))?;
    let ground_rack = reg.add(Entity::new(
        LocationId::parse("9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d")?,
        Kind::Rack,
        "Rack 7",
        Mount::in_entity(hall, Rotation::IDENTITY, Vec3::new(3.0, 7.2, 0.0)),
    ))?;

    // A spacecraft: body frame in LVLH, following a two-body orbit → module → rack.
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
    let craft = reg.add(Entity::new(
        LocationId::parse("3c2b1a09-8f7e-4d6c-9b5a-4f3e2d1c0b0a")?,
        Kind::Spacecraft,
        "sat-42",
        Mount::Trajectory {
            inertial: FrameId::GCRF,
            trajectory: Arc::new(move |t| orbit.state_at(t)),
            attitude: Attitude::Lvlh,
        },
    ))?;
    let module = reg.add(Entity::new(
        LocationId::parse("5e4d3c2b-1a09-4f8e-8d7c-6b5a4f3e2d1c")?,
        Kind::Module,
        "Module 2",
        Mount::in_entity(craft, Rotation::IDENTITY, Vec3::new(4.0, 0.0, 1.5)),
    ))?;
    let space_rack = reg.add(Entity::new(
        LocationId::parse("7c6b5a4f-3e2d-4c0b-9a09-8f7e6d5c4b3a")?,
        Kind::Rack,
        "Rack 1",
        Mount::in_entity(module, Rotation::IDENTITY, Vec3::new(0.0, 0.8, 0.0)),
    ))?;

    // Aliases are names, not identities: they can change later.
    reg.alias("dc:lhr:01", site)?;
    reg.alias("dc:lhr:01/hall-2/rack-07", ground_rack)?;
    reg.alias("sat:example:42", craft)?;
    reg.alias("sat:example:42/module-2/rack-1", space_rack)?;

    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    reg.register_frames(&mut frames)?;

    let a = reg.locate("dc:lhr:01/hall-2/rack-07", epoch, FrameId::GCRF, &frames)?;
    let b = reg.locate(
        "sat:example:42/module-2/rack-1",
        epoch,
        FrameId::GCRF,
        &frames,
    )?;
    let separation_m = a.separation_m(&b)?;
    let relative_speed_m_s = a.relative_speed_m_s(&b)?;
    // ANCHOR_END: registry

    let path: Vec<_> = reg
        .path(space_rack)?
        .into_iter()
        .map(|e| e.name.clone())
        .collect();
    println!("sat:example:42/module-2/rack-1 = {}", path.join(" / "));
    println!(
        "at {epoch}: separation {:.1} km, relative speed {:.3} km/s",
        separation_m / 1e3,
        relative_speed_m_s / 1e3
    );

    // Closest approach over the next day, minute by minute.
    let (mut best, mut when) = (f64::INFINITY, epoch);
    for m in 0..1440 {
        let t = epoch.add_seconds(m as f64 * 60.0)?;
        let a = reg.locate(ground_rack, t, FrameId::GCRF, &frames)?;
        let b = reg.locate(space_rack, t, FrameId::GCRF, &frames)?;
        let d = a.separation_m(&b)?;
        if d < best {
            (best, when) = (d, t);
        }
    }
    println!(
        "closest in 24 h: {:.1} km at {}",
        best / 1e3,
        when.to_calendar(TimeScale::Utc)?
    );
    Ok(())
}
