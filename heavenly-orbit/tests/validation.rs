//! Validation against SPICE (`prop2b`, `conics`, `oscelt`); values from
//! `tests/reference/generate.py`.

#[rustfmt::skip]
#[path = "reference/data.rs"]
mod data;

use heavenly_core::{
    Epoch, Error, FrameGraph, FrameId, Locator, Namespace, Rotation, StateVector, Transform, Vec3,
};
use heavenly_orbit::{gm, kepler, ConicElements, SampledTrajectory, TwoBodyLocator};

fn pv(s: &[f64; 6]) -> (Vec3, Vec3) {
    (
        Vec3::new(s[0], s[1], s[2]) * 1e3,
        Vec3::new(s[3], s[4], s[5]) * 1e3,
    )
}

fn et(e: f64) -> Epoch {
    Epoch::from_tdb_seconds_since_j2000(e).unwrap()
}

/// Relative error of a state against a reference.
fn rel_err((p, v): (Vec3, Vec3), (rp, rv): (Vec3, Vec3)) -> (f64, f64) {
    ((p - rp).norm() / rp.norm(), (v - rv).norm() / rv.norm())
}

#[test]
fn propagation_matches_spice_prop2b() {
    let mut worst = (0.0f64, 0.0f64);
    for (gm_km, s0, dt, s1) in data::PROP2B {
        let (r0, v0) = pv(s0);
        let got = kepler::propagate(gm_km * 1e9, r0, v0, *dt).unwrap();
        let e = rel_err(got, pv(s1));
        worst = (worst.0.max(e.0), worst.1.max(e.1));
    }
    eprintln!(
        "prop2b worst relative error: position {:.2e}, velocity {:.2e}",
        worst.0, worst.1
    );
    assert!(worst.0 < 1e-9 && worst.1 < 1e-9, "{worst:?}");
}

fn elements_from_spice(el: &[f64; 8]) -> ConicElements {
    ConicElements {
        periapsis_m: el[0] * 1e3,
        eccentricity: el[1],
        inclination_rad: el[2],
        ascending_node_rad: el[3],
        argument_of_periapsis_rad: el[4],
        mean_anomaly_rad: el[5],
        epoch: et(el[6]),
        gm_m3_s2: el[7] * 1e9,
    }
}

#[test]
fn elements_to_state_matches_spice_conics() {
    let mut worst = (0.0f64, 0.0f64);
    for (el, t, st) in data::CONICS {
        let got = elements_from_spice(el).state_at(et(*t)).unwrap();
        let e = rel_err(got, pv(st));
        worst = (worst.0.max(e.0), worst.1.max(e.1));
    }
    eprintln!(
        "conics worst relative error: position {:.2e}, velocity {:.2e}",
        worst.0, worst.1
    );
    assert!(worst.0 < 1e-9 && worst.1 < 1e-9, "{worst:?}");
}

#[test]
fn state_to_elements_matches_spice_oscelt() {
    let angle = |a: f64, b: f64| {
        let d = (a - b).rem_euclid(core::f64::consts::TAU);
        d.min(core::f64::consts::TAU - d)
    };
    for (st, t, gm_km, el) in data::OSCELT {
        let (p, v) = pv(st);
        let got = ConicElements::from_state(p, v, et(*t), gm_km * 1e9).unwrap();
        let want = elements_from_spice(el);
        assert!(
            (got.periapsis_m - want.periapsis_m).abs() < 1e-9 * want.periapsis_m,
            "q {got:?} {want:?}"
        );
        assert!(
            (got.eccentricity - want.eccentricity).abs() < 1e-9,
            "e {got:?} {want:?}"
        );
        for (a, b) in [
            (got.inclination_rad, want.inclination_rad),
            (got.ascending_node_rad, want.ascending_node_rad),
            (
                got.argument_of_periapsis_rad,
                want.argument_of_periapsis_rad,
            ),
        ] {
            assert!(angle(a, b) < 1e-9, "angles {got:?} {want:?}");
        }
        // Mean anomaly: periodic for ellipses, unbounded for hyperbolas.
        let dm = if got.eccentricity < 1.0 {
            angle(got.mean_anomaly_rad, want.mean_anomaly_rad)
        } else {
            (got.mean_anomaly_rad - want.mean_anomaly_rad).abs()
                / want.mean_anomaly_rad.abs().max(1.0)
        };
        assert!(dm < 1e-8, "M {got:?} {want:?}");
    }
}

