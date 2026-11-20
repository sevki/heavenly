//! Passes of the ISS over London, from a NORAD two-line element set.
//!
//! SGP4 gives states in TEME; the frame graph takes them through ITRF to an
//! east-north-up frame at the site.
use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::orbit::{Gravity, OpsMode, Sgp4, Tle};
use heavenly::query::{find_windows, look_angles};
use heavenly::*;

fn main() -> Result<(), Error> {
    // ANCHOR: sgp4
    let tle = Tle::parse(
        "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992",
        "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
    )?;
    let iss = Sgp4::new(&tle, Gravity::Wgs72, OpsMode::Improved)?;

    let mut frames = FrameGraph::new();
    // Registers ITRF → GCRF and TEME → ITRF.
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    let london = FrameId::custom(Namespace(1), 1).unwrap();
    let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
    frames.add_fixed(
        london,
        FrameId::ITRF,
        site.enu_to_body_fixed(&Ellipsoid::WGS84),
    )?;

    // Passes above 10° in the day after the TLE epoch.
    let start = iss.epoch();
    let end = start.add_seconds(86_400.0)?;
    let mask = 10f64.to_radians();
    let passes = find_windows(start, end, 30.0, 1e-3, |t| {
        look_angles::<_, _, Error>(london, &iss, t, &frames).map(|a| a.elevation_rad - mask)
    })?;
    // ANCHOR_END: sgp4

    println!("ISS epoch {}", start.to_calendar(TimeScale::Utc)?);
    for p in &passes {
        // Highest point: sample the pass every 5 s.
        let steps = (p.duration_s() / 5.0) as usize;
        let top = (0..=steps)
            .map(|k| {
                let t = p.start.add_seconds(k as f64 * 5.0).unwrap();
                look_angles::<_, _, Error>(london, &iss, t, &frames).unwrap()
            })
            .max_by(|a, b| a.elevation_rad.total_cmp(&b.elevation_rad))
            .unwrap();
        println!(
            "rise {}  {:4.0} s  max elevation {:4.1}° at azimuth {:5.1}°, range {:6.1} km",
            p.start.to_calendar(TimeScale::Utc)?,
            p.duration_s(),
            top.elevation_rad.to_degrees(),
            top.azimuth_rad.to_degrees(),
            top.range_m / 1e3
        );
    }
    Ok(())
}
