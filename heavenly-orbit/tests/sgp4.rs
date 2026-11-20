//! SGP4 against Vallado's C++ reference implementation over the official
//! verification TLEs, in every gravity model and operation mode.

#[rustfmt::skip]
#[path = "reference/sgp4_data.rs"]
mod data;

use heavenly_core::{Epoch, FrameGraph, FrameId, Locator, Vec3};
use heavenly_orbit::{checksum, Gravity, OpsMode, Sgp4, Tle};

fn gravity(s: &str) -> Gravity {
    match s {
        "Wgs72" => Gravity::Wgs72,
        "Wgs72Old" => Gravity::Wgs72Old,
        "Wgs84" => Gravity::Wgs84,
        _ => unreachable!(),
    }
}

#[test]
fn matches_vallado_on_the_verification_set() {
    let (mut worst_r, mut worst_v, mut n, mut deep) = (0.0f64, 0.0f64, 0, 0);
    for (l1, l2, grav, mode, epoch, init_err, rows) in data::VERIFICATION {
        let tle = Tle::parse_unchecked(l1, l2).unwrap();
        let opsmode = if *mode == "a" {
            OpsMode::Afspc
        } else {
            OpsMode::Improved
        };
        let sat = Sgp4::new(&tle, gravity(grav), opsmode);
        assert_eq!(*init_err != 0, sat.is_err(), "{l1}: init");
        let Ok(sat) = sat else { continue };
        deep += sat.is_deep_space() as usize;
        assert_eq!(sat.sgp4_epoch(), *epoch, "{l1}: epoch");
        for (t, err, r, v) in *rows {
            match sat.propagate(*t) {
                Ok((gr, gv)) => {
                    assert_eq!(*err, 0, "{l1} {grav} {mode} t={t}: expected error {err}");
                    let dr = (0..3).map(|i| (gr[i] - r[i]).powi(2)).sum::<f64>().sqrt();
                    let dv = (0..3).map(|i| (gv[i] - v[i]).powi(2)).sum::<f64>().sqrt();
                    worst_r = worst_r.max(dr);
                    worst_v = worst_v.max(dv);
                    assert!(
                        dr < 1e-8 && dv < 1e-11,
                        "{l1} {grav} {mode} t={t}: {dr:e} km {dv:e} km/s"
                    );
                    n += 1;
                }
                Err(e) => assert_eq!(e.code(), *err, "{l1} {grav} {mode} t={t}"),
            }
        }
    }
    eprintln!("{n} states, {deep} deep-space inits; worst {worst_r:.2e} km, {worst_v:.2e} km/s");
    assert!(n > 2000);
}

#[test]
fn tle_fields_match_the_reference_parser() {
    let xpdotp = 1440.0 / (2.0 * std::f64::consts::PI);
    let rad = std::f64::consts::PI / 180.0;
    for (l1, l2, satnum, f) in data::PARSED {
        let t = Tle::parse_unchecked(l1, l2).unwrap();
        assert_eq!(t.catalog_number, *satnum);
        let got = [
            t.bstar,
            t.eccentricity,
            t.inclination_deg * rad,
            t.raan_deg * rad,
            t.arg_perigee_deg * rad,
            t.mean_anomaly_deg * rad,
            t.mean_motion_rev_day / xpdotp,
            t.mean_motion_dot / (xpdotp * 1440.0),
            t.mean_motion_ddot / (xpdotp * 1440.0 * 1440.0),
        ];
        for (g, w) in got.iter().zip(f) {
            assert!((g - w).abs() <= 1e-15 * w.abs(), "{l1}: {got:?} vs {f:?}");
        }
        // Lines that carry a checksum digit pass the strict parser.
        let has_sum = |l: &str| l.len() == 69 && l.as_bytes()[68] - b'0' == checksum(l);
        if has_sum(l1) && has_sum(l2) {
            assert_eq!(Tle::parse(l1, l2).unwrap(), t);
        }
    }
}

const ISS: (&str, &str) = (
    "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992",
    "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
);

#[test]
fn strict_parsing() {
    let t = Tle::parse(ISS.0, ISS.1).unwrap();
    assert_eq!(t.catalog_number, 25544);
    assert_eq!(t.epoch_year, 2020);
    assert!((t.bstar + 0.31515e-4).abs() < 1e-18);
    assert!((t.eccentricity - 0.0001413).abs() < 1e-18);
    assert_eq!(t.revolution, 23600);
    // Corrupt one digit: the checksum catches it.
    let bad = ISS.1.replacen("51.6461", "51.6462", 1);
    assert!(Tle::parse(ISS.0, &bad).is_err());
    assert!(Tle::parse_unchecked(ISS.0, &bad).is_ok());
    // Mismatched catalogue numbers and swapped lines are rejected.
    assert!(Tle::parse_unchecked(ISS.1, ISS.0).is_err());
    let other = ISS.1.replacen("25544", "25545", 1);
    assert!(Tle::parse_unchecked(ISS.0, &other).is_err());
    // Alpha-5: A = 10, so A0001 is 100001; J (skipping I) is 18.
    let a5 = |n: &str| {
        let l1 = ISS.0.replacen("25544", n, 1);
        let l2 = ISS.1.replacen("25544", n, 1);
        Tle::parse_unchecked(&l1, &l2).unwrap().catalog_number
    };
    assert_eq!(a5("A0001"), 100001);
    assert_eq!(a5("J9999"), 189999);
    assert_eq!(a5("Z9999"), 339999);
}

#[test]
fn epoch_and_locator() {
    let t = Tle::parse(ISS.0, ISS.1).unwrap();
    // Day 194.88612269 of 2020 is 12 July, 21:16:01.4 UTC.
    let e = t.epoch().unwrap();
    let expect = Epoch::parse("2020-07-12T21:16:01.000416Z").unwrap();
    assert!(
        e.seconds_since(expect).abs() < 1e-6,
        "{}",
        e.seconds_since(expect)
    );
    let sat = Sgp4::new(&t, Gravity::Wgs72, OpsMode::Improved).unwrap();
    assert!(!sat.is_deep_space());
    // The locator agrees with `propagate`, in metres.
    let later = e.add_seconds(5400.0).unwrap();
    let s = sat
        .state_at(later, FrameId::TEME, &FrameGraph::new())
        .unwrap();
    let (r, v) = sat.propagate(90.0).unwrap();
    assert_eq!(s.frame, FrameId::TEME);
    assert!((s.position_m - Vec3::from(r) * 1e3).norm() < 1e-6);
    assert!((s.velocity_m_s - Vec3::from(v) * 1e3).norm() < 1e-9);
    // ISS altitude is about 420 km.
    let alt = s.position_m.norm() - 6_378_137.0;
    assert!((380e3..460e3).contains(&alt), "{alt}");
}

#[test]
fn minutes_since_epoch_ignore_leap_seconds() {
    // A TLE from 2016-12-31; 2017-01-01 00:00 UTC came after a leap second.
    let l1 = "1 25544U 98067A   16366.50000000  .00000000  00000-0  00000-0 0  0000";
    let l2 = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896  0000";
    let sat = Sgp4::new(
        &Tle::parse_unchecked(l1, l2).unwrap(),
        Gravity::Wgs72,
        OpsMode::Improved,
    )
    .unwrap();
    let next_noon = Epoch::parse("2017-01-01T12:00:00Z").unwrap();
    assert_eq!(next_noon.seconds_since(sat.epoch()), 86401.0);
    assert_eq!(sat.minutes_since_epoch(next_noon).unwrap(), 1440.0);
}
