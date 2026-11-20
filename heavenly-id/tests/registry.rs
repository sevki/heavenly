use std::sync::Arc;

use heavenly_core::{Epoch, Error, FrameGraph, FrameId, Namespace, Rotation, Vec3};
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_id::{Alias, Attitude, Entity, IdError, Kind, LocationId, Mount, Registry};
use heavenly_orbit::{gm, ConicElements, TwoBodyLocator};

fn epoch() -> Epoch {
    Epoch::parse("2026-11-19T17:00:00Z").unwrap()
}

#[test]
fn alias_syntax() {
    for ok in [
        "dc:lhr:01",
        "earth.surface.gb-lon.01",
        "sat:example:42/module-2/rack_1",
        "a",
    ] {
        assert!(Alias::new(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "DC:lhr",
        "1dc",
        "dc::lhr",
        "dc:lhr:",
        "dc lhr",
        "dc:lhr/é",
        &"a".repeat(129),
    ] {
        assert!(
            matches!(Alias::new(bad), Err(IdError::InvalidAlias { .. })),
            "{bad:?}"
        );
    }
    let a = Alias::new("sat:example:42/module-2").unwrap();
    assert_eq!(
        a.segments().collect::<Vec<_>>(),
        ["sat", "example", "42", "module-2"]
    );
}

fn site_mount() -> Mount {
    Mount::Geodetic {
        body_fixed: FrameId::ITRF,
        ellipsoid: Ellipsoid::WGS84,
        position: Geodetic::from_degrees(51.5072, -0.1276, 35.0).unwrap(),
    }
}

#[test]
fn aliases_are_separate_from_identity() {
    let mut reg = Registry::new(Namespace(7)).unwrap();
    let a = reg
        .add(Entity::new(
            LocationId(1),
            Kind::Facility,
            "LHR1",
            site_mount(),
        ))
        .unwrap();
    let b = reg
        .add(Entity::new(
            LocationId(2),
            Kind::Facility,
            "LHR2",
            site_mount(),
        ))
        .unwrap();
    let frame_a = reg.frame_of(a).unwrap();

    reg.alias("dc:lhr:01", a).unwrap();
    reg.alias("dc:lhr:01", a).unwrap(); // idempotent
    assert_eq!(
        reg.alias("dc:lhr:01", b),
        Err(IdError::DuplicateAlias {
            alias: "dc:lhr:01".into(),
            existing: a
        })
    );
    assert_eq!(
        reg.alias("dc:lhr:03", LocationId(99)),
        Err(IdError::UnknownId(LocationId(99)))
    );

    // Re-pointing and renaming change what an alias names, never an ID or frame.
    assert_eq!(reg.repoint("dc:lhr:01", b), Ok(a));
    assert_eq!(reg.id_of("dc:lhr:01"), Ok(b));
    reg.rename("dc:lhr:01", "dc:lhr:primary").unwrap();
    assert_eq!(reg.id_of("dc:lhr:primary"), Ok(b));
    assert_eq!(
        reg.id_of("dc:lhr:01"),
        Err(IdError::UnknownAlias("dc:lhr:01".into()))
    );
    assert_eq!(reg.frame_of(a).unwrap(), frame_a);

    reg.alias("legacy.lhr", b).unwrap();
    assert_eq!(reg.aliases_of(b).len(), 2);
    assert_eq!(reg.unalias("legacy.lhr"), Ok(b));
    assert_eq!(reg.resolve("dc:lhr:primary").unwrap().name, "LHR2");
    assert_eq!(reg.resolve(a).unwrap().name, "LHR1");

    assert_eq!(
        reg.add(Entity::new(LocationId(1), Kind::Rack, "dup", Mount::None))
            .unwrap_err(),
        IdError::DuplicateId(a)
    );
    // Removing an entity removes its aliases.
    reg.remove(b).unwrap();
    assert!(matches!(
        reg.resolve("dc:lhr:primary"),
        Err(IdError::UnknownAlias(_))
    ));
}

#[test]
fn frames_are_derived_from_ids() {
    let mut r1 = Registry::new(Namespace(7)).unwrap();
    let mut r2 = Registry::new(Namespace(7)).unwrap();
    let id = LocationId(0xdead_beef_0000_1111_2222_3333_4444_5555);
    r1.add(Entity::new(id, Kind::Rack, "a", site_mount()))
        .unwrap();
    r2.add(Entity::new(
        LocationId(5),
        Kind::Rack,
        "other",
        site_mount(),
    ))
    .unwrap();
    r2.add(Entity::new(id, Kind::Rack, "a", site_mount()))
        .unwrap();
    assert_eq!(r1.frame_of(id).unwrap(), r2.frame_of(id).unwrap());
    assert_eq!(r1.frame_of(id).unwrap().namespace(), Namespace(7));

    // Two IDs that fold to the same code are rejected rather than aliased.
    let mut r = Registry::new(Namespace(7)).unwrap();
    r.add(Entity::new(LocationId(1), Kind::Rack, "a", site_mount()))
        .unwrap();
    assert!(matches!(
        r.add(Entity::new(
            LocationId(1u128 << 64),
            Kind::Rack,
            "b",
            site_mount()
        )),
        Err(IdError::FrameCollision { .. })
    ));
    assert!(Registry::new(Namespace::HEAVENLY).is_err());
}

#[test]
fn hierarchy_errors_are_explicit() {
    let mut reg = Registry::new(Namespace(7)).unwrap();
    let region = reg
        .add(Entity::new(
            LocationId(1),
            Kind::Other("region".into()),
            "Europe",
            Mount::None,
        ))
        .unwrap();
    assert_eq!(reg.frame_of(region), Err(IdError::NotLocatable(region)));

    // Mounted in a parent that has no frame.
    reg.add(Entity::new(
        LocationId(2),
        Kind::Rack,
        "r",
        Mount::in_entity(region, Rotation::IDENTITY, Vec3::ZERO),
    ))
    .unwrap();
    assert_eq!(reg.validate(), Err(IdError::NotLocatable(region)));
    reg.remove(LocationId(2)).unwrap();

    // Missing parent.
    reg.add(Entity::new(
        LocationId(3),
        Kind::Rack,
        "r",
        Mount::in_entity(LocationId(42), Rotation::IDENTITY, Vec3::ZERO),
    ))
    .unwrap();
    assert_eq!(
        reg.validate(),
        Err(IdError::MissingParent {
            entity: LocationId(3),
            parent: LocationId(42)
        })
    );
    reg.remove(LocationId(3)).unwrap();

    // Cycle.
    reg.add(Entity::new(
        LocationId(4),
        Kind::Room,
        "a",
        Mount::in_entity(LocationId(5), Rotation::IDENTITY, Vec3::ZERO),
    ))
    .unwrap();
    reg.add(Entity::new(
        LocationId(5),
        Kind::Room,
        "b",
        Mount::in_entity(LocationId(4), Rotation::IDENTITY, Vec3::ZERO),
    ))
    .unwrap();
    assert!(matches!(reg.validate(), Err(IdError::Cycle(_))));
    assert!(matches!(
        reg.register_frames(&mut FrameGraph::new()),
        Err(IdError::Cycle(_))
    ));
}

/// Site → building (rotated 30°) → room (rotated 90°) → rack → device.
fn data_centre() -> (Registry, [LocationId; 5]) {
    let mut reg = Registry::new(Namespace(7)).unwrap();
    let rz = |deg: f64| Rotation::from_axis_angle(Vec3::Z, deg.to_radians());
    let site = reg
        .add(Entity::new(
            LocationId(10),
            Kind::Site,
            "London LHR1",
            site_mount(),
        ))
        .unwrap();
    let building = reg
        .add(Entity::new(
            LocationId(11),
            Kind::Building,
            "Building A",
            Mount::in_entity(site, rz(30.0), Vec3::new(120.0, -40.0, 0.0)),
        ))
        .unwrap();
    let room = reg
        .add(Entity::new(
            LocationId(12),
            Kind::Room,
            "Hall 2",
            Mount::in_entity(building, rz(90.0), Vec3::new(10.0, 20.0, 4.5)),
        ))
        .unwrap();
    let rack = reg
        .add(Entity::new(
            LocationId(13),
            Kind::Rack,
            "Rack 7",
            Mount::in_entity(room, Rotation::IDENTITY, Vec3::new(3.0, 7.2, 0.0)),
        ))
        .unwrap();
    let device = reg
        .add(Entity::new(
            LocationId(14),
            Kind::Device,
            "Server 7-33",
            Mount::in_entity(rack, Rotation::IDENTITY, Vec3::new(0.3, 0.4, 1.47)),
        ))
        .unwrap();
    reg.alias("dc:lhr:01", site).unwrap();
    reg.alias("dc:lhr:01/hall-2/rack-07/u33", device).unwrap();
    (reg, [site, building, room, rack, device])
}

#[test]
fn nested_frames_compose_exactly() {
    let (reg, [site, building, room, rack, device]) = data_centre();
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    reg.register_frames(&mut graph).unwrap();

    let names: Vec<_> = reg
        .path(device)
        .unwrap()
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "London LHR1",
            "Building A",
            "Hall 2",
            "Rack 7",
            "Server 7-33"
        ]
    );
    assert_eq!(
        reg.children(room).iter().map(|e| e.id).collect::<Vec<_>>(),
        [rack]
    );

    // By hand: compose the offsets down the chain in the site's ENU frame.
    let rz = |deg: f64| Rotation::from_axis_angle(Vec3::Z, deg.to_radians());
    let in_room = Vec3::new(3.0, 7.2, 0.0) + Vec3::new(0.3, 0.4, 1.47);
    let in_building = rz(90.0).apply(in_room) + Vec3::new(10.0, 20.0, 4.5);
    let in_site = rz(30.0).apply(in_building) + Vec3::new(120.0, -40.0, 0.0);
    let enu = Geodetic::from_degrees(51.5072, -0.1276, 35.0)
        .unwrap()
        .enu_to_body_fixed(&Ellipsoid::WGS84);
    let expected_itrf = enu.apply_position(in_site);

    let s = reg
        .locate(
            "dc:lhr:01/hall-2/rack-07/u33",
            epoch(),
            FrameId::ITRF,
            &graph,
        )
        .unwrap();
    assert!(
        (s.position_m - expected_itrf).norm() < 1e-6,
        "{:?}",
        s.position_m - expected_itrf
    );
    assert!(s.velocity_m_s.norm() < 1e-9);

    // In GCRF it moves with the Earth's rotation (~290 m/s at London).
    let g = reg.locate(device, epoch(), FrameId::GCRF, &graph).unwrap();
    assert!(
        (g.velocity_m_s.norm() - 289.6).abs() < 1.0,
        "{}",
        g.velocity_m_s.norm()
    );
    // Distance between two entities is frame-independent.
    let a = reg.locate(site, epoch(), FrameId::GCRF, &graph).unwrap();
    let a2 = reg.locate(site, epoch(), FrameId::ITRF, &graph).unwrap();
    assert!((a.separation_m(&g).unwrap() - a2.separation_m(&s).unwrap()).abs() < 1e-6);
    let _ = building;
}

