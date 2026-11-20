//! Locating a rack inside a facility frame, relative to a site frame.
use heavenly::*;

fn main() -> Result<(), Error> {
    // ANCHOR: rack
    let site = FrameId::custom(Namespace(1), 100).unwrap();
    let facility = FrameId::custom(Namespace(1), 101).unwrap();

    // The facility frame sits 10 m east of the site origin, rotated 90° about up.
    let mut frames = FrameGraph::new();
    frames.add_fixed(
        facility,
        site,
        Transform::fixed(
            Rotation::from_axis_angle(Vec3::Z, core::f64::consts::FRAC_PI_2),
            Vec3::new(10.0, 0.0, 0.0),
        ),
    )?;

    let rack = LocationRecord {
        id: LocationId(0x1),
        locator: FixedLocator {
            frame: facility,
            position_m: Vec3::new(1.0, 0.0, 0.0),
        },
    };

    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
    let state = rack.locator.state_at(epoch, site, &frames)?;
    // 1 m along the facility's x axis is 1 m along the site's y axis.
    assert!((state.position_m - Vec3::new(10.0, 1.0, 0.0)).norm() < 1e-12);
    // ANCHOR_END: rack
    println!(
        "rack {} at {:?} in {}",
        rack.id, state.position_m, state.frame
    );
    Ok(())
}
