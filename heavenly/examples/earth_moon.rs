//! A rack in London and a lander at the lunar south pole, in one frame.
//!
//! Needs JPL kernels in the directory named by `HEAVENLY_KERNELS`:
//! `de440s.bsp` and `moon_pa_de440_200625.bpc` from
//! <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/>.
use std::sync::Arc;

use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::spice::{frames, Pck, Spk};
use heavenly::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(dir) = std::env::var("HEAVENLY_KERNELS") else {
        eprintln!(
            "set HEAVENLY_KERNELS to a directory with de440s.bsp and moon_pa_de440_200625.bpc"
        );
        return Ok(());
    };
    let dir = std::path::PathBuf::from(dir);

    // ANCHOR: earth_moon
    let mut graph = FrameGraph::new();
    // Ephemeris: GCRF and MOON_ICRF relative to the Solar System barycentre.
    let spk = Arc::new(Spk::open(dir.join("de440s.bsp"))?);
    frames::register_ephemeris(&mut graph, spk, frames::STANDARD_BODIES)?;
    // Orientation: ITRF → GCRF (Earth) and MOON_ME → MOON_PA → MOON_ICRF (Moon).
    earth::register(
        &mut graph,
        earth::EarthOrientation::from_bulletin(0.0712, 0.1683, 0.3021),
    )?;
    frames::register_moon(
        &mut graph,
        Arc::new(Pck::open(dir.join("moon_pa_de440_200625.bpc"))?),
    )?;

    // Facility frames, east-north-up on each body.
    let london = FrameId::custom(Namespace(1), 1).unwrap();
    let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
    graph.add_fixed(
        london,
        FrameId::ITRF,
        site.enu_to_body_fixed(&Ellipsoid::WGS84),
    )?;

    let shackleton = FrameId::custom(Namespace(1), 2).unwrap();
    let rim = Geodetic::from_degrees(-89.67, 129.78, 1_900.0)?;
    graph.add_fixed(
        shackleton,
        FrameId::MOON_ME,
        rim.enu_to_body_fixed(&Ellipsoid::MOON),
    )?;

    let rack = FixedLocator {
        frame: london,
        position_m: Vec3::new(0.0, 20.0, 1.5),
    };
    let lander = FixedLocator {
        frame: shackleton,
        position_m: Vec3::ZERO,
    };

    let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
    let a = rack.state_at(epoch, FrameId::GCRF, &graph)?;
    let b = lander.state_at(epoch, FrameId::GCRF, &graph)?;

    let distance_m = a.separation_m(&b)?;
    let range_rate_m_s = (b.position_m - a.position_m)
        .normalized()
        .unwrap()
        .dot(b.velocity_m_s - a.velocity_m_s);
    // ANCHOR_END: earth_moon

    // Geometric distance, not light-time-corrected.
    println!("London rack → Shackleton lander at {epoch}:");
    println!("  distance   {:.3} km", distance_m / 1e3);
    println!("  range rate {:+.3} m/s", range_rate_m_s);
    println!("  one-way light time ≈ {:.4} s", distance_m / 299_792_458.0);
    assert!((356e6..407e6).contains(&distance_m));
    Ok(())
}
