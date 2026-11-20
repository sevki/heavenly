use heavenly_core::*;

/// Deterministic pseudo-random numbers in [-1, 1).
struct Lcg(u64);
impl Lcg {
    fn f(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
    fn v(&mut self, scale: f64) -> Vec3 {
        Vec3::new(self.f(), self.f(), self.f()) * scale
    }
    fn rot(&mut self) -> Rotation {
        Rotation::from_axis_angle(self.v(1.0), self.f() * 3.0)
    }
    fn transform(&mut self) -> Transform {
        Transform::new(self.rot(), self.v(7e6), self.v(8e3), self.v(1e-3))
    }
}

fn close(a: Vec3, b: Vec3, tol: f64) -> bool {
    (a - b).norm() <= tol * (1.0 + a.norm().max(b.norm()))
}

#[test]
fn rotation_basics() {
    let r = Rotation::from_axis_angle(Vec3::Z, core::f64::consts::FRAC_PI_2);
    assert!(close(r.apply(Vec3::X), Vec3::Y, 1e-15));
    assert!(close(r.inverse().apply(Vec3::Y), Vec3::X, 1e-15));
    assert!((r.angle_rad() - core::f64::consts::FRAC_PI_2).abs() < 1e-15);
    let m = r.to_matrix();
    assert!((m[1][0] - 1.0).abs() < 1e-15 && (m[0][1] + 1.0).abs() < 1e-15);
}

#[test]
fn inverse_round_trip() {
    let mut g = Lcg(1);
    for _ in 0..200 {
        let t = g.transform();
        let (p, v) = (g.v(1e7), g.v(1e4));
        let (pb, vb) = t.apply(p, v);
        let (pa, va) = t.inverse().apply(pb, vb);
        assert!(close(p, pa, 1e-14), "{p:?} {pa:?}");
        assert!(close(v, va, 1e-12), "{v:?} {va:?}");

        let id = t.then(&t.inverse());
        let (pi, vi) = id.apply(p, v);
        assert!(close(p, pi, 1e-14) && close(v, vi, 1e-12));
    }
}

#[test]
fn composition_matches_sequential_application() {
    let mut g = Lcg(2);
    for _ in 0..200 {
        let (t1, t2, t3) = (g.transform(), g.transform(), g.transform());
        let (p, v) = (g.v(1e7), g.v(1e4));
        let (p1, v1) = t1.apply(p, v);
        let (p2, v2) = t2.apply(p1, v1);
        let (p3, v3) = t3.apply(p2, v2);
        // Associativity: (t1·t2)·t3 and t1·(t2·t3).
        for c in [t1.then(&t2).then(&t3), t1.then(&t2.then(&t3))] {
            let (pc, vc) = c.apply(p, v);
            assert!(close(p3, pc, 1e-13), "{p3:?} {pc:?}");
            assert!(close(v3, vc, 1e-12), "{v3:?} {vc:?}");
        }
    }
}

/// A frame whose orientation spins at `w` about `axis` from `r0`, and whose
/// origin follows `t0 + u·τ + ½a·τ²`. Exact kinematics, for finite differences.
#[derive(Clone, Copy)]
struct Motion {
    r0: Rotation,
    axis: Vec3,
    w: f64,
    t0: Vec3,
    u: Vec3,
    a: Vec3,
}

impl Motion {
    fn random(g: &mut Lcg) -> Motion {
        Motion {
            r0: g.rot(),
            axis: g.v(1.0).normalized().unwrap(),
            w: g.f() * 1e-3,
            t0: g.v(4e7),
            u: g.v(3e3),
            a: g.v(1.0),
        }
    }
    fn at(&self, tau: f64) -> Transform {
        Transform::new(
            Rotation::from_axis_angle(self.axis, self.w * tau).compose(self.r0),
            self.t0 + self.u * tau + self.a * (0.5 * tau * tau),
            self.u + self.a * tau,
            self.axis * self.w,
        )
    }
}

/// Central-difference check that `v_B` equals d/dt `p_B` for a moving point.
fn check_velocity(t_at: impl Fn(f64) -> Transform, p0: Vec3, v0: Vec3) {
    let tau = 1234.5;
    let h = 1e-2;
    let pos = |s: f64| t_at(s).apply_position(p0 + v0 * s);
    let numeric = (pos(tau + h) - pos(tau - h)) / (2.0 * h);
    let (_, analytic) = t_at(tau).apply(p0 + v0 * tau, v0);
    assert!(
        (numeric - analytic).norm() < 1e-5 * (1.0 + analytic.norm()),
        "numeric {numeric:?} analytic {analytic:?}"
    );
}

#[test]
fn velocity_transformation_matches_finite_differences() {
    let mut g = Lcg(3);
    for _ in 0..50 {
        let (m1, m2, m3) = (
            Motion::random(&mut g),
            Motion::random(&mut g),
            Motion::random(&mut g),
        );
        let (p0, v0) = (g.v(1e6), g.v(1e2));
        check_velocity(|s| m1.at(s), p0, v0);
        check_velocity(|s| m1.at(s).inverse(), p0, v0);
        check_velocity(|s| m1.at(s).then(&m2.at(s)), p0, v0);
        check_velocity(
            |s| m1.at(s).then(&m2.at(s).inverse()).then(&m3.at(s)),
            p0,
            v0,
        );
        check_velocity(|s| m1.at(s).then(&m2.at(s)).inverse(), p0, v0);
    }
}

fn custom(code: i64) -> FrameId {
    FrameId::custom(Namespace(7), code).unwrap()
}

fn tau(e: Epoch) -> f64 {
    e.seconds_since(Epoch::J2000)
}

#[test]
fn graph_resolves_multi_hop_paths_in_both_directions() {
    let mut g = Lcg(4);
    let (m_earth, m_site, m_sat, m_module) = (
        Motion::random(&mut g),
        Motion::random(&mut g),
        Motion::random(&mut g),
        Motion::random(&mut g),
    );
    let site = custom(1);
    let sat = custom(2);
    let module = custom(3);

    // Tree:     GCRF
    //          /    \
    //       ITRF    sat
    //        |       |
    //       site   module
    let mut frames = FrameGraph::new();
    frames
        .add_edge(
            FrameId::ITRF,
            FrameId::GCRF,
            move |e| Ok(m_earth.at(tau(e))),
        )
        .unwrap();
    frames
        .add_edge(site, FrameId::ITRF, move |e| Ok(m_site.at(tau(e))))
        .unwrap();
    frames
        .add_edge(sat, FrameId::GCRF, move |e| Ok(m_sat.at(tau(e))))
        .unwrap();
    frames
        .add_edge(module, sat, move |e| Ok(m_module.at(tau(e))))
        .unwrap();

    let expected = |s: f64| {
        m_site
            .at(s)
            .then(&m_earth.at(s))
            .then(&m_sat.at(s).inverse())
            .then(&m_module.at(s).inverse())
    };
    let resolved = |s: f64| {
        let e = Epoch::J2000.add_seconds(s).unwrap();
        let ft = frames.transform(site, module, e).unwrap();
        assert_eq!((ft.source, ft.destination, ft.epoch), (site, module, e));
        ft.transform
    };
    let (p, v) = (g.v(100.0), g.v(1.0));
    let (pe, ve) = expected(10.0).apply(p, v);
    let (pr, vr) = resolved(10.0).apply(p, v);
    assert!(close(pe, pr, 1e-14) && close(ve, vr, 1e-12));
    check_velocity(resolved, p, v);

    // Reverse direction is the inverse.
    let back = frames
        .transform(module, site, Epoch::J2000)
        .unwrap()
        .transform;
    let (p1, v1) = resolved(0.0).apply(p, v);
    let (p2, v2) = back.apply(p1, v1);
    // Round-tripping through ~1e8 m translations loses ~1e-8 m to rounding.
    assert!(
        (p - p2).norm() < 1e-6 && (v - v2).norm() < 1e-9,
        "{:?} {:?}",
        p - p2,
        v - v2
    );

    // Identity.
    let id = frames.transform(sat, sat, Epoch::J2000).unwrap();
    assert_eq!(id.transform, Transform::IDENTITY);
}

#[test]
fn graph_errors_are_explicit() {
    let mut frames = FrameGraph::new();
    let a = custom(10);
    let b = custom(11);
    let lonely = custom(12);
    let start = Epoch::parse("2026-01-01T00:00:00 TDB").unwrap();
    frames
        .add_edge(a, b, move |e| {
            if e < start {
                Err(Error::OutOfRange {
                    source: a,
                    destination: b,
                    epoch: e,
                })
            } else {
                Ok(Transform::IDENTITY)
            }
        })
        .unwrap();

    assert_eq!(
        frames.add_fixed(b, a, Transform::IDENTITY),
        Err(Error::DuplicateEdge { a: b, b: a })
    );
    assert_eq!(
        frames.transform(a, lonely, start),
        Err(Error::NoPath {
            source: a,
            destination: lonely
        })
    );
    assert!(matches!(
        frames.transform(b, a, Epoch::J2000),
        Err(Error::OutOfRange { .. })
    ));
    assert!(frames.transform(b, a, start).is_ok());
}

#[test]
fn frame_transform_checks_labels() {
    let e = Epoch::J2000;
    let ab = FrameTransform {
        source: custom(1),
        destination: custom(2),
        epoch: e,
        transform: Transform::IDENTITY,
    };
    let cd = FrameTransform {
        source: custom(3),
        destination: custom(4),
        epoch: e,
        transform: Transform::IDENTITY,
    };
    assert!(matches!(ab.then(&cd), Err(Error::FrameMismatch { .. })));
    assert!(ab.then(&ab.inverse()).is_ok());

    let s = StateVector::at_rest(custom(2), e, Vec3::ZERO);
    assert!(matches!(ab.apply(&s), Err(Error::FrameMismatch { .. })));
    let s = StateVector::at_rest(custom(1), e.add_nanoseconds(1), Vec3::ZERO);
    assert!(matches!(ab.apply(&s), Err(Error::EpochMismatch { .. })));
}

#[test]
fn typed_coordinates_and_locators() {
    // Earth-fixed frame spinning about GCRF z at roughly Earth's rate.
    const OMEGA: f64 = 7.292_115e-5;
    let mut frames = FrameGraph::new();
    frames
        .add_edge(FrameId::ITRF, FrameId::GCRF, |e| {
            let theta = OMEGA * tau(e);
            Ok(Transform::new(
                Rotation::from_axis_angle(Vec3::Z, theta),
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::Z * OMEGA,
            ))
        })
        .unwrap();

    let station = LocationRecord {
        id: LocationId(0x42),
        locator: FixedLocator {
            frame: FrameId::ITRF,
            position_m: Vec3::new(6.378e6, 0.0, 0.0),
        },
    };
    let epoch = Epoch::J2000;
    let s = station
        .locator
        .state_at(epoch, FrameId::GCRF, &frames)
        .unwrap();
    // At rest on the equator: inertial speed ω·R, directed +y.
    assert!(close(
        s.velocity_m_s,
        Vec3::new(0.0, OMEGA * 6.378e6, 0.0),
        1e-12
    ));

    let fixed = Coordinate::<EarthFixed>::new(epoch, Vec3::new(6.378e6, 0.0, 0.0), Vec3::ZERO);
    let inertial: Coordinate<Gcrf> = fixed.to_frame(&frames).unwrap();
    assert_eq!(inertial.to_state(), s);
    assert_eq!(Coordinate::<Gcrf>::from_state(&s).unwrap(), inertial);
    assert!(Coordinate::<EarthFixed>::from_state(&s).is_err());

    let other = Coordinate::<Gcrf>::new(epoch, Vec3::new(6.378e6, 3.0, 4.0), s.velocity_m_s);
    assert!((inertial.separation_m(&other).unwrap() - 5.0).abs() < 1e-9);
    assert_eq!(inertial.relative_speed_m_s(&other).unwrap(), 0.0);
    let later = Coordinate::<Gcrf>::new(epoch.add_nanoseconds(1), Vec3::ZERO, Vec3::ZERO);
    assert!(matches!(
        inertial.separation_m(&later),
        Err(Error::EpochMismatch { .. })
    ));
}

#[test]
fn frame_ids() {
    assert_eq!(FrameId::GCRF.to_string(), "GCRF");
    assert_eq!(custom(5).to_string(), "7:5");
    assert!(FrameId::custom(Namespace::HEAVENLY, 99).is_none());
    assert_eq!(
        LocationId(0xabc).to_string(),
        "00000000000000000000000000000abc"
    );
}

#[test]
fn rotation_from_matrix_round_trips() {
    let mut g = Lcg(9);
    for i in 0..500 {
        // Include angles near π, where the trace pivot is not used.
        let angle = if i % 2 == 0 {
            g.f() * 3.0
        } else {
            core::f64::consts::PI - g.f().abs() * 1e-3
        };
        let r = Rotation::from_axis_angle(g.v(1.0), angle);
        let back = Rotation::from_matrix(r.to_matrix()).unwrap();
        let v = g.v(1.0);
        assert!(close(r.apply(v), back.apply(v), 1e-14));
    }
    assert!(Rotation::from_matrix([[f64::NAN; 3]; 3]).is_none());
}
