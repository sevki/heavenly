//! Accuracy and cost of the numerical propagator's tolerance: a day of a
//! low orbit under two-body forces, against Kepler's solution.
//!
//! `cargo run --release -p heavenly-orbit --example tolerance`

use heavenly_core::{Epoch, FrameId, StateVector};
use heavenly_orbit::{gm, kepler, ConicElements, ForceModel, NumericalPropagator, Tolerance};

fn main() {
    let e0 = Epoch::from_tt_seconds_since_j2000(8.36e8).unwrap();
    let el = ConicElements {
        periapsis_m: 6_878_137.0,
        eccentricity: 0.001,
        inclination_rad: 0.9,
        ascending_node_rad: 0.3,
        argument_of_periapsis_rad: 0.7,
        mean_anomaly_rad: 0.2,
        epoch: e0,
        gm_m3_s2: gm::EARTH,
    };
    let (r0, v0) = el.state_at(e0).unwrap();
    let day = 86_400.0;
    let (r1, _) = kepler::propagate(gm::EARTH, r0, v0, day).unwrap();
    println!("relative  absolute  error after 1 d   time (two-body / full model, 1 d)");
    for (rel, abs) in [
        (1e-10, 1e-7),
        (1e-11, 1e-8),
        (1e-12, 1e-9),
        (1e-13, 1e-10),
        (1e-14, 1e-11),
    ] {
        let mut p = NumericalPropagator::new(
            StateVector::new(FrameId::GCRF, e0, r0, v0),
            ForceModel::two_body(gm::EARTH),
        )
        .unwrap();
        p.tolerance = Tolerance {
            relative: rel,
            absolute: abs,
        };
        let t = std::time::Instant::now();
        let got = p.propagate(e0.add_seconds(day).unwrap()).unwrap();
        let two_body = t.elapsed();
        p.forces = ForceModel::earth();
        let t = std::time::Instant::now();
        p.propagate(e0.add_seconds(day).unwrap()).unwrap();
        println!(
            "{rel:8.0e}  {abs:8.0e}  {:12.2e} m   {two_body:?} / {:?}",
            (got.position_m - r1).norm(),
            t.elapsed()
        );
    }
}
