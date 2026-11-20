//! Probability of collision against an independent implementation
//! (`tests/reference/generate_collision.py`) and Monte Carlo, and covariance
//! through frames and RTN.

#[rustfmt::skip]
#[path = "reference/collision_data.rs"]
mod data;

use heavenly_core::{
    Covariance, Epoch, Error, FrameGraph, FrameId, Matrix6, StateVector, UncertainState, Vec3,
};
use heavenly_geo::earth;
use heavenly_query::collision_probability;

fn v3(a: &[f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn position_cov(c: &[f64; 9]) -> Covariance {
    let mut m = [[0.0; 6]; 6];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = c[3 * i + j];
        }
        m[i + 3][i + 3] = 1e-4;
    }
    Covariance::new(m).unwrap()
}

fn uncertain(r: &[f64; 3], v: &[f64; 3], c: &[f64; 9]) -> UncertainState {
    UncertainState::new(
        StateVector::new(FrameId::GCRF, Epoch::J2000, v3(r), v3(v)),
        position_cov(c),
    )
}

#[test]
fn probability_matches_the_independent_reference() {
    let mut worst = 0.0f64;
    for (ra, va, ca, rb, vb, cb, radius, p) in data::PC {
        let got =
            collision_probability(&uncertain(ra, va, ca), &uncertain(rb, vb, cb), *radius).unwrap();
        worst = worst.max((got.probability - p).abs() / p);
    }
    eprintln!(
        "collision probability: worst relative error {worst:.1e} over {} encounters",
        data::PC.len()
    );
    assert!(worst < 1e-7, "{worst}");
}

// A small deterministic generator: xorshift64* and Box–Muller.
struct Rng(u64);
impl Rng {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        (-2.0 * self.uniform().ln()).sqrt() * (std::f64::consts::TAU * self.uniform()).cos()
    }
}

#[test]
fn probability_matches_monte_carlo() {
    // Sample the relative position from the combined covariance, fly it in a
    // straight line, and count the passes within the radius: no encounter
    // plane, no projection.
    let (ra, va, ca, rb, vb, cb, radius, _) =
        data::PC.iter().max_by(|x, y| x.7.total_cmp(&y.7)).unwrap();
    let (a, b) = (uncertain(ra, va, ca), uncertain(rb, vb, cb));
    let p = collision_probability(&a, &b, *radius).unwrap().probability;
    let c = a.covariance.plus(&b.covariance).unwrap().position();
    // Cholesky of the 3×3 position covariance.
    let mut l = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..=i {
            let s: f64 = c[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            l[i][j] = if i == j { s.sqrt() } else { s / l[j][j] };
        }
    }
    let r = v3(rb) - v3(ra);
    let z = (v3(vb) - v3(va)).normalized().unwrap();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let n = 2_000_000;
    let mut hits = 0u64;
    for _ in 0..n {
        let e = [rng.normal(), rng.normal(), rng.normal()];
        let d = Vec3::new(
            l[0][0] * e[0],
            l[1][0] * e[0] + l[1][1] * e[1],
            l[2][0] * e[0] + l[2][1] * e[1] + l[2][2] * e[2],
        );
        let q = r + d;
        if (q - z * q.dot(z)).norm() < *radius {
            hits += 1;
        }
    }
    let mc = hits as f64 / n as f64;
    let sigma = (p * (1.0 - p) / n as f64).sqrt();
    eprintln!("collision probability {p:.5e}, Monte Carlo {mc:.5e} ± {sigma:.1e}");
    assert!((mc - p).abs() < 4.0 * sigma, "{mc} vs {p}");
}

#[test]
fn inputs_are_checked() {
    let (ra, va, ca, rb, vb, cb, _, _) = &data::PC[0];
    let (a, b) = (uncertain(ra, va, ca), uncertain(rb, vb, cb));
    let mut itrf = b;
    itrf.state.frame = FrameId::ITRF;
    assert!(matches!(
        collision_probability(&a, &itrf, 10.0),
        Err(Error::FrameMismatch { .. })
    ));
    let mut later = b;
    later.state.epoch = Epoch::J2000.add_seconds(1.0).unwrap();
    assert!(matches!(
        collision_probability(&a, &later, 10.0),
        Err(Error::EpochMismatch { .. })
    ));
    let mut together = b;
    together.state.velocity_m_s = a.state.velocity_m_s;
    assert!(matches!(
        collision_probability(&a, &together, 10.0),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        collision_probability(&a, &b, 0.0),
        Err(Error::InvalidInput(_))
    ));
    // A covariance must be symmetric and positive semi-definite.
    let mut m = [[0.0; 6]; 6];
    m[0][1] = 1.0;
    assert!(Covariance::new(m).is_err());
    let mut m = [[0.0; 6]; 6];
    m[0][0] = -1.0;
    assert!(Covariance::new(m).is_err());
}

