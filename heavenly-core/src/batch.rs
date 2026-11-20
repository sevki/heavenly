//! Batch operations on many vectors and states, vectorised with SIMD.
//!
//! The batch methods ([`Rotation::apply_batch`],
//! [`Transform::apply_positions`], [`Transform::apply_batch`],
//! [`FrameTransform::apply_batch`] and [`StateVector::to_frame_batch`])
//! give **exactly** the same results as calling the single-item methods in
//! a loop, bit for bit, on every platform.
//!
//! [`StateVector::to_frame_batch`] resolves one transform per run of
//! states that share a frame and an epoch, instead of one per state. That
//! alone is 5–14× faster than [`StateVector::to_frame`] in a loop.
//!
//! **SIMD.** With the `simd` feature (on by default), position-and-velocity
//! transforms are processed four at a time with
//! [`wide`](https://docs.rs/wide)'s `f64x4`. That uses AVX, SSE2, NEON or
//! WebAssembly `simd128`, depending on the target, or plain scalar code.
//! Each lane does the same IEEE operations in the same order as the scalar
//! code, and fused multiply-add is never used, so rounding is identical.
//! Rotation-only batches stay plain loops, because the compiler already
//! vectorises them.
//!
//! Measured with `cargo bench -p heavenly-core --bench batch` (x86_64,
//! ns per state, `simd` off → on):
//!
//! | | baseline (SSE2) | `-C target-cpu=native` (AVX-512) |
//! |---|---|---|
//! | `Transform::apply_batch` | 12.1 → 7.7 | 12.0 → 3.9 |
//! | `StateVector::to_frame_batch` | 33.0 → 12.1 | 34.9 → 8.8 |

use crate::{
    Error, FrameId, FrameResolver, FrameTransform, Rotation, StateVector, Transform, Vec3,
};

#[cfg(feature = "simd")]
mod lanes {
    use crate::Vec3;
    use wide::f64x4;

    /// Four vectors, one per lane, as three registers.
    #[derive(Clone, Copy)]
    pub(super) struct V4 {
        pub x: f64x4,
        pub y: f64x4,
        pub z: f64x4,
    }

    impl V4 {
        #[inline(always)]
        pub fn load(v: &[Vec3]) -> V4 {
            V4 {
                x: f64x4::from([v[0].x, v[1].x, v[2].x, v[3].x]),
                y: f64x4::from([v[0].y, v[1].y, v[2].y, v[3].y]),
                z: f64x4::from([v[0].z, v[1].z, v[2].z, v[3].z]),
            }
        }

        #[inline(always)]
        pub fn store(self, out: &mut [Vec3]) {
            let (x, y, z) = (self.x.to_array(), self.y.to_array(), self.z.to_array());
            for k in 0..4 {
                out[k] = Vec3::new(x[k], y[k], z[k]);
            }
        }

        #[inline(always)]
        pub fn splat(v: Vec3) -> V4 {
            V4 {
                x: f64x4::splat(v.x),
                y: f64x4::splat(v.y),
                z: f64x4::splat(v.z),
            }
        }

        /// `Vec3::cross`, lane by lane, in the same order of operations.
        #[inline(always)]
        pub fn cross(self, o: V4) -> V4 {
            V4 {
                x: self.y * o.z - self.z * o.y,
                y: self.z * o.x - self.x * o.z,
                z: self.x * o.y - self.y * o.x,
            }
        }

        #[inline(always)]
        pub fn add(self, o: V4) -> V4 {
            V4 {
                x: self.x + o.x,
                y: self.y + o.y,
                z: self.z + o.z,
            }
        }

        #[inline(always)]
        pub fn scale(self, s: f64x4) -> V4 {
            V4 {
                x: self.x * s,
                y: self.y * s,
                z: self.z * s,
            }
        }
    }

    /// `Rotation::apply` on four vectors: `v + 2w(q × v) + q × (2 q × v)`.
    #[inline(always)]
    pub(super) fn rotate(q: V4, w: f64x4, v: V4) -> V4 {
        let t = q.cross(v).scale(f64x4::splat(2.0));
        v.add(t.scale(w)).add(q.cross(t))
    }
}

impl Rotation {
    /// Rotates every vector in `vs` in place. Identical to applying
    /// [`Rotation::apply`] to each one.
    ///
    /// This is a plain loop on purpose: the compiler already vectorises it,
    /// and an explicit SIMD kernel measured no faster (see the bench).
    pub fn apply_batch(self, vs: &mut [Vec3]) {
        for v in vs {
            *v = self.apply(*v);
        }
    }
}

impl Transform {
    /// Transforms positions in place (no velocities). Identical to
    /// [`Transform::apply_position`] on each one. A plain loop, like
    /// [`Rotation::apply_batch`].
    pub fn apply_positions(&self, positions: &mut [Vec3]) {
        for p in positions {
            *p = self.apply_position(*p);
        }
    }

