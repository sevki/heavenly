# Covariance and collision probability

A tracked object's state is an estimate. Its *covariance* says how far off
it might be, and in which directions. Usually it is least certain along the
orbit, because a small error in speed becomes a growing error in where the
object is. Two objects can pass a few hundred metres apart. Whether that is
dangerous depends on those uncertainties, which is what the *probability of
collision* measures.

## Covariance on a state

[`Covariance`] is a 6×6 position and velocity covariance (m², m²/s, m²/s²).
It is checked to be symmetric and positive semi-definite.
[`UncertainState`] pairs one with a `StateVector`, in the state's frame and
at its epoch.

```rust,ignore
use heavenly::*;

// 100 m radial, 1 km along track, 50 m cross track; velocities in m/s.
let rtn = Covariance::diagonal([100.0, 1_000.0, 50.0], [0.1, 0.01, 0.05])?;
let c = Covariance::from_rtn(&rtn, &state)?;       // into the state's frame
let tracked = UncertainState::new(state, c);
let in_itrf = tracked.to_frame(FrameId::ITRF, &frames)?;
```

- **RTN.** Conjunction data messages give covariances in the
  radial–transverse–normal axes of the orbit. `from_rtn` and `to_rtn`
  convert.
- **Frames.** `to_frame` moves the covariance with the state, through the
  transform's exact Jacobian `[[R, 0], [[ω]× R, R]]`. In a rotating frame
  such as ITRF, a position error therefore also becomes a velocity error,
  `ω × δr`. The tests compare this against a finite-difference Jacobian of
  the GCRF → ITRF state transform.

## Through time

[`uncertainty::propagate_uncertain`] carries a covariance along an orbit as
`Φ C Φᵀ`. Here `Φ` is the state transition matrix, computed by central
differences with Richardson extrapolation (24 propagations). The
propagation is any function, so this works for two-body, J2 secular, SGP4
or numerical orbits. [`NumericalPropagator::propagate_uncertain`] wraps it
with the propagator's own forces.

The tests check it four ways:

| Check | Result |
|-------|--------|
| `Φ` of a Hamiltonian flow is symplectic, `Φᵀ J Φ = J`: two-body over a day | 2e-7 |
| …Molniya through perigee | 7e-7 |
| …numerical J2–J6 about a fixed axis, 12 h | 2e-6 |
| An along-track velocity error after a day, against Clohessy–Wiltshire `δy = (4/n) sin(nt) δv − 3 δv t` | 2 556.5 m both |
| Linear covariance after one orbit, against 20 000 propagated samples | 0.5 % in variance |

Linear propagation holds while the uncertainty is small compared with the
orbit's curvature. Over long spans, or with very large uncertainties, the
real distribution curves along the orbit into a "banana" that no Gaussian
describes.

## Probability of collision

[`collision_probability`] uses the standard short-encounter model (Foster
and Estes; Chan; Alfano):

- the two objects move in straight lines through the encounter;
- their position errors are independent and Gaussian, so the covariances
  add;
- a collision is the miss falling within the *hard-body radius*, the sum of
  the two objects' radii.

The combined covariance is projected onto the *encounter plane*, which is
perpendicular to the relative velocity. The 2-D Gaussian is then integrated
over the hard-body disc: exactly across it (with `erf`), and by Simpson's
rule along it.

```rust,ignore
let a = sat.propagate_uncertain(&sat_cov, tca)?;
let b = debris.propagate_uncertain(&debris_cov, tca)?;
let pc = collision_probability(&a, &b, 10.0)?; // 5 m + 5 m
println!("{:.2e}, miss {:.2} σ", pc.probability, pc.miss_sigmas);
```

Both states must be in the same inertial frame and at the time of closest
approach; [`catalog::conjunctions`] finds that time. The model does not
apply to slow, co-orbiting encounters that last minutes. These fail with an
error rather than returning a wrong number.

| Check | Result |
|-------|--------|
| Zero miss, circular uncertainty: `1 − exp(−R²/2σ²)` | 1e-10 |
| 30 random encounters, independent implementation (NumPy polar-grid integration, its own encounter-plane basis) | 1.9e-11 |
| The likeliest of them against 2 000 000 Monte Carlo samples (straight-line passes, no projection) | within 0.1 σ |

### Dilution

`cargo run --release -p heavenly --example collision` runs a whole
assessment:

1. A satellite and a piece of debris are tracked today, with RTN
   covariances.
2. Both orbits are integrated with the full force model, and the close
   approach is found.
3. Both covariances are propagated to it, and the probability is computed.

For a 120 m miss 8 hours out, it reports 4.4e-5. Then it scales both
covariances:

| σ × | 10 | 3 | 1 | 0.3 | 0.1 | 0.03 |
|-----|----|---|---|-----|-----|------|
| Probability | 4.7e-7 | 5.2e-6 | 4.4e-5 | 2.2e-4 | 2.7e-6 | 0 |

Better tracking does not simply lower the probability. When the
uncertainty is much larger than the miss, the probability is *diluted*: it
is low because the objects could be anywhere, not because they will miss.
The probability peaks when the uncertainty is comparable to the miss
distance, and only then falls, as the miss becomes certain. A low
probability with a large covariance means "track it better", not "safe".

[`Covariance`]: https://sevki.github.io/heavenly/api/heavenly_core/covariance/struct.Covariance.html
[`UncertainState`]: https://sevki.github.io/heavenly/api/heavenly_core/covariance/struct.UncertainState.html
[`uncertainty::propagate_uncertain`]: https://sevki.github.io/heavenly/api/heavenly_orbit/uncertainty/fn.propagate_uncertain.html
[`NumericalPropagator::propagate_uncertain`]: https://sevki.github.io/heavenly/api/heavenly_orbit/struct.NumericalPropagator.html#method.propagate_uncertain
[`collision_probability`]: https://sevki.github.io/heavenly/api/heavenly_query/fn.collision_probability.html
[`catalog::conjunctions`]: https://sevki.github.io/heavenly/api/heavenly_query/catalog/fn.conjunctions.html
