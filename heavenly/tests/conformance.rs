//! A portable conformance suite: the same reference values as the crates'
//! own validation tests (SOFA, SPICE, Vallado's SGP4, Skyfield), with the
//! synthetic SPICE kernel compiled in, so it needs no filesystem.
//!
//! It runs natively with `cargo test`, and as WebAssembly
//! (wasm32-unknown-unknown, under Node) with
//! `wasm-pack test --node heavenly -- --test conformance`.

// Some fixture files allow dead_code themselves, some do not.
#![allow(clippy::duplicated_attributes)]

#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-orbit/tests/reference/sgp4_data.rs"]
mod sgp4_data;
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-orbit/tests/reference/data.rs"]
mod orbit_data;
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-geo/tests/reference/data.rs"]
mod geo_data;
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-geo/tests/reference/teme_data.rs"]
mod teme_data;
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-spice/tests/reference/data.rs"]
mod spice_data;
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../heavenly-query/tests/reference/data.rs"]
mod query_data;

use std::sync::Arc;

use heavenly::geo::earth::{self, EarthOrientation};
use heavenly::orbit::{kepler, Gravity, OpsMode, Sgp4, Tle};
use heavenly::query::{light_time, Direction};
use heavenly::spice::{SpiceError, Spk, SpkLocator};
use heavenly::*;

/// Prints the worst differences: to the console under wasm, stderr natively.
macro_rules! report {
    ($($t:tt)*) => {{
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!($($t)*);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($t)*);
    }};
}

const SYNTHETIC_BSP: &[u8] = include_bytes!("../../heavenly-spice/tests/data/synthetic.bsp");