fn iss(start: Epoch) -> TwoBodyLocator {
    TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_798_137.0,
            eccentricity: 0.0,
            inclination_rad: 51.64f64.to_radians(),
            ascending_node_rad: 1.0,
            argument_of_periapsis_rad: 0.0,
            mean_anomaly_rad: 0.0,
            epoch: start,
            gm_m3_s2: gm::EARTH,
        },
    }
}

fn spacecraft(reg: &mut Registry, start: Epoch) -> (LocationId, LocationId, LocationId) {
    let orbit = iss(start);
    let craft = reg
        .add(Entity::new(
            LocationId(100),
            Kind::Spacecraft,
            "sat-42",
            Mount::Trajectory {
                inertial: FrameId::GCRF,
                trajectory: Arc::new(move |e| orbit.elements.state_at(e)),
                attitude: Attitude::Lvlh,
            },
        ))
        .unwrap();
    // A module 10 m "below" the origin (towards the Earth, +z in LVLH) and
    // 5 m ahead (+x), with a rack inside it.
    let module = reg
        .add(Entity::new(
            LocationId(101),
            Kind::Module,
            "module-2",
            Mount::in_entity(craft, Rotation::IDENTITY, Vec3::new(5.0, 0.0, 10.0)),
        ))
        .unwrap();
    let rack = reg
        .add(Entity::new(
            LocationId(102),
            Kind::Rack,
            "rack-1",
            Mount::in_entity(module, Rotation::IDENTITY, Vec3::new(0.0, 1.0, 0.0)),
        ))
        .unwrap();
    reg.alias("sat:example:42", craft).unwrap();
    reg.alias("sat:example:42/module-2/rack-1", rack).unwrap();
    (craft, module, rack)
}