#[test]
fn covariance_follows_a_rotating_frame() {
    // GCRF → ITRF rotates with the Earth: the velocity covariance picks up
    // ω × (position error). Compare with a finite-difference Jacobian of
    // the state transform itself.
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    let epoch = Epoch::parse("2026-11-19T17:00:00Z").unwrap();
    let s = StateVector::new(
        FrameId::GCRF,
        epoch,
        Vec3::new(6.9e6, 1.0e5, 2.0e5),
        Vec3::new(-100.0, 5.0e3, 5.5e3),
    );
    let mut m = [[0.0; 6]; 6];
    let sig = [300.0, 1_000.0, 200.0, 0.5, 0.2, 0.3];
    for i in 0..6 {
        m[i][i] = sig[i] * sig[i];
    }
    m[0][1] = 2.0e5;
    m[1][0] = 2.0e5;
    m[1][3] = -50.0;
    m[3][1] = -50.0;
    let c = Covariance::new(m).unwrap();
    let got = UncertainState::new(s, c)
        .to_frame(FrameId::ITRF, &graph)
        .unwrap();
    // Finite-difference Jacobian of to_frame.
    let mut j: Matrix6 = [[0.0; 6]; 6];
    for col in 0..6 {
        let h = if col < 3 { 1.0 } else { 1e-3 };
        let f = |sign: f64| {
            let mut x = [s.position_m.to_array(), s.velocity_m_s.to_array()];
            x[col / 3][col % 3] += sign * h;
            let t = StateVector {
                position_m: Vec3::from_array(x[0]),
                velocity_m_s: Vec3::from_array(x[1]),
                ..s
            }
            .to_frame(FrameId::ITRF, &graph)
            .unwrap();
            [t.position_m.to_array(), t.velocity_m_s.to_array()]
        };
        let (up, down) = (f(1.0), f(-1.0));
        for row in 0..6 {
            j[row][col] = (up[row / 3][row % 3] - down[row / 3][row % 3]) / (2.0 * h);
        }
    }
    let want = c.transformed(&j).unwrap().matrix();
    let have = got.covariance.matrix();
    for row in 0..6 {
        for col in 0..6 {
            let scale = (want[row][row] * want[col][col]).sqrt();
            assert!(
                (have[row][col] - want[row][col]).abs() < 1e-6 * scale,
                "{row},{col}: {} vs {}",
                have[row][col],
                want[row][col]
            );
        }
    }
    // The position uncertainty's size is unchanged by a rotation.
    let trace = |m: Matrix6| m[0][0] + m[1][1] + m[2][2];
    assert!((trace(have) - trace(m)).abs() < 1e-9 * trace(m));
    // …and the state is the transformed state.
    assert_eq!(got.state, s.to_frame(FrameId::ITRF, &graph).unwrap());
}

#[test]
fn rtn_round_trip() {
    let s = StateVector::new(
        FrameId::GCRF,
        Epoch::J2000,
        Vec3::new(7e6, 0.0, 0.0),
        Vec3::new(0.0, 5e3, 5e3),
    );
    // 100 m radial, 1 km along track, 50 m cross track.
    let rtn = Covariance::diagonal([100.0, 1_000.0, 50.0], [1.0, 0.1, 0.05]).unwrap();
    let inertial = Covariance::from_rtn(&rtn, &s).unwrap();
    let m = inertial.matrix();
    // Radial is x; along-track and cross-track share y and z equally.
    assert!((m[0][0] - 1e4).abs() < 1e-6);
    assert!((m[1][1] - 0.5 * (1e6 + 2500.0)).abs() < 1e-3);
    // Transverse is (0, 1, 1)/√2 and normal (0, −1, 1)/√2.
    assert!((m[1][2] - 0.5 * (1e6 - 2500.0)).abs() < 1e-3);
    let back = inertial.to_rtn(&s).unwrap().matrix();
    for (a, b) in back.iter().flatten().zip(rtn.matrix().iter().flatten()) {
        assert!((a - b).abs() < 1e-6 * (1.0 + b.abs()));
    }
}
