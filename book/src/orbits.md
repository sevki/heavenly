# Orbits and trajectories

Facilities sit still in their frames. Spacecraft do not. `heavenly-orbit`
provides `Locator`s for objects whose position comes from a trajectory, and
`heavenly-spice` adds one for bodies in an ephemeris.

```toml
heavenly-orbit = { git = "https://github.com/sevki/heavenly" }
```

| Locator | Crate | Source of the state |
|---------|-------|---------------------|
| `FixedLocator` | core | a fixed point in a frame |
| `TwoBodyLocator` | orbit | Keplerian elements, propagated analytically |
| `SampledTrajectory` | orbit | a table of states (tracking, OEM, a numerical propagator) |
| `SpkLocator` | spice | a body in an SPK kernel |

All of them answer the same question: *where is it, in frame X, at time T?*

```rust,ignore
{{#include ../../heavenly-orbit/examples/spacecraft.rs:spacecraft}}
```

## Two-body motion

`kepler::propagate` solves Kepler's equation in the universal anomaly χ, so
one algorithm covers every conic. It uses the Laguerre–Conway iteration,
which converges where plain Newton iteration can diverge on hyperbolic
orbits. Elliptic propagation is reduced modulo the period first, so long
spans keep their precision.

<figure class="hv-figure">
{{#heavenly-diagram conics}}
<figcaption>Every curve is computed by <code>kepler::propagate</code> from the same periapsis.</figcaption>
</figure>

## Orbital elements

`ConicElements` follows SPICE's `conics` / `oscelt` conventions: periapsis
distance `q` rather than semi-major axis, so circular, parabolic and
hyperbolic orbits need no special cases.

<figure class="hv-figure">
{{#heavenly-diagram orbital-elements}}
<figcaption>The six angles and distances of <code>ConicElements</code>; the orbit is computed by <code>ConicElements::state_at</code>.</figcaption>
</figure>

| Field | Symbol | Meaning |
|-------|--------|---------|
| `periapsis_m` | q | closest distance to the focus |
| `eccentricity` | e | 0 circle, < 1 ellipse, 1 parabola, > 1 hyperbola |
| `inclination_rad` | i | tilt of the orbit plane from the reference plane |
| `ascending_node_rad` | Ω | direction where the orbit rises through the plane |
| `argument_of_periapsis_rad` | ω | from the node to periapsis, in the orbit plane |
| `mean_anomaly_rad` | M₀ | position along the orbit at `epoch` |

For degenerate orbits, `from_state` follows `oscelt`. An equatorial orbit has
Ω = 0, with ω measured from the x axis. A circular orbit has ω = 0, with the
mean anomaly measured from the node.

Two-body motion ignores every perturbation. For a low Earth orbit, Earth's
oblateness alone moves the satellite by kilometres per day. Use
`TwoBodyLocator` for short arcs, for conic approximations, or as a reference.
For anything operational, use a `SampledTrajectory` filled by a real
propagator or by tracking data.

## Sampled trajectories and missing data

`SampledTrajectory` interpolates position and velocity with cubic Hermite
polynomials, as CCSDS OEM files do. Asking for a state it cannot honestly
give is an explicit error:

* `Error::OutsideCoverage` before the first sample or after the last;
* `Error::DataGap { before, after }` inside an interval longer than
  `max_gap_s`.

<figure class="hv-figure">
{{#heavenly-diagram sampled-trajectory}}
<figcaption>Top: where a trajectory answers and where it refuses. Bottom: measured interpolation error, zero at each sample and bounded by h⁴/384·rω⁴ between them.</figcaption>
</figure>

The error grows with the fourth power of the sample spacing:

| LEO sample spacing | Worst position error (measured) | Bound |
|--------------------|----------------------------------|-------|
| 10 s | 0.29 mm | 0.29 mm |
| 60 s | 0.38 m | 0.37 m |

## Time

Propagation intervals are in TDB, the time argument of SPICE and the DE
ephemerides. For geocentric orbits TT is the strictly correct scale. The two
differ by a periodic term of at most 1.7 ms, which is a few metres for a
low orbit propagated across half a year.

`Epoch::tdb_seconds_since(a)` forms that interval exactly. Subtracting two
`tdb_seconds_since_j2000()` values instead would lose about 0.1 µs to f64
rounding, which is about 1 mm at orbital speed. The test suite caught exactly
that.

## Accuracy

| Check | Reference | Worst relative error |
|-------|-----------|----------------------|
| `kepler::propagate`: LEO, HEO, near-parabolic (e = 0.999–1.001), hyperbolic, heliocentric; ±50 days or years | SPICE `prop2b` | 3.4e-12 |
| `ConicElements::state_at`, including e = 0 and e = 1 | SPICE `conics` | 1.3e-12 |
| `ConicElements::from_state` | SPICE `oscelt` | < 1e-9 in every element |
| `SampledTrajectory` vs the analytic orbit | h⁴/384·rω⁴ bound | within 1 % of the bound |