#[test]
fn lvlh_spacecraft_frames() {
    let start = epoch();
    let mut reg = Registry::new(Namespace(7)).unwrap();
    let (craft, module, _) = spacecraft(&mut reg, start);
    let mut graph = FrameGraph::new();
    reg.register_frames(&mut graph).unwrap();

    let t = start.add_seconds(1234.0).unwrap();
    let (r, v) = iss(start).elements.state_at(t).unwrap();
    let c = reg.locate(craft, t, FrameId::GCRF, &graph).unwrap();
    assert!((c.position_m - r).norm() < 1e-6 && (c.velocity_m_s - v).norm() < 1e-9);

    // +z is nadir and +x is along-track for a circular orbit.
    let m = reg.locate(module, t, FrameId::GCRF, &graph).unwrap();
    let expected = r - r.normalized().unwrap() * 10.0 + v.normalized().unwrap() * 5.0;
    assert!(
        (m.position_m - expected).norm() < 1e-6,
        "{:?}",
        m.position_m - expected
    );

    // The module's velocity is the derivative of its position: this checks
    // the LVLH angular velocity (its ω×Δ term is ~0.013 m/s here), by central
    // differences. Truncation error is h²/6·|r⃛| ≈ 4e-6 m/s at h = 0.05 s.
    let h = 0.05;
    let p = |dt: f64| {
        reg.locate(module, t.add_seconds(dt).unwrap(), FrameId::GCRF, &graph)
            .unwrap()
            .position_m
    };
    let numeric = (p(h) - p(-h)) / (2.0 * h);
    assert!(
        (numeric - m.velocity_m_s).norm() < 1e-5,
        "{:?}",
        numeric - m.velocity_m_s
    );
}

