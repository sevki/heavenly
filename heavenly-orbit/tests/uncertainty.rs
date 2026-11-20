//! Covariance propagation: the finite-difference state transition matrix
//! against the structure of Hamiltonian flow, Monte Carlo, and the
//! along-track drift of a circular orbit.

use heavenly_core::{Covariance, Epoch, FrameId, Matrix6, StateVector, UncertainState, Vec3};
use heavenly_orbit::forces::EARTH_RADIUS_M;
use heavenly_orbit::uncertainty::{propagate_uncertain, state_transition_matrix};
use heavenly_orbit::{gm, kepler, ConicElements, ForceModel, NumericalPropagator, Zonal};

fn start(alt_km: f64, e: f64) -> StateVector {
    let epoch = Epoch::from_tt_seconds_since_j2000(8.36e8).unwrap();
    let el = ConicElements {
        periapsis_m: EARTH_RADIUS_M + alt_km * 1e3,
        eccentricity: e,
        inclination_rad: 0.9,
        ascending_node_rad: 0.3,
        argument_of_periapsis_rad: 0.7,
        mean_anomaly_rad: 0.2,
        epoch,
        gm_m3_s2: gm::EARTH,
    };
    let (r, v) = el.state_at(epoch).unwrap();
    StateVector::new(FrameId::GCRF, epoch, r, v)
}

fn kepler_to(dt: f64) -> impl Fn(&StateVector) -> Result<StateVector, heavenly_core::Error> {
    move |s: &StateVector| {
        let (r, v) = kepler::propagate(gm::EARTH, s.position_m, s.velocity_m_s, dt)?;
        Ok(StateVector::new(s.frame, s.epoch.add_seconds(dt)?, r, v))
    }
}

/// max |Φᵀ J Φ − J| with J = [[0, I], [−I, 0]], in units where position and
/// velocity are scaled by the orbit's own size (r, v), so every entry is
/// comparable.
fn symplectic_defect(phi: &Matrix6, r: f64, v: f64) -> f64 {
    let s = |i: usize| if i < 3 { r } else { v };
    let mut p = [[0.0; 6]; 6];
    for i in 0..6 {
        for j in 0..6 {
            p[i][j] = phi[i][j] * s(j) / s(i);
        }
    }
    let jm = |i: usize, k: usize| match (i < 3, k < 3) {
        (true, false) if k == i + 3 => 1.0,
        (false, true) if i == k + 3 => -1.0,
        _ => 0.0,
    };
    let mut worst = 0.0f64;
    for a in 0..6 {
        for b in 0..6 {
            let mut x = 0.0;
            for i in 0..6 {
                for k in 0..6 {
                    x += p[i][a] * jm(i, k) * p[k][b];
                }
            }
            // In scaled units J picks up r·v / (r·v) = 1 on the
            // off-diagonal blocks.
            worst = worst.max((x - jm(a, b)).abs());
        }
    }
    worst
}

#[test]
fn two_body_transition_matrix_is_symplectic() {
    for (alt, e, dt) in [
        (500.0, 0.001, 5_400.0),
        (600.0, 0.74, 40_000.0),
        (500.0, 0.001, 86_400.0),
    ] {
        let s = start(alt, e);
        let phi = state_transition_matrix(&s, kepler_to(dt)).unwrap();
        let d = symplectic_defect(&phi, s.position_m.norm(), s.velocity_m_s.norm());
        eprintln!("two-body STM, {alt} km e={e}, {dt} s: symplectic defect {d:.1e}");
        assert!(d < 1e-6, "{d}");
    }
}

#[test]
fn numerical_j2_transition_matrix_is_symplectic() {
    // J2 about a fixed axis is still a Hamiltonian flow.
    fn z(_: Epoch) -> Vec3 {
        Vec3::Z
    }
    let forces = ForceModel {
        zonal: Some(Zonal {
            pole: z,
            ..Zonal::earth(6)
        }),
        ..ForceModel::two_body(gm::EARTH)
    };
    let s = start(500.0, 0.01);
    let end = s.epoch.add_seconds(43_200.0).unwrap();
    let phi = state_transition_matrix(&s, |x| NumericalPropagator::new(*x, forces)?.propagate(end))
        .unwrap();
    let d = symplectic_defect(&phi, s.position_m.norm(), s.velocity_m_s.norm());
    eprintln!("numerical J2-J6 STM, 12 h: symplectic defect {d:.1e}");
    assert!(d < 1e-5, "{d}");
}

