//! A spacecraft seen from a ground station, from elements and from samples.
use heavenly_core::*;
use heavenly_geo::{earth, Ellipsoid, Geodetic};
use heavenly_orbit::{gm, ConicElements, SampledTrajectory, TwoBodyLocator};

fn main() -> Result<(), Error> {
    // ANCHOR: spacecraft
    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    let station = FrameId::custom(Namespace(1), 1).unwrap();
    let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
    frames.add_fixed(
        station,
        FrameId::ITRF,
        site.enu_to_body_fixed(&Ellipsoid::WGS84),
    )?;

    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
    let iss = TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_778_137.0,
            eccentricity: 0.0005,
            inclination_rad: 51.64f64.to_radians(),
            ascending_node_rad: 3.0,
            argument_of_periapsis_rad: 0.0,
            mean_anomaly_rad: 0.0,
            epoch,
            gm_m3_s2: gm::EARTH,
        },
    };

    // Range from the station over one orbit, every ten minutes.
    for minutes in (0..=90).step_by(10) {
        let t = epoch.add_seconds(minutes as f64 * 60.0)?;
        let local = iss.state_at(t, station, &frames)?; // east-north-up at the site
        let elevation = (local.position_m.z / local.position_m.norm())
            .asin()
            .to_degrees();
        println!(
            "+{minutes:2} min  range {:8.1} km  elevation {elevation:6.1}°",
            local.position_m.norm() / 1e3
        );
    }

    // The same orbit as tracking data, with an outage from 20 to 30 minutes.
    let samples: Vec<StateVector> = (0..=90)
        .filter(|m| !(21..30).contains(m))
        .map(|m| iss.state(epoch.add_seconds(m as f64 * 60.0).unwrap()))
        .collect::<Result<_, _>>()?;
    let tracked = SampledTrajectory::new(samples, 90.0)?;
    let in_gap = tracked.state_at(epoch.add_seconds(25.0 * 60.0)?, station, &frames);
    assert!(matches!(in_gap, Err(Error::DataGap { .. })));
    // ANCHOR_END: spacecraft
    println!("during the outage: {}", in_gap.unwrap_err());
    Ok(())
}
