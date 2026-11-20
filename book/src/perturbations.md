# Perturbations

A two-body orbit is a fixed ellipse. A real Earth satellite's orbit is not.

- **The Earth's oblateness (J2)** turns the orbit's plane and its ellipse.
  A low orbit's node moves several degrees a day, and the satellite drifts
  by hundreds of kilometres from the fixed ellipse within a day.
- **Air drag** lowers a low orbit until it re-enters.
- **The Sun and the Moon** tilt a geostationary orbit by nearly a degree a
  year.
- **Sunlight** pushes on large, light satellites.

`heavenly::orbit` has three models, from cheap to complete:

| Model | Cost | Has | Use it for |
|-------|------|-----|------------|
| [`TwoBodyLocator`] | one Kepler solve | nothing but the central body | hours, or a first look |
| [`SecularJ2Locator`] | one Kepler solve | J2's steady turning of the node, periapsis and mean anomaly | days of a low orbit's ground track, sun-synchronous and frozen orbits |
| [`NumericalPropagator`] | an integration | J2–J6, drag, solar radiation pressure, the Sun and the Moon | anything that must be right to metres |

`Sgp4` sits beside these. It propagates TLEs with its own analytic
perturbation model, and is the right choice when the input is a TLE (see
[TLEs and SGP4](sgp4.md)).

## J2 secular motion

J2 makes the node turn at `Ω̇ = −(3/2) n J2 (R/p)² cos i`. It is westward
for prograde orbits, about −5° a day at the ISS's 51.6°. At 98° the node
turns once a year, eastward, and the plane keeps its angle to the Sun: a
*sun-synchronous* orbit (`design::sun_synchronous_inclination_rad`). The
periapsis turns at `ω̇ = (3/4) n J2 (R/p)² (5 cos² i − 1)`. This rate is
zero at 63.4°, which is why Molniya orbits use that inclination.

```rust,ignore
# use heavenly_core::*;
# use heavenly_orbit::{gm, ConicElements, SecularJ2Locator};
# fn main() -> Result<(), Error> {
let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
let el = ConicElements {
    periapsis_m: 6_378_137.0 + 420e3,
    eccentricity: 0.0005,
    inclination_rad: 51.6f64.to_radians(),
    ascending_node_rad: 0.0,
    argument_of_periapsis_rad: 0.0,
    mean_anomaly_rad: 0.0,
    epoch,
    gm_m3_s2: gm::EARTH,
};
// Elements from a state vector are osculating: convert the semi-major axis
// to its mean value first.
let iss = SecularJ2Locator::earth_from_osculating(el)?;
let (node_rate, _, _) = iss.rates();
assert!((node_rate.to_degrees() * 86_400.0 + 5.0).abs() < 0.1); // −5°/day
# Ok(()) }
```

The secular model works with *mean* elements. Elements computed from a
state vector are *osculating*: they include J2's short-period wobble. For a
low orbit that wobble is about 9 km in semi-major axis, and it would shift
the mean motion enough to put the satellite 1 300 km along its orbit after
a day. `earth_from_osculating` removes the first-order term (Kozai) from
the semi-major axis. In the tests the secular orbit then stays within 6 km
of the full numerical one over a day.

## Numerical propagation

[`NumericalPropagator`] integrates `r̈ = a(t, r, ṙ)` in GCRF with an adaptive
Dormand–Prince 5(4) integrator. Each requested epoch is hit exactly, with
no interpolation. The acceleration comes from a [`ForceModel`]:

- **Gravity**: the central body, plus [`Zonal`] harmonics to J6 (EGM2008).
  They are aligned with the Earth's actual rotation axis
  (`analytic::earth_pole`: IAU 2006 precession and the main nutation
  terms). The GCRF z axis is 0.35° away from that axis in 2026.
- **[`Drag`]**: `−½ ρ (Cd A/m) |v_rel| v_rel`. The atmosphere turns with the
  Earth, and the density comes from Vallado's exponential model.
- **[`RadiationPressure`]**: a cannonball, switched off in the Earth's
  cylindrical shadow.