#[test]
fn elements_round_trip_including_degenerate_orbits() {
    let epoch = Epoch::J2000;
    let cases = [
        // circular, inclined
        (7.0e6, 0.0, 0.9, 1.1, 0.0),
        // equatorial, eccentric
        (7.0e6, 0.3, 0.0, 0.0, 0.7),
        // circular equatorial
        (4.2164e7, 0.0, 0.0, 0.0, 0.0),
        // retrograde equatorial
        (8.0e6, 0.1, core::f64::consts::PI, 0.0, 2.0),
        // parabolic
        (1.0e7, 1.0, 0.4, 2.0, 1.0),
    ];
    for (q, e, i, node, argp) in cases {
        let el = ConicElements {
            periapsis_m: q,
            eccentricity: e,
            inclination_rad: i,
            ascending_node_rad: node,
            argument_of_periapsis_rad: argp,
            mean_anomaly_rad: 0.4,
            epoch,
            gm_m3_s2: gm::EARTH,
        };
        let (p, v) = el.state_at(epoch).unwrap();
        let back = ConicElements::from_state(p, v, epoch, gm::EARTH).unwrap();
        // Whatever angle conventions apply, the state must be reproduced.
        let (p2, v2) = back.state_at(epoch.add_seconds(1234.5).unwrap()).unwrap();
        let (p1, v1) = el.state_at(epoch.add_seconds(1234.5).unwrap()).unwrap();
        assert!((p1 - p2).norm() < 1e-6 * p1.norm(), "{el:?} -> {back:?}");
        assert!((v1 - v2).norm() < 1e-6 * v1.norm());
    }
}

#[test]
fn invalid_inputs_are_rejected() {
    let r = Vec3::new(7e6, 0.0, 0.0);
    assert_eq!(
        kepler::propagate(gm::EARTH, Vec3::ZERO, Vec3::Y, 1.0),
        Err(Error::InvalidInput(
            "position is at the central body's centre"
        ))
    );
    assert!(matches!(
        kepler::propagate(gm::EARTH, r, r * 1e-3, 1.0),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        kepler::propagate(f64::NAN, r, Vec3::Y, 1.0),
        Err(Error::NonFinite)
    );
    assert!(matches!(
        kepler::propagate(-1.0, r, Vec3::Y, 1.0),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        kepler::propagate(gm::EARTH, r, Vec3::Y * 7e3, 0.0),
        Ok((r, Vec3::Y * 7e3))
    );
}

#[test]
fn energy_and_angular_momentum_are_conserved() {
    let (r0, v0) = (Vec3::new(7e6, 1e5, -2e5), Vec3::new(-100.0, 7.4e3, 1.2e3));
    let energy = |r: Vec3, v: Vec3| v.norm_squared() / 2.0 - gm::EARTH / r.norm();
    for dt in [-1e7, -3600.0, 1.0, 5400.0, 1e5, 1e8] {
        let (r, v) = kepler::propagate(gm::EARTH, r0, v0, dt).unwrap();
        assert!(
            (energy(r, v) - energy(r0, v0)).abs() < 1e-9 * energy(r0, v0).abs(),
            "dt {dt}"
        );
        assert!((r.cross(v) - r0.cross(v0)).norm() < 1e-9 * r0.cross(v0).norm());
    }
}

fn leo(epoch: Epoch) -> TwoBodyLocator {
    TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_778_137.0,
            eccentricity: 0.001,
            inclination_rad: 51.6f64.to_radians(),
            ascending_node_rad: 1.0,
            argument_of_periapsis_rad: 0.5,
            mean_anomaly_rad: 0.0,
            epoch,
            gm_m3_s2: gm::EARTH,
        },
    }
}

fn sample(
    orbit: &TwoBodyLocator,
    start: Epoch,
    step_s: f64,
    n: usize,
    skip: impl Fn(usize) -> bool,
) -> Vec<StateVector> {
    (0..n)
        .filter(|i| !skip(*i))
        .map(|i| {
            orbit
                .state(start.add_seconds(i as f64 * step_s).unwrap())
                .unwrap()
        })
        .collect()
}

