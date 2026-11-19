//! Validation against SPICE (spiceypy). Values come from
//! `tests/reference/generate.py`, which also writes the synthetic kernels.

#[rustfmt::skip]
#[path = "reference/data.rs"]
mod data;

use std::path::PathBuf;
use std::sync::Arc;

use heavenly_core::{Epoch, FrameGraph, FrameId, FrameResolver, Rotation, Vec3};
use heavenly_spice::{frames, Pck, SpiceError, Spk};

fn data_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn et(e: f64) -> Epoch {
    Epoch::from_tdb_seconds_since_j2000(e).unwrap()
}

/// Checks states against SPICE at the same ET, so the only difference is
/// floating-point evaluation order.
fn check_states(spk: &Spk, refs: &[(i32, i32, f64, [f64; 6])], pos_tol_m: f64, vel_tol_m_s: f64) {
    let mut worst = (0.0f64, 0.0f64);
    for (t, o, e, s) in refs {
        let (p, v) = spk
            .state_at_et(*t, *o, *e)
            .unwrap_or_else(|err| panic!("{t} wrt {o} at {e}: {err}"));
        let dp = (p - Vec3::new(s[0], s[1], s[2]) * 1e3).norm();
        let dv = (v - Vec3::new(s[3], s[4], s[5]) * 1e3).norm();
        worst = (worst.0.max(dp), worst.1.max(dv));
        assert!(
            dp < pos_tol_m && dv < vel_tol_m_s,
            "{t} wrt {o} at {e}: {dp} m, {dv} m/s"
        );
    }
    eprintln!("worst: {:.3e} m, {:.3e} m/s", worst.0, worst.1);
}

#[test]
fn synthetic_spk_matches_spice() {
    let spk = Spk::open(data_file("synthetic.bsp")).unwrap();
    // Positions reach ~1.5e11 m, where an f64 ulp is ~3e-5 m.
    check_states(&spk, data::SYNTHETIC_STATES, 1e-3, 1e-9);
}

#[test]
fn big_endian_spk_reads_identically() {
    let le = Spk::open(data_file("synthetic.bsp")).unwrap();
    let be = Spk::open(data_file("synthetic_be.bsp")).unwrap();
    assert_eq!(le.segments(), be.segments());
    for (t, o, e, _) in data::SYNTHETIC_STATES {
        assert_eq!(
            le.state_at_et(*t, *o, *e).unwrap(),
            be.state_at_et(*t, *o, *e).unwrap()
        );
    }
}

#[test]
fn segment_metadata_and_errors() {
    let spk = Spk::open(data_file("synthetic.bsp")).unwrap();
    let s = spk.segments();
    assert_eq!(s.len(), 4);
    assert_eq!((s[0].target, s[0].center, s[0].data_type), (3, 0, 3));
    assert_eq!(
        (s[3].target, s[3].start_et, s[3].end_et),
        (301, 0.0, 50.0 * 86400.0)
    );

    let outside = et(500.0 * 86400.0);
    assert!(matches!(
        spk.state(301, 399, outside),
        Err(SpiceError::NoCoverage { id: 301, .. })
    ));
    // Body 10 (the Sun) is not in the kernel, so it is a root that shares no
    // centre with the Earth's chain.
    assert!(matches!(
        spk.state(10, 399, Epoch::J2000),
        Err(SpiceError::NoPath { .. })
    ));

    assert!(matches!(
        Spk::from_bytes(vec![0; 2048]),
        Err(SpiceError::Format(_))
    ));
    let pck_bytes = std::fs::read(data_file("synthetic.bpc")).unwrap();
    assert!(matches!(
        Spk::from_bytes(pck_bytes),
        Err(SpiceError::Format(_))
    ));
}

fn mat3(m: &[f64; 36], row0: usize, col0: usize) -> [[f64; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = m[(row0 + i) * 6 + col0 + j];
        }
    }
    r
}

fn apply(m: &[[f64; 3]; 3], v: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
        m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
        m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
    )
}

