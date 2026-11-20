//! Single-item methods in a loop vs the batch methods.
//!
//! `cargo bench -p heavenly-core --bench batch` (add
//! `RUSTFLAGS="-C target-cpu=native"` for AVX on x86_64, and
//! `--no-default-features --features std` to see the scalar fallback).

use std::hint::black_box;
use std::time::Instant;

use heavenly_core::{
    Epoch, FrameGraph, FrameId, Namespace, Rotation, StateVector, Transform, Vec3,
};

const N: usize = 4096;

fn time(label: &str, reps: usize, mut f: impl FnMut()) -> f64 {
    for _ in 0..reps / 10 + 1 {
        f(); // warm up
    }
    let t = Instant::now();
    for _ in 0..reps {
        f();
    }
    let ns = t.elapsed().as_nanos() as f64 / (reps * N) as f64;
    println!("{label:<52} {ns:6.2} ns per item");
    ns
}

fn main() {
    let vs: Vec<Vec3> = (0..N)
        .map(|k| Vec3::new(k as f64, 1.5 * k as f64, -0.5 * k as f64 + 3.0))
        .collect();
    let r = Rotation::from_quaternion(0.9, 0.1, -0.3, 0.2).unwrap();
    let t = Transform::new(
        r,
        Vec3::new(1e3, 2e3, 3e3),
        Vec3::new(4.0, 5.0, 6.0),
        Vec3::new(0.0, 0.0, 7.29e-5),
    );
    let reps = 2000;
    println!("simd feature: {}", cfg!(feature = "simd"));

    let mut out = vs.clone();
    let a = time("Rotation::apply, in a loop", reps, || {
        out.copy_from_slice(black_box(&vs));
        for o in out.iter_mut() {
            *o = r.apply(*o);
        }
        black_box(&out);
    });
    let b = time("Rotation::apply_batch", reps, || {
        out.copy_from_slice(&vs);
        r.apply_batch(black_box(&mut out));
        black_box(&out);
    });
    println!("{:>52} {:6.2}x", "speed-up", a / b);

    let (mut p, mut v) = (vs.clone(), vs.clone());
    let a = time(
        "Transform::apply (position + velocity), in a loop",
        reps,
        || {
            let (p, v) = (black_box(&mut p), black_box(&mut v));
            for (pp, vv) in p.iter_mut().zip(v.iter_mut()) {
                (*pp, *vv) = t.apply(*pp, *vv);
            }
            black_box((&p, &v));
        },
    );
    let b = time("Transform::apply_batch", reps, || {
        t.apply_batch(black_box(&mut p), black_box(&mut v)).unwrap();
        black_box((&p, &v));
    });
    println!("{:>52} {:6.2}x", "speed-up", a / b);

    // Many objects at one epoch through a two-edge frame graph.
    let f = |c| FrameId::custom(Namespace(3), c).unwrap();
    let mut g = FrameGraph::new();
    g.add_fixed(f(1), f(2), Transform::fixed(r, Vec3::new(1.0, 2.0, 3.0)))
        .unwrap();
    g.add_edge(f(2), f(3), move |_| Ok(t)).unwrap();
    let states: Vec<StateVector> = vs
        .iter()
        .map(|p| StateVector::new(f(1), Epoch::J2000, *p, *p * 1e-3))
        .collect();
    let mut moved = states.clone();
    let a = time(
        "StateVector::to_frame, in a loop (one epoch)",
        reps / 10,
        || {
            for (m, s) in moved.iter_mut().zip(&states) {
                *m = black_box(s).to_frame(f(3), &g).unwrap();
            }
            black_box(&moved);
        },
    );
    let b = time("StateVector::to_frame_batch (one epoch)", reps / 10, || {
        moved.copy_from_slice(&states);
        StateVector::to_frame_batch(black_box(&mut moved), f(3), &g).unwrap();
        black_box(&moved);
    });
    println!("{:>52} {:6.2}x", "speed-up", a / b);
}
