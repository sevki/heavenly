//! Perturbed propagation: the analytic Moon and pole against DE440 and SOFA,
//! the zonal field and whole orbits against an independent implementation
//! (`tests/reference/generate_numerical.py`), and physical invariants.

#[rustfmt::skip]
#[path = "reference/numerical_data.rs"]
mod data;

use heavenly_core::{Epoch, Error, FrameId, StateVector, Vec3};
use heavenly_orbit::analytic::{earth_pole, moon_position_m};
use heavenly_orbit::forces::{exponential_density, EARTH_RADIUS_M, EARTH_ZONALS};
use heavenly_orbit::{
    gm, kepler, ConicElements, Drag, ForceModel, NumericalPropagator, RadiationPressure,
    SecularJ2Locator, ThirdBody, Zonal,
};

fn tt(s: f64) -> Epoch {
    Epoch::from_tt_seconds_since_j2000(s).unwrap()
}

fn v3(a: &[f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn angle(a: Vec3, b: Vec3) -> f64 {
    a.cross(b).norm().atan2(a.dot(b))
}

fn fixed_pole(_: Epoch) -> Vec3 {
    Vec3::Z
}

fn gcrf(epoch: Epoch, y: &[f64; 6]) -> StateVector {
    StateVector::new(
        FrameId::GCRF,
        epoch,
        Vec3::new(y[0], y[1], y[2]),
        Vec3::new(y[3], y[4], y[5]),
    )
}

fn elements(alt_km: f64, e: f64, i_deg: f64, epoch: Epoch) -> ConicElements {
    ConicElements {
        periapsis_m: EARTH_RADIUS_M + alt_km * 1e3,
        eccentricity: e,
        inclination_rad: i_deg.to_radians(),
        ascending_node_rad: 0.3,
        argument_of_periapsis_rad: 0.7,
        mean_anomaly_rad: 0.2,
        epoch,
        gm_m3_s2: gm::EARTH,
    }
}

fn start(el: &ConicElements, forces: ForceModel) -> NumericalPropagator {
    let (r, v) = el.state_at(el.epoch).unwrap();
    NumericalPropagator::new(StateVector::new(FrameId::GCRF, el.epoch, r, v), forces).unwrap()
}

#[test]
fn analytic_moon_against_de440() {
    let (mut worst_angle, mut worst_dist) = (0.0f64, 0.0f64);
    for (t, p) in data::MOON {
        let got = moon_position_m(tt(*t));
        let want = v3(p);
        worst_angle = worst_angle.max(angle(got, want).to_degrees());
        worst_dist = worst_dist.max((got.norm() - want.norm()).abs() / want.norm());
    }
    eprintln!(
        "analytic Moon vs DE440 1950-2050: worst direction {worst_angle:.3}°, distance {:.3} %",
        100.0 * worst_dist
    );
    assert!(worst_angle < 0.1, "{worst_angle}°");
    assert!(worst_dist < 1.5e-3, "{worst_dist}");
}

#[test]
fn earth_pole_against_sofa() {
    let mut worst = 0.0f64;
    for (t, p) in data::POLE {
        worst = worst.max(angle(earth_pole(tt(*t)), v3(p)));
    }
    let arcsec = worst.to_degrees() * 3600.0;
    eprintln!("analytic pole vs SOFA xy06 1950-2050: worst {arcsec:.3}″");
    assert!(arcsec < 0.2, "{arcsec}″");
}

#[test]
fn zonal_constants_and_field() {
    // From the normalised EGM2008 coefficients.
    let cbar: [f64; 5] = [
        -4.841_651_437_908_15e-4,
        9.571_612_070_934_73e-7,
        5.399_658_666_389_91e-7,
        6.867_029_137_366_81e-8,
        -1.499_539_279_785_27e-7,
    ];
    for (k, c) in cbar.iter().enumerate() {
        let n = (k + 2) as f64;
        let j = -c * (2.0 * n + 1.0).sqrt();
        assert!(
            (j - EARTH_ZONALS[k]).abs() <= 1e-15 * j.abs(),
            "J{}: {j} vs {}",
            k + 2,
            EARTH_ZONALS[k]
        );
    }
    let z = Zonal {
        pole: fixed_pole,
        ..Zonal::earth(6)
    };
    let mut worst = 0.0f64;
    for (r, a, u) in data::ZONAL {
        let got = z.acceleration(gm::EARTH, Epoch::J2000, v3(r));
        worst = worst.max((got - v3(a)).norm() / v3(a).norm());
        let pot = z.potential(gm::EARTH, Epoch::J2000, v3(r));
        assert!((pot - u).abs() <= 1e-13 * u.abs(), "{pot} vs {u}");
    }
    eprintln!("zonal J2-J6 acceleration: worst relative error {worst:.1e}");
    assert!(worst < 1e-13);
}

#[test]
fn two_body_matches_kepler() {
    let e0 = tt(8.36e8);
    for (alt, e, days) in [
        (500.0, 0.001, 1.0),
        (600.0, 0.74, 3.0),
        (35_786.0, 0.0002, 10.0),
    ] {
        let el = elements(alt, e, 30.0, e0);
        let orbit = start(&el, ForceModel::two_body(gm::EARTH));
        let (r0, v0) = el.state_at(e0).unwrap();
        let mut worst = 0.0f64;
        for k in 1..=10 {
            let dt = days * 86_400.0 * k as f64 / 10.0;
            let got = orbit.propagate(e0.add_seconds(dt).unwrap()).unwrap();
            let (r, _) = kepler::propagate(gm::EARTH, r0, v0, dt).unwrap();
            worst = worst.max((got.position_m - r).norm());
        }
        eprintln!("two-body, {alt} km e={e}, {days} d: {worst:.2e} m from Kepler");
        assert!(worst < 0.01, "{worst} m");
    }
}

#[test]
fn backwards_and_out_of_order() {
    let e0 = tt(8.36e8);
    let orbit = start(&elements(500.0, 0.01, 51.6, e0), ForceModel::earth());
    let later = e0.add_seconds(20_000.0).unwrap();
    let ahead = orbit.propagate(later).unwrap();
    // Back from there to the start recovers the initial state.
    let back = NumericalPropagator::new(ahead, ForceModel::earth())
        .unwrap()
        .propagate(e0)
        .unwrap();
    assert!((back.position_m - orbit.initial().position_m).norm() < 2e-3);
    // Epochs either side of the start, in any order, give the same states
    // as one at a time.
    let epochs = [3_000.0, -5_000.0, 9_000.0, 1_000.0, -200.0].map(|s| e0.add_seconds(s).unwrap());
    let mut got = Vec::new();
    orbit.for_each(epochs, |s| got.push(s)).unwrap();
    for (e, s) in epochs.iter().zip(&got) {
        let one = orbit.propagate(*e).unwrap();
        assert_eq!(s.epoch, *e);
        assert!((s.position_m - one.position_m).norm() < 1e-6);
    }
    // Sampled for interpolation, either side of the initial epoch.
    let table = orbit
        .sample(
            e0.add_seconds(-3_000.0).unwrap(),
            e0.add_seconds(6_000.0).unwrap(),
            30.0,
        )
        .unwrap();
    let mid = e0.add_seconds(1_234.5).unwrap();
    let interp = table.state(mid).unwrap();
    assert!((interp.position_m - orbit.propagate(mid).unwrap().position_m).norm() < 0.05);
}

#[test]
fn j2_conserves_energy_and_polar_angular_momentum() {
    // With a fixed axis, the J2 field is static and axially symmetric:
    // energy and the axial angular momentum are constants of the motion.
    let e0 = tt(8.36e8);
    let z = Zonal {
        pole: fixed_pole,
        ..Zonal::earth(6)
    };
    let forces = ForceModel {
        zonal: Some(z),
        ..ForceModel::two_body(gm::EARTH)
    };
    let orbit = start(&elements(400.0, 0.05, 63.0, e0), forces);
    let energy = |s: &StateVector| {
        0.5 * s.velocity_m_s.norm_squared()
            - gm::EARTH / s.position_m.norm()
            - z.potential(gm::EARTH, s.epoch, s.position_m)
    };
    let hz = |s: &StateVector| s.position_m.cross(s.velocity_m_s).z;
    let s0 = orbit.initial();
    let (mut de, mut dh) = (0.0f64, 0.0f64);
    orbit
        .for_each(
            (1..=40).map(|k| e0.add_seconds(k as f64 * 21_600.0).unwrap()),
            |s| {
                de = de.max(((energy(&s) - energy(&s0)) / energy(&s0)).abs());
                dh = dh.max(((hz(&s) - hz(&s0)) / hz(&s0)).abs());
            },
        )
        .unwrap();
    eprintln!("J2-J6, 10 days: energy drift {de:.1e}, axial angular momentum drift {dh:.1e}");
    assert!(de < 1e-10 && dh < 1e-10, "{de} {dh}");
}

#[test]
fn secular_rates_match_numerical_j2() {
    // Over ten days the node and periapsis of a J2-only orbit turn at the
    // secular rates; the short-period terms average out.
    let e0 = tt(8.36e8);
    let z = Zonal {
        pole: fixed_pole,
        ..Zonal::earth(2)
    };
    let forces = ForceModel {
        zonal: Some(z),
        ..ForceModel::two_body(gm::EARTH)
    };
    for (alt, e, inc) in [
        (700.0, 0.001, 98.2),
        (500.0, 0.02, 51.6),
        (800.0, 0.05, 30.0),
    ] {
        let el = elements(alt, e, inc, e0);
        let secular = SecularJ2Locator::earth(el).unwrap();
        let (node_rate, _, _) = secular.rates();
        let days = 10.0;
        let end = e0.add_seconds(days * 86_400.0).unwrap();
        let s = start(&el, forces).propagate(end).unwrap();
        let osc = ConicElements::from_state(s.position_m, s.velocity_m_s, end, gm::EARTH).unwrap();
        let mut moved = osc.ascending_node_rad - el.ascending_node_rad;
        moved =
            (moved + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        let expected = node_rate * days * 86_400.0;
        let analytic = secular.elements_at(end).ascending_node_rad - el.ascending_node_rad;
        eprintln!(
            "{alt} km, {inc}°: node moved {:.4}° in 10 days, secular rate gives {:.4}°",
            moved.to_degrees(),
            expected.to_degrees()
        );
        assert!(
            (moved - expected).abs() < 0.01 * expected.abs() + 2e-4,
            "{moved} vs {expected}"
        );
        assert!(
            ((analytic - expected + 1.0).rem_euclid(std::f64::consts::TAU) - 1.0).abs() < 1e-12
        );
    }
}

#[test]
fn sun_synchronous_secular_orbit_keeps_facing_the_sun() {
    let e0 = tt(8.36e8);
    let a = EARTH_RADIUS_M + 700e3;
    let i = heavenly_orbit::design::sun_synchronous_inclination_rad(a, 0.0).unwrap();
    let el = ConicElements {
        inclination_rad: i,
        eccentricity: 0.0,
        periapsis_m: a,
        ..elements(700.0, 0.0, 0.0, e0)
    };
    let (rate, _, _) = SecularJ2Locator::earth(el).unwrap().rates();
    let year = 365.242_19 * 86_400.0;
    assert!((rate * year / std::f64::consts::TAU - 1.0).abs() < 1e-9);
}

#[test]
fn drag_lowers_an_orbit_until_it_reenters() {
    // Drag alone on an equatorial orbit, so the osculating semi-major axis is
    // not blurred by J2 and the height above the ellipsoid is the altitude.
    let e0 = tt(8.36e8);
    let forces = ForceModel {
        drag: Some(Drag::exponential(0.02)),
        ..ForceModel::two_body(gm::EARTH)
    };
    let orbit = start(&elements(250.0, 0.0, 0.0, e0), forces);
    let a = |s: &StateVector| {
        let el =
            ConicElements::from_state(s.position_m, s.velocity_m_s, s.epoch, gm::EARTH).unwrap();
        el.periapsis_m / (1.0 - el.eccentricity)
    };
    let hour = orbit.propagate(e0.add_seconds(3_600.0).unwrap()).unwrap();
    let lost_m = a(&orbit.initial()) - a(&hour);
    // Circular decay: da/dt = −ρ (Cd A/m) √(μ a), with the air moving at the
    // Earth's rotation, which slows the relative wind by ωr/v (6 % here)
    // and so the drag by about twice that.
    let r = EARTH_RADIUS_M + 250e3;
    let v = (gm::EARTH / r).sqrt();
    let wind = 1.0 - 7.292_115e-5 * r / v;
    let expected =
        exponential_density(250e3) * 0.02 * (gm::EARTH * r).sqrt() * wind * wind * 3_600.0;
    eprintln!("250 km, Cd·A/m 0.02: semi-major axis fell {lost_m:.1} m in an hour (analytic {expected:.1} m)");
    assert!(
        (lost_m / expected - 1.0).abs() < 0.02,
        "{lost_m} vs {expected}"
    );
    // Weeks later it has come down: an explicit error, not a state underground.
    let err = orbit
        .propagate(e0.add_seconds(90.0 * 86_400.0).unwrap())
        .unwrap_err();
    assert_eq!(err, Error::InvalidInput("the orbit reached the surface"));
}

#[test]
fn needs_gcrf_and_finite_state() {
    let s = StateVector::new(
        FrameId::ITRF,
        Epoch::J2000,
        Vec3::new(7e6, 0.0, 0.0),
        Vec3::new(0.0, 7.5e3, 0.0),
    );
    assert!(matches!(
        NumericalPropagator::new(s, ForceModel::earth()),
        Err(Error::FrameMismatch { .. })
    ));
    let s = StateVector {
        frame: FrameId::GCRF,
        position_m: Vec3::new(f64::NAN, 0.0, 0.0),
        ..s
    };
    assert_eq!(
        NumericalPropagator::new(s, ForceModel::earth()).unwrap_err(),
        Error::NonFinite
    );
}

#[test]
fn orbits_match_the_independent_reference() {
    let mut failures = Vec::new();
    for (name, tt0, flags, y0, outputs, rk4_bound) in data::ORBITS {
        let [degree, true_pole, sun_moon, cd, cr] = *flags;
        let zonal = Zonal {
            pole: if true_pole == 1.0 {
                earth_pole
            } else {
                fixed_pole
            },
            ..Zonal::earth(degree as u8)
        };
        let forces = ForceModel {
            zonal: Some(zonal),
            sun: (sun_moon == 1.0).then_some(ThirdBody::SUN),
            moon: (sun_moon == 1.0).then_some(ThirdBody::MOON),
            drag: (cd > 0.0).then(|| Drag::exponential(cd)),
            radiation: (cr > 0.0).then_some(RadiationPressure {
                cr_area_m2_per_kg: cr,
            }),
            ..ForceModel::two_body(gm::EARTH)
        };
        let e0 = tt(*tt0);
        let orbit = NumericalPropagator::new(gcrf(e0, y0), forces).unwrap();
        let mut worst = 0.0f64;
        orbit
            .for_each(
                outputs.iter().map(|(t, _)| e0.add_seconds(*t).unwrap()),
                |s| {
                    let (_, y) = outputs
                        .iter()
                        .find(|(t, _)| e0.add_seconds(*t).unwrap() == s.epoch)
                        .unwrap();
                    worst = worst.max((s.position_m - Vec3::new(y[0], y[1], y[2])).norm());
                },
            )
            .unwrap();
        let span_days = outputs.last().unwrap().0 / 86_400.0;
        eprintln!("numerical {name}: worst {worst:.3} m over {span_days} d (reference RK4 h vs h/2: {rk4_bound:.1e} m)");
        // With the same pole and no Sun or Moon, the two implementations
        // agree to the integrators' accuracy. With the analytic Sun, Moon
        // and pole against DE440 and SOFA, the difference is what those
        // series cost.
        let limit = match *name {
            "leo_zonal_fixed_pole" => 0.01,
            "leo_full" => 2.0,
            "sso_full" => 10.0,
            "molniya" => 100.0,
            "geo" => 1_000.0,
            _ => unreachable!(),
        };
        if worst > limit {
            failures.push(format!("{name}: {worst} m > {limit} m"));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