/// Checks body-fixed → ICRF position *and* velocity of fixed surface points
/// against SPICE's 6×6 state transform.
fn check_pck(pck: &Pck, refs: &[(f64, [f64; 36])], tol_rad: f64) {
    let r = 1.7374e6;
    let points = [
        Vec3::X * r,
        Vec3::Y * r,
        Vec3::Z * r,
        Vec3::new(0.6, -0.48, 0.64) * r,
    ];
    let mut worst = (0.0f64, 0.0f64);
    for (e, m) in refs {
        let t = pck
            .body_fixed_to_icrf_at_et(frames::MOON_PA_DE440, *e)
            .unwrap();
        let (rot, drot) = (mat3(m, 0, 0), mat3(m, 3, 0));
        for p in points {
            let (pi, vi) = t.apply(p, Vec3::ZERO);
            worst.0 = worst.0.max((pi - apply(&rot, p)).norm() / r);
            worst.1 = worst.1.max((vi - apply(&drot, p)).norm() / r);
        }
    }
    eprintln!("worst: {:.3e} rad, {:.3e} rad/s", worst.0, worst.1);
    assert!(worst.0 < tol_rad && worst.1 < 1e-15, "{worst:?}");
}

#[test]
fn synthetic_pck_matches_spice() {
    let pck = Pck::open(data_file("synthetic.bpc")).unwrap();
    check_pck(&pck, data::SYNTHETIC_PCK, 1e-12);
}

#[test]
fn moon_me_to_pa_matches_frame_kernel() {
    let t = frames::moon_me_to_pa();
    let m = &data::MOON_ME_TO_PA;
    let reference =
        Rotation::from_matrix([[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]])
            .unwrap();
    for v in [Vec3::X, Vec3::Y, Vec3::Z] {
        assert!((t.rotation.apply(v) - reference.apply(v)).norm() < 1e-15);
    }
}

#[test]
fn frame_graph_connects_bodies() {
    let mut graph = FrameGraph::new();
    let spk = Arc::new(Spk::open(data_file("synthetic.bsp")).unwrap());
    let added =
        frames::register_ephemeris(&mut graph, Arc::clone(&spk), frames::STANDARD_BODIES).unwrap();
    // The synthetic kernel has no body 499, so Mars is skipped.
    assert_eq!(added, vec![FrameId::GCRF, FrameId::MOON_ICRF]);
    frames::register_moon(
        &mut graph,
        Arc::new(Pck::open(data_file("synthetic.bpc")).unwrap()),
    )
    .unwrap();

    let epoch = et(1234567.0);
    let t = graph
        .transform(FrameId::MOON_ICRF, FrameId::GCRF, epoch)
        .unwrap();
    let (p, v) = spk.state(301, 399, epoch).unwrap();
    // The graph routes Moon → SSB → Earth, subtracting ~1.5e11 m vectors.
    assert!((t.transform.translation_m - p).norm() < 1e-3);
    assert!((t.transform.origin_velocity_m_s - v).norm() < 1e-9);

    // MOON_ME resolves through MOON_PA and MOON_ICRF to the Earth.
    assert!(graph
        .transform(FrameId::MOON_ME, FrameId::GCRF, epoch)
        .is_ok());
    // Out of coverage becomes an explicit core error.
    assert!(matches!(
        graph.transform(FrameId::MOON_ICRF, FrameId::GCRF, et(900.0 * 86400.0)),
        Err(heavenly_core::Error::OutOfRange { .. })
    ));
}

/// Validation against the real DE440 kernels. Run with
/// `HEAVENLY_KERNELS=<dir> cargo test -p heavenly-spice -- --ignored`,
/// where `<dir>` holds `de440s.bsp` and `moon_pa_de440_200625.bpc`.
#[test]
#[ignore = "needs JPL kernels; set HEAVENLY_KERNELS"]
fn de440_matches_spice() {
    let dir = PathBuf::from(std::env::var("HEAVENLY_KERNELS").expect("set HEAVENLY_KERNELS"));
    let spk = Spk::open(dir.join("de440s.bsp")).unwrap();
    check_states(&spk, data::DE440S_STATES, 1e-3, 1e-9);
    let pck = Pck::open(dir.join("moon_pa_de440_200625.bpc")).unwrap();
    // The prime-meridian angle reaches thousands of radians over the
    // kernel's span, where an f64 ulp is ~5e-13 rad: 1e-11 rad ≈ 17 µm.
    check_pck(&pck, data::DE440_MOON_PA, 1e-11);
}
