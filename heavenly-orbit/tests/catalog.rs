//! Loading catalogues, and propagating them together, bit-identically to
//! one object at a time.

#[rustfmt::skip]
#[path = "reference/omm_data.rs"]
mod omm_data;

use heavenly_core::{Epoch, FrameGraph, FrameId, Namespace, Rotation, Transform, Vec3};
use heavenly_orbit::{Catalog, Gravity, Omm, OpsMode};

/// The verification TLEs as three-line sets, with one corrupted checksum.
fn tle_text() -> (String, usize) {
    let mut text = String::new();
    let mut bad = 0;
    for (k, (l1, l2, id, ..)) in omm_data::STATES.iter().enumerate() {
        text.push_str(&format!("0 VER {id}\n"));
        let l1 = if k == 3 {
            l1.replacen("0  9955", "0  9956", 1)
        } else {
            (*l1).to_string()
        };
        text.push_str(&format!("{l1}\n{l2}\n"));
        let has_sum =
            |l: &str| l.len() == 69 && l.as_bytes()[68] - b'0' == heavenly_orbit::checksum(l);
        if !(has_sum(&l1) && has_sum(l2)) {
            bad += 1;
        }
    }
    (text, bad)
}

#[test]
fn loading_reports_what_it_could_not_load() {
    let (text, bad_sums) = tle_text();
    let (cat, rejected) = Catalog::from_tle_text(&text, Gravity::Wgs72, OpsMode::Improved);
    let failed_init = omm_data::STATES.iter().filter(|s| s.3 != 0).count();
    assert!(bad_sums >= 1);
    assert_eq!(cat.len() + rejected.len(), omm_data::STATES.len());
    assert!(
        rejected.len() >= bad_sums && rejected.len() <= bad_sums + failed_init,
        "{rejected:?}"
    );
    assert_eq!(cat.entries[0].name, "VER 5");
    assert_eq!(cat.entries[0].norad_cat_id, 5);
    // Every rejection points at the first line of its set.
    let lines: Vec<&str> = text.lines().collect();
    assert!(rejected.iter().all(|r| lines[r.at - 1].starts_with("1 ")));

    let omms = Omm::parse_json(omm_data::JSON).unwrap();
    let (from_omm, rejected) = Catalog::from_omms(&omms, Gravity::Wgs72, OpsMode::Improved);
    assert_eq!(from_omm.len() + rejected.len(), omms.len());
    assert_eq!(rejected.len(), failed_init);
}

#[test]
fn states_at_matches_one_at_a_time() {
    let omms = Omm::parse_json(omm_data::JSON).unwrap();
    let (cat, _) = Catalog::from_omms(&omms, Gravity::Wgs72, OpsMode::Improved);
    // A moving frame, so the batch path has real work to do.
    let spin = FrameId::custom(Namespace(5), 1).unwrap();
    let mut g = FrameGraph::new();
    g.add_edge(FrameId::TEME, spin, |t: Epoch| {
        let a = t.tt_seconds_since_j2000() * 7.292e-5;
        Ok(Transform::new(
            Rotation::from_axis_angle(Vec3::Z, a),
            Vec3::new(1.0, -2.0, 3.0),
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -7.292e-5),
        ))
    })
    .unwrap();
    let t = cat.entries[10].sgp4.epoch().add_seconds(3600.0).unwrap();
    let states = cat.states_at(t, spin, &g).unwrap();
    let mut some = 0;
    for (e, s) in cat.entries.iter().zip(&states) {
        let one = e.sgp4.state(t).ok().map(|x| x.to_frame(spin, &g).unwrap());
        assert_eq!(one, *s);
        some += s.is_some() as usize;
    }
    assert!(some > 20);
    let positions = cat.positions_at(t, spin, &g).unwrap();
    for (k, p) in positions.iter().enumerate() {
        assert_eq!(cat.position_of(k, t, spin, &g).unwrap(), *p);
    }
    assert!(cat.position_of(cat.len(), t, spin, &g).is_err());
}