#[test]
fn design_doc_scenario_london_rack_to_orbiting_rack() {
    // From the design proposal: resolve "dc:lhr:01" and "sat:example:42" (here
    // a rack in each) and compute their separation and relative velocity.
    let start = epoch();
    let (mut reg, _) = data_centre();
    spacecraft(&mut reg, start);
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    reg.register_frames(&mut graph).unwrap();

    let mut closest = f64::INFINITY;
    for minutes in 0..(24 * 60) {
        let t = start.add_seconds(minutes as f64 * 60.0).unwrap();
        let a = reg
            .locate("dc:lhr:01/hall-2/rack-07/u33", t, FrameId::GCRF, &graph)
            .unwrap();
        let b = reg
            .locate("sat:example:42/module-2/rack-1", t, FrameId::GCRF, &graph)
            .unwrap();
        let d = a.separation_m(&b).unwrap();
        // Bounded by the geometry: altitude below, Earth's diameter plus orbit above.
        assert!(d > 400e3 && d < 6378e3 + 6798e3 + 1e3, "{d}");
        assert!(a.relative_speed_m_s(&b).unwrap() < 8.5e3);
        closest = closest.min(d);
    }
    // A 51.6° orbit passes near London's latitude at least once a day.
    assert!(closest < 2000e3, "{closest}");
}

#[test]
fn trajectory_errors_propagate() {
    let mut reg = Registry::new(Namespace(7)).unwrap();
    reg.add(Entity::new(
        LocationId(1),
        Kind::Spacecraft,
        "lost",
        Mount::Trajectory {
            inertial: FrameId::GCRF,
            trajectory: Arc::new(|e| {
                Err(Error::OutsideCoverage {
                    epoch: e,
                    start: Epoch::J2000,
                    end: Epoch::J2000,
                })
            }),
            attitude: Attitude::Inertial(Rotation::IDENTITY),
        },
    ))
    .unwrap();
    let mut graph = FrameGraph::new();
    reg.register_frames(&mut graph).unwrap();
    assert!(matches!(
        reg.locate(LocationId(1), epoch(), FrameId::GCRF, &graph),
        Err(IdError::Core(Error::OutsideCoverage { .. }))
    ));
}
