//! IERS EOP tables against astropy's IERS readers (see
//! `tests/reference/generate_eop.py`).

#[rustfmt::skip]
#[path = "reference/eop_data.rs"]
mod data;

use heavenly_core::{Epoch, Error, FrameGraph, FrameId, FrameResolver};
use heavenly_geo::earth::{self, EopProvider};
use heavenly_geo::eop::EopTable;

const ARCSEC: f64 = std::f64::consts::PI / 180.0 / 3600.0;

fn read(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Compares with astropy: exactly at the tabulated days, and between them
/// within the difference between astropy's linear interpolation and our
/// four-point Lagrange.
fn check(table: &EopTable, refs: &[(&str, bool, f64, f64, f64)], label: &str) {
    let (mut worst_ut1, mut worst_pm) = (0.0f64, 0.0f64);
    for (utc, node, ut1, x, y) in refs {
        let e = table.get(Epoch::parse(utc).unwrap()).unwrap();
        let d_ut1 = (e.ut1_minus_utc_s - ut1).abs();
        let d_pm = ((e.xp_rad / ARCSEC - x).abs()).max((e.yp_rad / ARCSEC - y).abs());
        if *node {
            assert!(
                d_ut1 < 1e-12 && d_pm < 1e-12,
                "{label} {utc}: {d_ut1:e} s, {d_pm:e}\""
            );
        } else {
            worst_ut1 = worst_ut1.max(d_ut1);
            worst_pm = worst_pm.max(d_pm);
            assert!(
                d_ut1 < 5e-5 && d_pm < 3e-4,
                "{label} {utc}: {d_ut1:e} s, {d_pm:e}\""
            );
        }
    }
    eprintln!(
        "{label}: Lagrange vs astropy's linear, worst {worst_ut1:.1e} s UT1, {:.3} mas pole",
        worst_pm * 1e3
    );
}

#[test]
fn finals2000a_matches_astropy() {
    let t = EopTable::parse_finals(&read("finals2000A_2016.txt")).unwrap();
    assert_eq!(t.records().len(), 365);
    assert_eq!(t.first_predicted(), None);
    check(&t, data::FINALS_2016, "finals2000A 2016");
}

#[test]
fn c04_matches_astropy() {
    let t = EopTable::parse_c04(&read("eopc04_2016.txt")).unwrap();
    assert_eq!(t.records().len(), 365);
    assert!(t
        .records()
        .iter()
        .all(|r| r.dx_mas.is_some() && r.lod_ms.is_some()));
    check(&t, data::C04_2016, "C04 2016");
}

#[test]
fn predictions_are_flagged() {
    let t = EopTable::parse(&read("finals2000A_tail.txt")).unwrap();
    assert_eq!(
        t.first_predicted(),
        Some(Epoch::parse(data::FIRST_PREDICTED).unwrap())
    );
    let p = t.first_predicted().unwrap();
    assert!(t.records().iter().all(|r| r.predicted == (r.epoch >= p)));
    check(&t, data::FINALS_TAIL, "finals2000A tail");
}

/// Drops every other day of C04 and predicts the dropped days: four-point
/// Lagrange must beat linear interpolation (what astropy and TEMPO use).
#[test]
fn lagrange_beats_linear_on_held_out_days() {
    use heavenly_geo::eop::EopRecord;
    let full = EopTable::parse_c04(&read("eopc04_2016.txt")).unwrap();
    let r = full.records();
    let even: Vec<EopRecord> = r.iter().step_by(2).copied().collect();
    let half = EopTable::from_records(even).unwrap();
    let ut1_tai =
        |rec: &EopRecord| rec.ut1_minus_utc_s - rec.epoch.tai_minus_utc_seconds().unwrap() as f64;
    let (mut lag, mut lin) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
    for k in (3..r.len() - 3).step_by(2) {
        let got = half.get(r[k].epoch).unwrap();
        let tai = r[k].epoch.tai_minus_utc_seconds().unwrap() as f64;
        lag.0 = lag
            .0
            .max((got.ut1_minus_utc_s - tai - ut1_tai(&r[k])).abs());
        lag.1 = lag.1.max((got.xp_rad / ARCSEC - r[k].xp_arcsec).abs());
        let mid = |a: f64, b: f64| (a + b) / 2.0;
        lin.0 = lin
            .0
            .max((mid(ut1_tai(&r[k - 1]), ut1_tai(&r[k + 1])) - ut1_tai(&r[k])).abs());
        lin.1 = lin
            .1
            .max((mid(r[k - 1].xp_arcsec, r[k + 1].xp_arcsec) - r[k].xp_arcsec).abs());
    }
    eprintln!(
        "held-out days: Lagrange {:.1e} s, {:.3} mas; linear {:.1e} s, {:.3} mas",
        lag.0,
        lag.1 * 1e3,
        lin.0,
        lin.1 * 1e3
    );
    assert!(lag.0 < lin.0 && lag.1 < lin.1);
}

#[test]
fn the_format_is_recognised() {
    let a = EopTable::parse(&read("finals2000A_2016.txt")).unwrap();
    let c = EopTable::parse(&read("eopc04_2016.txt")).unwrap();
    assert_eq!(a.range(), c.range());
    // The two series agree to well under a millisecond and a milliarcsecond.
    let e = Epoch::parse("2017-03-14T15:09:26Z").unwrap();
    let (ea, ec) = (a.get(e).unwrap(), c.get(e).unwrap());
    assert!((ea.ut1_minus_utc_s - ec.ut1_minus_utc_s).abs() < 1e-4);
    assert!((ea.xp_rad - ec.xp_rad).abs() < 1e-3 * ARCSEC);
}

#[test]
fn ut1_is_continuous_across_the_leap_second() {
    let t = EopTable::parse_finals(&read("finals2000A_2016.txt")).unwrap();
    // UT1 − UTC jumps by +1 s when the leap second is inserted at the end
    // of 2016-12-31, while UT1 itself (UT1 − TAI) stays smooth.
    let before = Epoch::parse("2016-12-31T23:59:59.5Z").unwrap();
    let after = Epoch::parse("2017-01-01T00:00:00.5Z").unwrap();
    assert_eq!(after.seconds_since(before), 2.0);
    let (b, a) = (t.get(before).unwrap(), t.get(after).unwrap());
    assert!((a.ut1_minus_utc_s - b.ut1_minus_utc_s - 1.0).abs() < 1e-6);
    let ut1_tai = |e: Epoch, v: f64| v - e.tai_minus_utc_seconds().unwrap() as f64;
    assert!((ut1_tai(after, a.ut1_minus_utc_s) - ut1_tai(before, b.ut1_minus_utc_s)).abs() < 1e-7);
}

#[test]
fn no_extrapolation() {
    let t = EopTable::parse_finals(&read("finals2000A_2016.txt")).unwrap();
    let (start, end) = t.range();
    assert_eq!(start, Epoch::parse("2016-07-01T00:00:00Z").unwrap());
    assert_eq!(end, Epoch::parse("2017-06-30T00:00:00Z").unwrap());
    assert!(t.get(start).is_ok() && t.get(end).is_ok());
    for e in [
        start.add_seconds(-1.0).unwrap(),
        end.add_seconds(1.0).unwrap(),
    ] {
        assert!(matches!(t.get(e), Err(Error::OutsideCoverage { .. })));
    }
}

#[test]
fn drives_the_earth_frames() {
    let t = EopTable::parse_finals(&read("finals2000A_2016.txt")).unwrap();
    let e = Epoch::parse("2017-03-14T15:09:26Z").unwrap();
    let eop = t.earth_orientation(e).unwrap();
    let mut g = FrameGraph::new();
    earth::register(&mut g, t.clone()).unwrap();
    let via_graph = g.transform(FrameId::ITRF, FrameId::GCRF, e).unwrap();
    let direct = earth::itrf_to_gcrf(e, &eop).unwrap();
    assert_eq!(via_graph.transform, direct);
    // Outside the table the graph reports the coverage error.
    let late = Epoch::parse("2030-01-01T00:00:00Z").unwrap();
    assert!(g.transform(FrameId::ITRF, FrameId::GCRF, late).is_err());
}

#[test]
fn malformed_input() {
    assert!(EopTable::parse_finals("").is_err());
    let one = read("finals2000A_2016.txt")
        .lines()
        .next()
        .unwrap()
        .to_string();
    assert!(EopTable::parse_finals(&one).is_err()); // a single day is not a table
    let text = read("finals2000A_2016.txt");
    let lines: Vec<&str> = text.lines().take(3).collect();
    let swapped = format!("{}\n{}\n", lines[1], lines[0]);
    assert!(EopTable::parse_finals(&swapped).is_err());
    let garbled = lines[0].replacen("0.", "x.", 1);
    assert!(EopTable::parse_finals(&format!("{garbled}\n{}\n", lines[1])).is_err());
    assert!(EopTable::parse_c04("# header\n1990 1 1 0 47892.00 bad\n").is_err());
}
