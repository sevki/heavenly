//! Which ISS passes over London can you actually see? The satellite has to
//! be above the horizon, in sunlight, while the sky at the site is dark
//! (the Sun more than 6° below the horizon).
//!
//! The Sun here is `AnalyticSun`, which needs no kernels.
use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::orbit::{Gravity, OpsMode, Sgp4, Tle};
use heavenly::query::{find_windows, look_angles, AnalyticSun, Occulter, ShadowKind};
use heavenly::*;

fn main() -> Result<(), Error> {
    // ANCHOR: visible
    let tle = Tle::parse(
        "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992",
        "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
    )?;
    let iss = Sgp4::new(&tle, Gravity::Wgs72, OpsMode::Improved)?;
    let mut frames = FrameGraph::new();
    earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
    let london = FrameId::custom(Namespace(1), 1).unwrap();
    let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
    frames.add_fixed(
        london,
        FrameId::ITRF,
        site.enu_to_body_fixed(&Ellipsoid::WGS84),
    )?;

    let elevation =
        |t: Epoch| look_angles::<_, _, Error>(london, &iss, t, &frames).map(|a| a.elevation_rad);
    let sun_elevation = |t: Epoch| {
        look_angles::<_, _, Error>(london, &AnalyticSun, t, &frames).map(|a| a.elevation_rad)
    };
    let in_sunlight = |t: Epoch| {
        Occulter::EARTH
            .shadow::<_, _, _, Error>(&iss, &AnalyticSun, t, &frames)
            .map(|s| s.sunlit_fraction())
    };

    // Visible: above 10°, sunlit, and the Sun below −6°. Each condition is a
    // signed function, and the smallest of them is positive exactly when
    // all three hold, so one window search finds the visible stretches.
    let start = iss.epoch();
    let end = start.add_seconds(2.0 * 86_400.0)?;
    let visible = find_windows(start, end, 20.0, 1e-2, |t| {
        let above = elevation(t)? - 10f64.to_radians();
        let lit = in_sunlight(t)? - 0.5;
        let dark = -6f64.to_radians() - sun_elevation(t)?;
        Ok::<f64, Error>(above.min(lit).min(dark))
    })?;
    // ANCHOR_END: visible

    let utc = |t: Epoch| t.to_calendar(TimeScale::Utc).map(|c| c.to_string());
    println!("Visible ISS passes over London (elevation > 10°, sunlit, Sun < −6°):");
    for w in &visible {
        let mid = w.start.add_seconds(w.duration_s() / 2.0)?;
        let a = look_angles::<_, _, Error>(london, &iss, mid, &frames)?;
        // Why did the window end? Set, entered the Earth's shadow, or dawn.
        let why = if elevation(w.end)? <= 10f64.to_radians() + 1e-4 {
            "sets"
        } else if Occulter::EARTH
            .shadow::<_, _, _, Error>(&iss, &AnalyticSun, w.end, &frames)?
            .kind()
            != ShadowKind::Sunlit
        {
            "enters the Earth's shadow"
        } else {
            "sky brightens"
        };
        println!(
            "  {} to {} ({:3.0} s), elevation {:4.1}° at the middle, then {why}",
            utc(w.start)?,
            utc(w.end)?,
            w.duration_s(),
            a.elevation_rad.to_degrees(),
        );
    }
    Ok(())
}