#[test]
fn along_track_error_follows_clohessy_wiltshire() {
    // In a circular orbit an along-track velocity error δv changes the
    // period, and the satellite falls behind (or ahead): Clohessy–Wiltshire
    // gives δy = (4/n) sin(nt) δv − 3 δv t.
    let s = start(500.0, 0.0);
    let dv = 0.01;
    let rtn = Covariance::diagonal([1e-3, 1e-3, 1e-3], [1e-6, dv, 1e-6]).unwrap();
    let c0 = Covariance::from_rtn(&rtn, &s).unwrap();
    let day = 86_400.0;
    let end = propagate_uncertain(&UncertainState::new(s, c0), kepler_to(day)).unwrap();
    let along = end.covariance.to_rtn(&end.state).unwrap().sigmas()[1];
    let n = (gm::EARTH / s.position_m.norm().powi(3)).sqrt();
    let cw = ((4.0 / n) * (n * day).sin() * dv - 3.0 * dv * day).abs();
    eprintln!(
        "along-track σ after a day from σv = {dv} m/s: {along:.1} m (Clohessy–Wiltshire {cw:.1} m)"
    );
    assert!((along / cw - 1.0).abs() < 1e-3, "{along} vs {cw}");
}

#[test]
fn linear_covariance_matches_monte_carlo() {
    // 100 m and 0.1 m/s errors over one orbit: sample, propagate each
    // sample, and compare the sample covariance with Φ C Φᵀ.
    let s = start(500.0, 0.001);
    let c0 = Covariance::diagonal([100.0, 60.0, 40.0], [0.1, 0.08, 0.05]).unwrap();
    let dt = 5_700.0;
    let linear = propagate_uncertain(&UncertainState::new(s, c0), kepler_to(dt)).unwrap();
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut normal = || {
        let mut u = || {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            ((seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        };
        (-2.0 * u().ln()).sqrt() * (std::f64::consts::TAU * u()).cos()
    };
    let sig = c0.sigmas();
    let n = 20_000;
    let mut sum = [0.0; 6];
    let mut sq = [[0.0; 6]; 6];
    let f = kepler_to(dt);
    for _ in 0..n {
        let d: [f64; 6] = std::array::from_fn(|i| sig[i] * normal());
        let x = StateVector {
            position_m: s.position_m + Vec3::new(d[0], d[1], d[2]),
            velocity_m_s: s.velocity_m_s + Vec3::new(d[3], d[4], d[5]),
            ..s
        };
        let y = f(&x).unwrap();
        let e: [f64; 6] = {
            let (p, v) = (
                y.position_m - linear.state.position_m,
                y.velocity_m_s - linear.state.velocity_m_s,
            );
            [p.x, p.y, p.z, v.x, v.y, v.z]
        };
        for i in 0..6 {
            sum[i] += e[i];
            for j in 0..6 {
                sq[i][j] += e[i] * e[j];
            }
        }
    }
    let lin = linear.covariance.matrix();
    let mut worst = 0.0f64;
    for i in 0..6 {
        let var = sq[i][i] / n as f64 - (sum[i] / n as f64).powi(2);
        worst = worst.max((var / lin[i][i] - 1.0).abs());
    }
    eprintln!("Monte Carlo ({n} samples) vs linear covariance after one orbit: worst variance ratio off by {:.1} %", 100.0 * worst);
    // Sampling error of a variance is √(2/n) ≈ 1 %.
    assert!(worst < 0.05, "{worst}");
}

#[test]
fn numerical_propagator_covariance_matches_two_body_when_forces_are_two_body() {
    let s = start(500.0, 0.001);
    let c0 = Covariance::diagonal([100.0, 60.0, 40.0], [0.1, 0.08, 0.05]).unwrap();
    let end = s.epoch.add_seconds(10_000.0).unwrap();
    let num = NumericalPropagator::new(s, ForceModel::two_body(gm::EARTH))
        .unwrap()
        .propagate_uncertain(&c0, end)
        .unwrap();
    let kep = propagate_uncertain(&UncertainState::new(s, c0), kepler_to(10_000.0)).unwrap();
    let (a, b) = (num.covariance.matrix(), kep.covariance.matrix());
    for i in 0..6 {
        for j in 0..6 {
            let scale = (b[i][i] * b[j][j]).sqrt();
            assert!(
                (a[i][j] - b[i][j]).abs() < 1e-4 * scale,
                "{i},{j}: {} vs {}",
                a[i][j],
                b[i][j]
            );
        }
    }
}
