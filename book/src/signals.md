# Signals and visibility

`StateVector::separation_m` gives the distance between two objects *at the
same instant*. A signal does not travel that path. It leaves one object, and
by the time it arrives the other has moved. `heavenly::query` answers the
questions that matter for communication: how long a signal takes, where to
point, and when the other end can be seen at all.

This is the `heavenly::query` module of the `heavenly` crate (cargo feature `query`, on by default).

## Light time

<figure class="hv-figure">
{{#heavenly-diagram light-time}}
<figcaption>Reception asks where the target was when the light now arriving left it. Transmission asks where it will be when the light arrives.</figcaption>
</figure>

`light_time(observer, target, epoch, frame, resolver, direction)` takes any
two locators and iterates `τ = |r_T(t ∓ τ) − r_O(t)| / c` to 1 ps. It returns:

* `light_time_s`;
* `target_epoch`;
* the corrected `relative_position_m`, which is the pointing direction;
* `range_m`;
* `range_rate_m_s`, the Doppler range rate.

`Direction::Reception` is SPICE's `CN` correction and `Direction::Transmission`
is `XCN`.

τ is a TDB interval, the coordinate time of the barycentric frame. The test
suite caught a subtle bug here: stepping the target epoch by τ in *TT*
instead moved it by up to 0.2 µs over a 10-minute light time, enough to put
a fast body 2 cm out. `Epoch::add_tdb_seconds` now does this exactly.

Not modelled:

* relativistic (Shapiro) delay: about 0.1 ns over an Earth–Moon path, and
  tens of µs for a ray grazing the Sun;
* atmospheric delay.

## Look angles and line of sight

`look_angles(site_frame, target, epoch, resolver)` gives azimuth, elevation,
range and range rate from any east-north-up site frame. That includes a
`heavenly::geo` ENU frame and a `heavenly::id` geodetic mount.

`Occulter::line_of_sight(a, b, epoch, resolver)` returns how far the straight
path between two objects clears a body, which is negative when it is
blocked. `Occulter::EARTH` uses WGS84 in the ITRF, and `Occulter::MOON` uses
the lunar sphere in `MOON_ME`. The value is continuous, so it can drive a
window search. Its sign is exact even for the oblate Earth: for a site on
the surface, the tests check that it agrees with the sign of elevation at
thousands of epochs.

## Windows

`find_windows(start, end, step, tolerance, f)` finds when any continuous
condition `f > 0` holds. It samples every `step` and bisects each crossing
to `tolerance`.

<figure class="hv-figure">
{{#heavenly-diagram visibility}}
<figcaption>Computed by heavenly::query: elevation of an ISS-like orbit from London, with the windows above a 10° mask found by bisection.</figcaption>
</figure>

Windows shorter than the step can be missed, so for low orbits use a step
of about 30 s or less.

## Accuracy

| Check | Reference | Worst |
|-------|-----------|-------|
| `light_time`, both directions, synthetic kernel | SPICE `spkezr` CN / XCN | 0.44 mm, 1.4e-12 s, 1.3e-10 m/s range rate |
| `light_time`, Moon / Sun / Mars from Earth, de440s, 1900–2100 | SPICE `spkezr` CN / XCN | 1.4 cm\*, 4.7e-11 s, 7.6e-9 m/s |
| `find_windows`, 2 days of passes over London | brute-force 1 s sampling | every rise and set within 1 s; bisection to 1 ms |
| `Occulter::EARTH` vs elevation, site on the ellipsoid | the sign of elevation | exact over 2 880 epochs |

\* Limited by SPICE itself: near 1909, `et − lt` in f64 resolves only 0.6 µs.

## Example

```rust,ignore
{{#include ../../heavenly/examples/placement.rs:placement}}
```
