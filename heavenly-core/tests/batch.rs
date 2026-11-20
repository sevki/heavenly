//! The batch operations give bit-identical results to the single-item
//! methods, with or without the `simd` feature.

use heavenly_core::{
    Epoch, Error, FrameGraph, FrameId, FrameResolver, Namespace, Rotation, StateVector, Transform,
    Vec3,
};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
    /// A value of any sign over 30 orders of magnitude, sometimes ±0.
    fn value(&mut self) -> f64 {
        match (self.next() * 8.0) as i64 {
            0 => 0.0,
            -1 => -0.0,
            _ => self.next() * 10f64.powf(self.next() * 15.0),
        }
    }
    fn vec(&mut self) -> Vec3 {
        Vec3::new(self.value(), self.value(), self.value())
    }
    fn rotation(&mut self) -> Rotation {
        Rotation::from_quaternion(self.next(), self.next(), self.next(), self.next()).unwrap()
    }
}

fn bits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

#[test]
fn rotation_batch_is_bit_identical() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for n in 0..14 {
        for _ in 0..50 {
            let r = rng.rotation();
            let vs: Vec<Vec3> = (0..n).map(|_| rng.vec()).collect();
            let mut batch = vs.clone();
            r.apply_batch(&mut batch);
            for (v, b) in vs.iter().zip(&batch) {
                assert_eq!(bits(r.apply(*v)), bits(*b), "{v:?}");
            }
        }
    }
}

#[test]
fn transform_batches_are_bit_identical() {
    let mut rng = Rng(42);
    for n in 0..14 {
        for _ in 0..50 {
            let t = Transform::new(rng.rotation(), rng.vec(), rng.vec(), rng.vec());
            let ps: Vec<Vec3> = (0..n).map(|_| rng.vec()).collect();
            let vs: Vec<Vec3> = (0..n).map(|_| rng.vec()).collect();
            let mut only_p = ps.clone();
            t.apply_positions(&mut only_p);
            let (mut bp, mut bv) = (ps.clone(), vs.clone());
            t.apply_batch(&mut bp, &mut bv).unwrap();
            for k in 0..n {
                let (p, v) = t.apply(ps[k], vs[k]);
                assert_eq!(bits(p), bits(bp[k]));
                assert_eq!(bits(v), bits(bv[k]));
                assert_eq!(bits(t.apply_position(ps[k])), bits(only_p[k]));
            }
        }
    }
    let t = Transform::default();
    assert!(t
        .apply_batch(&mut [Vec3::ZERO; 2], &mut [Vec3::ZERO; 3])
        .is_err());
}

#[test]
fn states_move_between_frames_identically() {
    let mut rng = Rng(7);
    let a = FrameId::custom(Namespace(9), 1).unwrap();
    let b = FrameId::custom(Namespace(9), 2).unwrap();
    let mut g = FrameGraph::new();
    let (rot, omega) = (rng.rotation(), Vec3::new(0.0, 0.0, 7.29e-5));
    g.add_edge(a, b, move |e: Epoch| {
        let s = e.tt_seconds_since_j2000();
        Ok(Transform::new(
            rot,
            Vec3::new(s, 1.0, 2.0),
            Vec3::new(3.0, s, 4.0),
            omega,
        ))
    })
    .unwrap();
    let t0 = Epoch::J2000;
    let t1 = t0.add_seconds(60.0).unwrap();
    // Runs of shared (frame, epoch), including one already in the
    // destination frame and a run shorter than four.
    let mut states = Vec::new();
    for (frame, epoch, n) in [(a, t0, 9), (a, t1, 3), (b, t1, 5), (a, t0, 6)] {
        for _ in 0..n {
            states.push(StateVector::new(frame, epoch, rng.vec(), rng.vec()));
        }
    }
    let mut batch = states.clone();
    StateVector::to_frame_batch(&mut batch, b, &g).unwrap();
    for (s, out) in states.iter().zip(&batch) {
        let one = s.to_frame(b, &g).unwrap();
        assert_eq!(one.frame, out.frame);
        assert_eq!(bits(one.position_m), bits(out.position_m));
        assert_eq!(bits(one.velocity_m_s), bits(out.velocity_m_s));
    }
    // A frame transform refuses states from another frame or epoch, and
    // leaves them all untouched.
    let ft = g.transform(a, b, t0).unwrap();
    let mut mixed = states[8..10].to_vec(); // the last of t0, the first of t1
    let before = mixed.clone();
    assert!(matches!(
        ft.apply_batch(&mut mixed),
        Err(Error::EpochMismatch { .. })
    ));
    assert_eq!(mixed, before);
}
