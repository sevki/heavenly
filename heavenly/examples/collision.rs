//! A conjunction assessment from start to finish:
//!
//! 1. an active satellite and a piece of debris, tracked today, each with a
//!    tracking covariance given in radial–transverse–normal axes;
//! 2. the close approach tomorrow, found with `catalog::conjunctions` on
//!    orbits integrated under the full force model;
//! 3. both covariances propagated to the time of closest approach;
//! 4. the probability of collision, and how it changes with the debris's
//!    tracking accuracy.
//!
//! The two objects are made up: the debris orbit is built backwards from a
//! chosen close approach.
//!
//! `cargo run --release -p heavenly --example collision`
use heavenly::orbit::forces::EARTH_RADIUS_M;
use heavenly::orbit::{
    gm, ConicElements, Drag, ForceModel, NumericalPropagator, RadiationPressure,
};
use heavenly::query::catalog::conjunctions;
use heavenly::query::collision_probability;
use heavenly::*;

fn main() -> Result<(), Error> {
    let now = Epoch::parse("2026-11-20T00:00:00Z")?;
    let tca_guess = now.add_seconds(8.0 * 3600.0 + 1234.0)?;

    // The satellite: 550 km, 53°.
    let sat_el = ConicElements {
        periapsis_m: EARTH_RADIUS_M + 550e3,
        eccentricity: 0.0001,
        inclination_rad: 53f64.to_radians(),
        ascending_node_rad: 40f64.to_radians(),
        argument_of_periapsis_rad: 0.0,
        mean_anomaly_rad: 0.0,
        epoch: now,
        gm_m3_s2: gm::EARTH,
    };
    let forces = |cd: f64, cr: f64| ForceModel {
        drag: Some(Drag::exponential(cd)),
        radiation: Some(RadiationPressure {
            cr_area_m2_per_kg: cr,
        }),
        ..ForceModel::earth()
    };
    let (r, v) = sat_el.state_at(now)?;
    let sat = NumericalPropagator::new(
        StateVector::new(FrameId::GCRF, now, r, v),
        forces(0.01, 0.01),
    )?;

    // The debris: at the satellite's position at `tca_guess`, 120 m above
    // it, crossing at 75° on a near-polar orbit. Integrate it back to today
    // to get its tracked state.
    let s = sat.propagate(tca_guess)?;
    let up = s.position_m / s.position_m.norm();
    let along = s.velocity_m_s / s.velocity_m_s.norm();
    let cross = up.cross(along);
    let angle = 75f64.to_radians();
    let speed = (gm::EARTH / (s.position_m.norm() + 120.0)).sqrt();
    let dv = (along * angle.cos() + cross * angle.sin()) * speed;
    let debris_at_tca = StateVector::new(FrameId::GCRF, tca_guess, s.position_m + up * 120.0, dv);
    let debris_forces = forces(0.05, 0.05); // lighter for its size
    let debris_now = NumericalPropagator::new(debris_at_tca, debris_forces)?.propagate(now)?;
    let debris = NumericalPropagator::new(debris_now, debris_forces)?;

    // 1. Tracking covariances today, in RTN. Diagonal for simplicity; real
    //    orbit-determination covariances correlate radial position with
    //    along-track velocity, which slows the along-track growth.
    let sat_rtn = Covariance::diagonal([10.0, 80.0, 10.0], [0.01, 0.005, 0.01])?;
    let debris_rtn = Covariance::diagonal([40.0, 300.0, 40.0], [0.05, 0.02, 0.04])?;
    let sat_cov = Covariance::from_rtn(&sat_rtn, &sat.initial())?;
    let debris_cov = Covariance::from_rtn(&debris_rtn, &debris.initial())?;

    // 2. Screen the next day for approaches under 5 km.
    let end = now.add_seconds(86_400.0)?;
    let tables = [sat.sample(now, end, 30.0)?, debris.sample(now, end, 30.0)?];
    let pos = |t: Epoch, k: usize| tables[k].state(t).map(|s| Some(s.position_m));
    let close = conjunctions::<Error>(
        now,
        end,
        60.0,
        1e-3,
        5e3,
        |t| (0..2).map(|k| pos(t, k)).collect(),
        pos,
    )?;
    println!("Approaches under 5 km in the next day: {}", close.len());
    let c = close
        .iter()
        .min_by(|x, y| x.miss_distance_m.total_cmp(&y.miss_distance_m))
        .ok_or(Error::InvalidInput("no close approach"))?;
    println!(
        "Closest at {} UTC, {:.1} h from now: {:.1} m",
        c.epoch.to_calendar(TimeScale::Utc)?,
        c.epoch.seconds_since(now) / 3600.0,
        c.miss_distance_m
    );

    // 3. Both states and covariances at the time of closest approach.
    let a = sat.propagate_uncertain(&sat_cov, c.epoch)?;
    let b = debris.propagate_uncertain(&debris_cov, c.epoch)?;
    for (name, today, u) in [("satellite", &sat_rtn, &a), ("debris", &debris_rtn, &b)] {
        let (s, t) = (u.covariance.to_rtn(&u.state)?.sigmas(), today.sigmas());
        println!(
            "  {name:9}: σ radial {:4.0} m, along-track {:5.0} m, cross-track {:3.0} m at the approach (today {:.0} / {:.0} / {:.0} m)",
            s[0], s[1], s[2], t[0], t[1], t[2]
        );
    }

    // 4. Probability of collision for a 10 m hard body (5 m each).
    let pc = collision_probability(&a, &b, 10.0)?;
    println!(
        "  relative speed {:.0} m/s; encounter-plane σ {:.0} × {:.0} m; miss = {:.2} σ",
        pc.relative_speed_m_s, pc.sigma_major_m, pc.sigma_minor_m, pc.miss_sigmas
    );
    println!(
        "  probability of collision: {:.2e}  ({} the common 1e-4 manoeuvre threshold)",
        pc.probability,
        if pc.probability > 1e-4 {
            "above"
        } else {
            "below"
        }
    );

    // A low probability is not always reassuring. With large uncertainties
    // the probability is "diluted": it rises as tracking improves, peaks
    // when the uncertainty is comparable to the miss, and only then falls,
    // as the miss becomes certain.
    println!("\nThe same encounter with both covariances scaled:");
    for k in [10.0, 3.0, 1.0, 0.3, 0.1, 0.03, 0.01] {
        let scaled = |u: &UncertainState| -> Result<UncertainState, Error> {
            Ok(UncertainState {
                covariance: u.covariance.transformed(&scale6(k))?,
                ..*u
            })
        };
        let p = collision_probability(&scaled(&a)?, &scaled(&b)?, 10.0)?;
        println!("  σ × {k:<5}: probability {:.2e}", p.probability);
    }
    Ok(())
}

/// `k·I`, which scales a covariance by `k²` (σ by `k`).
fn scale6(k: f64) -> Matrix6 {
    core::array::from_fn(|i| core::array::from_fn(|j| if i == j { k } else { 0.0 }))
}
