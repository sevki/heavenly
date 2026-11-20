//! Facilities on the Earth and the Moon, resolved into inertial frames.
use heavenly::geo::{earth, iau, Ellipsoid, Geodetic};
use heavenly::*;

fn main() -> Result<(), Error> {
    // ANCHOR: sites
    let mut frames = FrameGraph::new();
    // ITRF → GCRF, with Earth orientation parameters from IERS Bulletin A
    // (UT1 − UTC in seconds; polar motion x, y in arcseconds).
    earth::register(
        &mut frames,
        earth::EarthOrientation::from_bulletin(0.0712, 0.1683, 0.3021),
    )?;
    // MOON_IAU → MOON_ICRF and MARS_IAU → MARS_ICRF.
    iau::register(&mut frames)?;

    // A London facility frame: east-north-up at the site.
    let london = FrameId::custom(Namespace(1), 1).unwrap();
    let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
    frames.add_fixed(
        london,
        FrameId::ITRF,
        site.enu_to_body_fixed(&Ellipsoid::WGS84),
    )?;

    // A lunar facility near the south pole (Shackleton crater rim).
    let shackleton = FrameId::custom(Namespace(1), 2).unwrap();
    let rim = Geodetic::from_degrees(-89.67, 129.78, 1_900.0)?;
    frames.add_fixed(
        shackleton,
        FrameId::MOON_IAU,
        rim.enu_to_body_fixed(&Ellipsoid::MOON),
    )?;

    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
    let rack = FixedLocator {
        frame: london,
        position_m: Vec3::new(0.0, 20.0, 1.5),
    };
    let lander = FixedLocator {
        frame: shackleton,
        position_m: Vec3::ZERO,
    };

    let a = rack.state_at(epoch, FrameId::GCRF, &frames)?;
    let b = lander.state_at(epoch, FrameId::MOON_ICRF, &frames)?;

    // Comparing `a` and `b` needs the Earth→Moon vector from an ephemeris
    // (`heavenly-spice`), so this is a frame error rather than a wrong answer:
    assert!(matches!(
        a.separation_m(&b),
        Err(Error::FrameMismatch { .. })
    ));
    // ANCHOR_END: sites

    println!(
        "London rack in GCRF:     {:?} m, {:.1} m/s",
        a.position_m,
        a.velocity_m_s.norm()
    );
    println!(
        "Shackleton in MOON_ICRF: {:?} m, {:.2} m/s",
        b.position_m,
        b.velocity_m_s.norm()
    );
    let back = Geodetic::from_cartesian(
        a.to_frame(FrameId::ITRF, &frames)?.position_m,
        &Ellipsoid::WGS84,
    )?;
    println!(
        "round trip: {:.6}°, {:.6}°, {:.3} m",
        back.latitude_rad.to_degrees(),
        back.longitude_rad.to_degrees(),
        back.height_m
    );
    Ok(())
}