- **[`ThirdBody`]**: the Sun and the Moon, by default from the
  [analytic series](#accuracy). Pass your own positions, for example
  sampled from DE440, for more accuracy.

```rust,ignore
# use heavenly_core::*;
# use heavenly_orbit::{gm, ConicElements, Drag, ForceModel, NumericalPropagator, RadiationPressure};
# fn main() -> Result<(), Error> {
# let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
# let el = ConicElements { periapsis_m: 6_778_137.0, eccentricity: 0.0005,
#     inclination_rad: 0.9, ascending_node_rad: 0.0, argument_of_periapsis_rad: 0.0,
#     mean_anomaly_rad: 0.0, epoch, gm_m3_s2: gm::EARTH };
let (r, v) = el.state_at(epoch)?;
let forces = ForceModel {
    drag: Some(Drag::exponential(0.005)), // Cd·A/m, m²/kg: ISS-like
    radiation: Some(RadiationPressure { cr_area_m2_per_kg: 0.006 }),
    ..ForceModel::earth() // J2–J6, Sun, Moon
};
let orbit = NumericalPropagator::new(StateVector::new(FrameId::GCRF, epoch, r, v), forces)?;
let tomorrow = orbit.propagate(epoch.add_seconds(86_400.0)?)?;
// Or tabulate it once and interpolate (cubic Hermite, centimetres at 30 s).
let table = orbit.sample(epoch, epoch.add_seconds(86_400.0)?, 30.0)?;
# let _ = (tomorrow, table);
# Ok(()) }
```

`NumericalPropagator` is a `Locator`, but each call integrates from the
initial state. To look up many epochs, use `for_each` (one pass through
them) or `sample`.

When the satellite comes down, propagation fails with
`Error::InvalidInput("the orbit reached the surface")`. It never returns a
state underground.

### Cost and tolerance

The default tolerance (1e-13 relative) keeps the integration error to about
1 mm per day in a low orbit. Each factor of ten looser makes the error ten
times larger and the integration about 1.6 times faster. Measured on one
core, for one day of a low orbit:

| Tolerance | Error after 1 day | Two-body | Full force model |
|-----------|-------------------|----------|------------------|
| 1e-11 | 12 cm | 1.5 ms | 18 ms |
| 1e-12 | 1.2 cm | 2.3 ms | 29 ms |
| 1e-13 (default) | 1.2 mm | 3.9 ms | 48 ms |
| 1e-14 | 0.15 mm | 7.1 ms | 101 ms |

Run `cargo run --release -p heavenly-orbit --example tolerance` to measure
it on your machine. The full model spends most of its time in the Sun, Moon
and pole series, which are evaluated at every step.

## Accuracy

The tests measure each part against an independent reference
(`heavenly-orbit/tests/numerical.rs`; the fixtures come from
`tests/reference/generate_numerical.py`, which uses SPICE, SOFA and NumPy):

| What | Against | Worst difference |
|------|---------|------------------|
| Two-body integration, 1–10 days | Kepler's solution | 1.4–3.6 mm |
| Zonal field J2–J6 | explicit Legendre polynomials | 5e-16 relative |
| Same forces, 1 day, low orbit | independent RK4 implementation | 2 mm |
| Energy and axial angular momentum, J2–J6, 10 days | conserved | 3e-11 relative |
| Drag decay, 1 hour at 250 km | analytic `ȧ = −ρ B √(μa)` | 0.3 % |
| J2 node rate, 10 days | numerical J2 | 0.2 % |
| Analytic Moon, 1950–2050 | DE440 | 0.08°, 0.13 % in distance |
| Analytic pole, 1950–2050 | SOFA IAU 2006/2000A | 0.17″ |
| Full model with the analytic Sun, Moon and pole | DE440 and SOFA in the same forces | 0.35 m (LEO, 1 d), 2 m (SSO, 3 d), 25 m (Molniya, 7 d), 300 m (GEO, 30 d) |

The last row is the price of needing no kernels. Pass DE440 positions to
`ThirdBody` and an IERS pole to `Zonal` to remove it.

### What it does not model

- **Tesseral harmonics** (J22 and beyond). These make geostationary
  satellites drift in longitude towards 75° E or 105° W, and they move low
  orbits by tens of metres a day.
- **Space weather.** The exponential atmosphere is static and has
  mean-solar-activity density. Real density at 400 km varies tenfold with
  the Sun's activity and from day to night, so decay times are typical
  values, not predictions.
- **Attitude.** Drag and radiation pressure act on a cannonball.
- Tides, Earth albedo, the planets, and relativity. Each is below a metre a
  day for Earth satellites.

[`TwoBodyLocator`]: https://sevki.github.io/heavenly/api/heavenly_orbit/struct.TwoBodyLocator.html
[`SecularJ2Locator`]: https://sevki.github.io/heavenly/api/heavenly_orbit/struct.SecularJ2Locator.html
[`NumericalPropagator`]: https://sevki.github.io/heavenly/api/heavenly_orbit/struct.NumericalPropagator.html
[`ForceModel`]: https://sevki.github.io/heavenly/api/heavenly_orbit/forces/struct.ForceModel.html
[`Zonal`]: https://sevki.github.io/heavenly/api/heavenly_orbit/forces/struct.Zonal.html
[`Drag`]: https://sevki.github.io/heavenly/api/heavenly_orbit/forces/struct.Drag.html
[`RadiationPressure`]: https://sevki.github.io/heavenly/api/heavenly_orbit/forces/struct.RadiationPressure.html
[`ThirdBody`]: https://sevki.github.io/heavenly/api/heavenly_orbit/forces/struct.ThirdBody.html
