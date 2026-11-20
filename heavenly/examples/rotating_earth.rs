//! A ground station in a rotating Earth-fixed frame, seen from an inertial frame.
//!
//! The Earth rotation model here is a uniform spin for illustration only;
//! real Earth orientation (precession, nutation, polar motion, UT1) belongs
//! in `heavenly-geo` / an ephemeris adapter.
use heavenly::*;

fn main() -> Result<(), Error> {
    // ANCHOR: earth
    const OMEGA_RAD_S: f64 = 7.292_115e-5;

    let mut frames = FrameGraph::new();
    frames.add_edge(FrameId::ITRF, FrameId::GCRF, |epoch| {
        let theta = OMEGA_RAD_S * epoch.seconds_since(Epoch::J2000);
        Ok(Transform::new(
            Rotation::from_axis_angle(Vec3::Z, theta),
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Z * OMEGA_RAD_S,
        ))
    })?;

    // Typed coordinates: the frame is part of the type.
    let station =
        Coordinate::<EarthFixed>::new(Epoch::J2000, Vec3::new(6_378_137.0, 0.0, 0.0), Vec3::ZERO);
    let inertial: Coordinate<Gcrf> = station.to_frame(&frames)?;

    // At rest on the ground, but moving at ~465 m/s inertially.
    let speed = inertial.velocity_m_s.norm();
    assert!((speed - 465.1).abs() < 0.1);
    // ANCHOR_END: earth
    println!("inertial speed: {speed:.1} m/s");
    Ok(())
}