#[test]
fn sampled_trajectory_interpolates_within_the_hermite_bound() {
    let start = Epoch::parse("2026-11-19T00:00:00Z").unwrap();
    let orbit = leo(start);
    for (step, bound_m) in [(10.0, 5e-4), (60.0, 0.5)] {
        let traj = SampledTrajectory::new(sample(&orbit, start, step, 200, |_| false), step * 1.5)
            .unwrap();
        let mut worst = (0.0f64, 0.0f64);
        for k in 0..2000 {
            let t = start
                .add_seconds(k as f64 * step * 199.0 / 2000.0 + 0.37)
                .unwrap();
            let s = traj.state(t).unwrap();
            let truth = orbit.state(t).unwrap();
            worst.0 = worst.0.max((s.position_m - truth.position_m).norm());
            worst.1 = worst.1.max((s.velocity_m_s - truth.velocity_m_s).norm());
        }
        // Bound: h⁴/384 · r·ω⁴ for a near-circular orbit.
        let omega = orbit.elements.mean_motion_rad_s();
        let theory = step.powi(4) / 384.0 * 6.78e6 * omega.powi(4);
        eprintln!(
            "h = {step} s: worst {:.3e} m, {:.3e} m/s (bound {theory:.3e} m)",
            worst.0, worst.1
        );
        // The bound assumes a circular orbit; e = 0.001 adds ~1%.
        assert!(worst.0 <= theory * 1.05 && worst.0 < bound_m, "{worst:?}");
    }
}

#[test]
fn sampled_trajectory_failure_modes() {
    let start = Epoch::parse("2026-11-19T00:00:00Z").unwrap();
    let orbit = leo(start);
    // Samples every 60 s, with samples 10–19 missing (a 660 s gap).
    let traj = SampledTrajectory::new(
        sample(&orbit, start, 60.0, 50, |i| (10..20).contains(&i)),
        120.0,
    )
    .unwrap();
    let at = |s: f64| start.add_seconds(s).unwrap();

    assert!(matches!(
        traj.state(at(-1.0)),
        Err(Error::OutsideCoverage { .. })
    ));
    assert!(matches!(
        traj.state(at(49.0 * 60.0 + 1.0)),
        Err(Error::OutsideCoverage { .. })
    ));
    match traj.state(at(15.0 * 60.0)) {
        Err(Error::DataGap { before, after, .. }) => {
            assert_eq!((before, after), (at(9.0 * 60.0), at(20.0 * 60.0)));
        }
        other => panic!("{other:?}"),
    }
    // Exactly on a sample, and either side of the gap, it works.
    assert_eq!(traj.state(at(9.0 * 60.0)).unwrap(), traj.samples()[9]);
    assert!(traj.state(at(20.0 * 60.0 + 30.0)).is_ok());
    assert!(traj.state(at(49.0 * 60.0)).is_ok());

    // Construction errors.
    let s = traj.samples().to_vec();
    assert!(matches!(
        SampledTrajectory::new(s[..1].to_vec(), 60.0),
        Err(Error::InvalidInput(_))
    ));
    let mut unsorted = s.clone();
    unsorted.swap(0, 1);
    assert!(matches!(
        SampledTrajectory::new(unsorted, 60.0),
        Err(Error::InvalidInput(_))
    ));
    let mut mixed = s.clone();
    mixed[3].frame = FrameId::ITRF;
    assert!(matches!(
        SampledTrajectory::new(mixed, 60.0),
        Err(Error::FrameMismatch { .. })
    ));
    let mut bad = s;
    bad[2].position_m.x = f64::NAN;
    assert_eq!(SampledTrajectory::new(bad, 60.0), Err(Error::NonFinite));
}

#[test]
fn locators_resolve_through_the_frame_graph() {
    let start = Epoch::parse("2026-11-19T00:00:00Z").unwrap();
    let orbit = leo(start);
    // A module frame on the spacecraft is not needed here; resolve the
    // spacecraft into a frame offset from GCRF.
    let offset = FrameId::custom(Namespace(9), 1).unwrap();
    let mut graph = FrameGraph::new();
    graph
        .add_fixed(
            offset,
            FrameId::GCRF,
            Transform::fixed(Rotation::IDENTITY, Vec3::new(1e3, 0.0, 0.0)),
        )
        .unwrap();
    let t = start.add_seconds(1000.0).unwrap();
    let a = orbit.state_at(t, offset, &graph).unwrap();
    let b = orbit.state(t).unwrap();
    assert_eq!(a.frame, offset);
    assert!((a.position_m - (b.position_m - Vec3::new(1e3, 0.0, 0.0))).norm() < 1e-6);

    let traj = SampledTrajectory::new(sample(&orbit, start, 10.0, 600, |_| false), 15.0).unwrap();
    let c = traj.state_at(t, offset, &graph).unwrap();
    assert!((c.position_m - a.position_m).norm() < 1e-3);
    // A frame the graph does not know is an explicit error.
    assert!(matches!(
        traj.state_at(t, FrameId::MOON_ICRF, &graph),
        Err(Error::NoPath { .. })
    ));
}