    /// Transforms positions and velocities in place, pairwise. Identical
    /// to [`Transform::apply`] on each pair. Fails if the slices differ in
    /// length.
    pub fn apply_batch(
        &self,
        positions: &mut [Vec3],
        velocities: &mut [Vec3],
    ) -> Result<(), Error> {
        if positions.len() != velocities.len() {
            return Err(Error::InvalidInput(
                "positions and velocities differ in length",
            ));
        }
        #[cfg(feature = "simd")]
        {
            use lanes::{rotate, V4};
            let [qw, qx, qy, qz] = self.rotation.quaternion();
            let (q, w) = (V4::splat(Vec3::new(qx, qy, qz)), wide::f64x4::splat(qw));
            let (t, tv) = (
                V4::splat(self.translation_m),
                V4::splat(self.origin_velocity_m_s),
            );
            let omega = V4::splat(self.angular_velocity_rad_s);
            let n = positions.len() / 4 * 4;
            for (pc, vc) in positions[..n]
                .chunks_exact_mut(4)
                .zip(velocities[..n].chunks_exact_mut(4))
            {
                let rp = rotate(q, w, V4::load(pc));
                let vb = rotate(q, w, V4::load(vc)).add(omega.cross(rp)).add(tv);
                rp.add(t).store(pc);
                vb.store(vc);
            }
            for (p, v) in positions[n..].iter_mut().zip(&mut velocities[n..]) {
                (*p, *v) = self.apply(*p, *v);
            }
        }
        #[cfg(not(feature = "simd"))]
        for (p, v) in positions.iter_mut().zip(velocities.iter_mut()) {
            (*p, *v) = self.apply(*p, *v);
        }
        Ok(())
    }
}

impl FrameTransform {
    /// Moves every state into the destination frame, in place. Every state
    /// must be in the source frame at the transform's epoch; if one is not,
    /// nothing is changed and the error names it.
    pub fn apply_batch(&self, states: &mut [StateVector]) -> Result<(), Error> {
        for s in states.iter() {
            if s.frame != self.source {
                return Err(Error::FrameMismatch {
                    expected: self.source,
                    found: s.frame,
                });
            }
            if s.epoch != self.epoch {
                return Err(Error::EpochMismatch {
                    expected: self.epoch,
                    found: s.epoch,
                });
            }
        }
        transform_states(&self.transform, states);
        for s in states.iter_mut() {
            s.frame = self.destination;
        }
        Ok(())
    }
}

/// Applies a transform to the positions and velocities of states.
fn transform_states(t: &Transform, states: &mut [StateVector]) {
    #[cfg(feature = "simd")]
    {
        use lanes::{rotate, V4};
        let [qw, qx, qy, qz] = t.rotation.quaternion();
        let (q, w) = (V4::splat(Vec3::new(qx, qy, qz)), wide::f64x4::splat(qw));
        let (tr, tv) = (V4::splat(t.translation_m), V4::splat(t.origin_velocity_m_s));
        let omega = V4::splat(t.angular_velocity_rad_s);
        let mut chunks = states.chunks_exact_mut(4);
        for c in &mut chunks {
            let p = [
                c[0].position_m,
                c[1].position_m,
                c[2].position_m,
                c[3].position_m,
            ];
            let v = [
                c[0].velocity_m_s,
                c[1].velocity_m_s,
                c[2].velocity_m_s,
                c[3].velocity_m_s,
            ];
            let rp = rotate(q, w, V4::load(&p));
            let vb = rotate(q, w, V4::load(&v)).add(omega.cross(rp)).add(tv);
            let (mut po, mut vo) = ([Vec3::ZERO; 4], [Vec3::ZERO; 4]);
            rp.add(tr).store(&mut po);
            vb.store(&mut vo);
            for k in 0..4 {
                c[k].position_m = po[k];
                c[k].velocity_m_s = vo[k];
            }
        }
        for s in chunks.into_remainder() {
            (s.position_m, s.velocity_m_s) = t.apply(s.position_m, s.velocity_m_s);
        }
    }
    #[cfg(not(feature = "simd"))]
    for s in states {
        (s.position_m, s.velocity_m_s) = t.apply(s.position_m, s.velocity_m_s);
    }
}

impl StateVector {
    /// Moves every state into `destination`, in place, resolving one
    /// transform for each run of consecutive states that share a frame and
    /// an epoch (the common case: many objects at one instant).
    ///
    /// The results are identical to [`StateVector::to_frame`] on each
    /// state. If a transform cannot be resolved, the error is returned and
    /// the states before that run have already been moved.
    pub fn to_frame_batch<R: FrameResolver + ?Sized>(
        states: &mut [StateVector],
        destination: FrameId,
        resolver: &R,
    ) -> Result<(), R::Error> {
        let mut start = 0;
        while start < states.len() {
            let (frame, epoch) = (states[start].frame, states[start].epoch);
            let len = states[start..]
                .iter()
                .take_while(|s| s.frame == frame && s.epoch == epoch)
                .count();
            // Applied even when the frames match, as `to_frame` does, so the
            // results stay identical (adding zero turns −0.0 into +0.0).
            let t = resolver.transform(frame, destination, epoch)?;
            t.apply_batch(&mut states[start..start + len])?;
            start += len;
        }
        Ok(())
    }
}
