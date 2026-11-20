//! OMM against the reference implementation's own OMM path, over the
//! official verification set written as KVN, JSON and CSV.

#[rustfmt::skip]
#[path = "reference/omm_data.rs"]
mod data;

use heavenly_orbit::{Gravity, Omm, OpsMode, Sgp4, Tle};

#[test]
fn kvn_json_and_csv_agree() {
    let kvn = Omm::parse_kvn(data::KVN).unwrap();
    let json = Omm::parse_json(data::JSON).unwrap();
    let csv = Omm::parse_csv(data::CSV).unwrap();
    assert_eq!(kvn.len(), data::STATES.len());
    assert_eq!(kvn, json);
    assert_eq!(kvn, csv);
    assert_eq!(kvn[0].object_name, "VER 5");
    assert_eq!(kvn[0].object_id, "1958-002B");
    assert_eq!(kvn[0].norad_cat_id, 5);
}

#[test]
fn sgp4_from_omm_matches_vallado() {
    let omms = Omm::parse_json(data::JSON).unwrap();
    let (mut worst, mut n) = (0.0f64, 0);
    for (omm, (l1, _, id, init_err, rows)) in omms.iter().zip(data::STATES) {
        assert_eq!(omm.norad_cat_id, *id);
        let sat = Sgp4::from_omm(omm, Gravity::Wgs72, OpsMode::Improved);
        assert_eq!(*init_err != 0, sat.is_err(), "{l1}");
        let Ok(sat) = sat else { continue };
        for (t, err, r, v) in *rows {
            match sat.propagate(*t) {
                Ok((gr, gv)) => {
                    let dr = (0..3).map(|i| (gr[i] - r[i]).powi(2)).sum::<f64>().sqrt();
                    let dv = (0..3).map(|i| (gv[i] - v[i]).powi(2)).sum::<f64>().sqrt();
                    worst = worst.max(dr);
                    assert!(
                        *err == 0 && dr < 1e-8 && dv < 1e-11,
                        "{id} t={t}: {dr:e} km"
                    );
                    n += 1;
                }
                Err(e) => assert_eq!(e.code(), *err, "{id} t={t}"),
            }
        }
    }
    eprintln!("OMM: {n} states vs Vallado (omm.initialize), worst {worst:.2e} km");
    assert!(n > 500);
}

#[test]
fn an_omm_and_its_tle_twin_agree() {
    // The same elements, rounded through two formats: the OMM's epoch is
    // given to the microsecond, and the mean motion converts differently.
    let omms = Omm::parse_kvn(data::KVN).unwrap();
    let mut worst = 0.0f64;
    for (omm, (l1, l2, _, init_err, rows)) in omms.iter().zip(data::STATES) {
        if *init_err != 0 {
            continue;
        }
        let tle = Tle::parse_unchecked(l1, l2).unwrap();
        // The OMM gives the epoch to the microsecond, and the exporter
        // truncates the TLE's epoch to it, so they differ by up to 1 µs,
        // plus a few ns from turning the TLE's day fraction into seconds.
        let de = omm.epoch.seconds_since(tle.epoch().unwrap());
        assert!(de.abs() <= 1.01e-6, "{l1}: epochs differ by {de:e} s");
        let (a, b) = (
            Sgp4::new(&tle, Gravity::Wgs72, OpsMode::Improved).unwrap(),
            Sgp4::from_omm(omm, Gravity::Wgs72, OpsMode::Improved).unwrap(),
        );
        for (t, err, ..) in *rows {
            if *err != 0 {
                break;
            }
            if let (Ok((ra, _)), Ok((rb, _))) = (a.propagate(*t), b.propagate(*t)) {
                worst = worst.max((0..3).map(|i| (ra[i] - rb[i]).powi(2)).sum::<f64>().sqrt());
            }
        }
    }
    eprintln!("OMM vs TLE twin: worst {worst:.2e} km");
    assert!(worst < 1e-5, "{worst} km");
}

#[test]
fn malformed_and_foreign_messages_are_rejected() {
    let one = data::KVN.split("\n\n").next().unwrap();
    assert_eq!(Omm::parse_kvn(one).unwrap().len(), 1);
    for (from, to) in [
        ("MEAN_ELEMENT_THEORY = SGP4", "MEAN_ELEMENT_THEORY = DSST"),
        ("REF_FRAME = TEME", "REF_FRAME = EME2000"),
        ("CENTER_NAME = EARTH", "CENTER_NAME = MOON"),
        (
            "EPOCH = 2000-06-27T18:50:19.733568",
            "EPOCH = 2000-06-27 noon",
        ),
        ("MEAN_MOTION = 10.82419157", "MEAN_MOTION = ten"),
        ("NORAD_CAT_ID = 5", "NORAD_CAT_ID = -5"),
    ] {
        assert!(one.contains(from), "{from}");
        assert!(Omm::parse_kvn(&one.replace(from, to)).is_err(), "{to}");
    }
    let without_epoch: String = one
        .lines()
        .filter(|l| !l.starts_with("EPOCH"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(Omm::parse_kvn(&without_epoch).is_err());
    // Units in brackets are allowed in KVN.
    assert!(
        Omm::parse_kvn(&one.replace("INCLINATION = 34.2682", "INCLINATION = 34.2682 [deg]"))
            .is_ok()
    );
    for bad in [
        "",
        "[",
        "[{]",
        "[{\"EPOCH\": }]",
        "[{\"A\": [1]}]",
        "[{}] x",
    ] {
        assert!(Omm::parse_json(bad).is_err(), "{bad:?}");
    }
    assert_eq!(Omm::parse_json("[]").unwrap().len(), 0);
    assert!(Omm::parse_csv("OBJECT_NAME,EPOCH\n\"unterminated,x\n").is_err());
}
