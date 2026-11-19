//! Converting one instant between time scales.
use heavenly_core::*;

fn main() -> Result<(), Error> {
    // ANCHOR: scales
    let e = Epoch::parse("2016-12-31T23:59:60.5Z")?; // inside a leap second
    for scale in [
        TimeScale::Utc,
        TimeScale::Tai,
        TimeScale::Tt,
        TimeScale::Tdb,
    ] {
        println!("{}", e.to_calendar(scale)?);
    }
    // SPICE "ephemeris time": TDB seconds past J2000.
    println!("ET = {:.6}", e.tdb_seconds_since_j2000());

    // Elapsed time is always physical (SI) seconds, so it counts the leap second.
    let a = Epoch::parse("2016-12-31T23:59:59Z")?;
    let b = Epoch::parse("2017-01-01T00:00:00Z")?;
    assert_eq!(b.seconds_since(a), 2.0);
    // ANCHOR_END: scales
    Ok(())
}