fn v(s: &[f64]) -> Vec3 {
    Vec3::new(s[0], s[1], s[2])
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sgp4_matches_vallado() {
    let (mut worst, mut n) = (0.0f64, 0);
    for (l1, l2, grav, mode, _, init_err, rows) in sgp4_data::VERIFICATION {
        let gravity = match *grav {
            "Wgs72" => Gravity::Wgs72,
            "Wgs72Old" => Gravity::Wgs72Old,
            _ => Gravity::Wgs84,
        };
        let opsmode = if *mode == "a" {
            OpsMode::Afspc
        } else {
            OpsMode::Improved
        };
        let sat = Sgp4::new(&Tle::parse_unchecked(l1, l2).unwrap(), gravity, opsmode);
        assert_eq!(*init_err != 0, sat.is_err(), "{l1}");
        let Ok(sat) = sat else { continue };
        for (t, err, r, vel) in *rows {
            match sat.propagate(*t) {
                Ok((gr, gv)) => {
                    let dr = (v(&gr) - v(r)).norm();
                    let dv = (v(&gv) - v(vel)).norm();
                    worst = worst.max(dr);
                    assert!(
                        *err == 0 && dr < 1e-8 && dv < 1e-11,
                        "{l1} {grav} {mode} t={t}: {dr:e} km"
                    );
                    n += 1;
                }
                Err(e) => assert_eq!(e.code(), *err, "{l1} t={t}"),
            }
        }
    }
    report!("SGP4 vs Vallado: {n} states, worst {worst:.2e} km");
    assert!(n > 2000);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn teme_and_gmst_match_sofa_and_skyfield() {
    let j2000 = Epoch::parse("2000-01-01T12:00:00Z").unwrap();
    let from_jd = |jd: f64| {
        let naive = (jd - 2451545.0) * 86400.0;
        let mut e = j2000.add_seconds(naive).unwrap();
        for _ in 0..2 {
            let leaps = e.tai_minus_utc_seconds().unwrap() - j2000.tai_minus_utc_seconds().unwrap();
            e = j2000.add_seconds(naive + leaps as f64).unwrap();
        }
        e
    };
    for (jd, gmst) in teme_data::GMST82 {
        let (g, _) = earth::gmst82(from_jd(*jd), &EarthOrientation::ZERO).unwrap();
        let d = (g - gmst).abs();
        assert!(d.min(core::f64::consts::TAU - d) < 1e-12, "{jd}");
    }
    let mut worst = 0.0f64;
    for (jd, xp, yp, r, vel, ri, vi) in teme_data::TEME_TO_ITRF {
        let eop = EarthOrientation {
            ut1_minus_utc_s: 0.0,
            xp_rad: *xp,
            yp_rad: *yp,
        };
        let t = earth::teme_to_itrf(from_jd(*jd), &eop).unwrap();
        let (p, w) = t.apply(v(r) * 1e3, v(vel) * 1e3);
        let dr = (p - v(ri) * 1e3).norm();
        worst = worst.max(dr);
        assert!(
            dr < 1e-7 && (w - v(vi) * 1e3).norm() < 1e-10,
            "{jd}: {dr:e} m"
        );
    }
    report!("TEME→ITRF vs Skyfield: worst {worst:.1e} m");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn earth_orientation_matches_sofa() {
    let mut worst = 0.0f64;
    for (utc, dut1, xp, yp, m) in geo_data::EARTH {
        let eop = EarthOrientation {
            ut1_minus_utc_s: *dut1,
            xp_rad: *xp,
            yp_rad: *yp,
        };
        let ours = earth::gcrs_to_itrs_matrix(Epoch::parse(utc).unwrap(), &eop).unwrap();
        // The angle of ours · refᵀ.
        let mut d = [[0.0; 3]; 3];
        for (i, row) in d.iter_mut().enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                *x = (0..3).map(|k| ours[i][k] * m[3 * j + k]).sum();
            }
        }
        let err = Rotation::from_matrix(d).unwrap().angle_rad();
        worst = worst.max(err);
        assert!(err < earth::MODEL_ACCURACY_RAD, "{utc}");
    }
    report!(
        "Earth orientation vs SOFA c2t06a: worst {:.2} mas",
        worst.to_degrees() * 3.6e6
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn two_body_matches_spice_prop2b() {
    let mut worst = 0.0f64;
    for (gm_km, s0, dt, s1) in orbit_data::PROP2B {
        let (p, w) = kepler::propagate(gm_km * 1e9, v(s0) * 1e3, v(&s0[3..]) * 1e3, *dt).unwrap();
        let (rp, rv) = (v(s1) * 1e3, v(&s1[3..]) * 1e3);
        worst = worst
            .max((p - rp).norm() / rp.norm())
            .max((w - rv).norm() / rv.norm());
    }
    report!("prop2b vs SPICE: worst relative {worst:.2e}");
    assert!(worst < 1e-9);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn spk_and_light_time_match_spice() {
    let spk = Arc::new(Spk::from_bytes(SYNTHETIC_BSP.to_vec()).unwrap());
    let mut worst = 0.0f64;
    for (t, o, e, s) in spice_data::SYNTHETIC_STATES {
        let (p, w) = spk.state_at_et(*t, *o, *e).unwrap();
        let dp = (p - v(s) * 1e3).norm();
        worst = worst.max(dp);
        assert!(
            dp < 1e-3 && (w - v(&s[3..]) * 1e3).norm() < 1e-9,
            "{t} wrt {o} at {e}"
        );
    }
    report!("SPK vs SPICE: worst {worst:.2e} m");

    let graph = FrameGraph::new();
    let loc = |body| SpkLocator {
        spk: Arc::clone(&spk),
        body,
    };
    for (target, observer, et, corr, st, lt) in query_data::SYNTHETIC {
        let dir = if *corr == "CN" {
            Direction::Reception
        } else {
            Direction::Transmission
        };
        let epoch = Epoch::from_tdb_seconds_since_j2000(*et).unwrap();
        let got = light_time::<_, _, _, SpiceError>(
            &loc(*observer),
            &loc(*target),
            epoch,
            FrameId::SSB_ICRF,
            &graph,
            dir,
        )
        .unwrap();
        // SPICE resolves `et ∓ lt` only to |et|·2⁻⁵² s (see heavenly-query's tests).
        let tol = 1e-3 + 2.0 * et.abs() * f64::EPSILON * 60e3;
        let dp = (got.relative_position_m - v(st) * 1e3).norm();
        assert!(dp < tol, "{target} from {observer} {corr}: {dp:e} m");
        assert!((got.light_time_s - lt).abs() < 1e-9);
    }
}
