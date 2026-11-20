# Sunlight and eclipses

Whether a satellite is in sunlight decides whether its solar panels charge,
how warm it gets, and whether you can see it from the ground.
`heavenly::query` answers this with the same `Locator`s and frame graph as
everything else.

## The Sun

`AnalyticSun` is a `Locator` for the Sun that needs no kernels. It is a short
analytic series (Meeus, ch. 25), referred to J2000 and placed in GCRF.
Against DE440 over 1950–2050, its direction is good to 32″ and its distance
to 7e-5.

That is plenty for day and night, Sun elevation and the eclipses of Earth
satellites. For more, use the Sun from a JPL ephemeris (`SpkLocator` with
body 10 in `heavenly::spice`). Every function below takes the Sun as any
`Locator`.

## Shadows

`Occulter::shadow` looks from a point towards the Sun past a body (the
Earth, the Moon, or one you define). It returns three angles: the Sun's
apparent radius, the body's apparent radius, and the angle between their
centres. From those:

| Method | Gives |
|--------|-------|
| `kind()` | `Sunlit`, `Penumbra`, `Umbra`, or `Antumbra` (beyond the tip of the umbra, where the body is inside the solar disk) |
| `sunlit_fraction()` | the fraction of the solar disk's area that is visible, 0 to 1 |
| `partial_margin()`, `total_margin()` | continuous signed angles, positive inside the penumbra and the umbra, for `find_windows` |

An oblate body is handled by stretching its polar axis until it is a
sphere. That keeps straight lines straight, so which rays towards the Sun
the body blocks is exact. Only the Sun's apparent radius picks up an error,
of the order of the flattening, about 1″ for the Earth.

`eclipses(...)` searches an interval and returns the partial and total
eclipse windows, with edges refined to a tolerance you choose.

## Visible passes

A satellite is visible to the eye when three things hold at once: it is
above the horizon, it is in sunlight, and the sky at the observer is dark.
Each condition is a signed function, so their minimum is positive exactly
when all three hold, and one `find_windows` search finds the visible
stretches:

```rust,ignore
{{#include ../../heavenly/examples/visible_passes.rs:visible}}
```

`cargo run -p heavenly --example visible_passes` lists seven visible ISS
passes over London in July 2020, from a TLE. Each one ends when the ISS
sets. At midsummer at that latitude the ISS never enters the Earth's
shadow during the night, which is why it can be seen all night.

The [playground](playground.md) shows the same thing live: the shadowed
parts of the ground track, the night side, and the pass list.

## An orbital data centre

A solar-powered data centre in orbit wants as little shadow as possible. A
**dawn–dusk sun-synchronous orbit** gives that. The Earth's oblateness turns
its plane once a year, so the plane keeps facing the Sun and the satellite
rides the day–night line. `design::sun_synchronous_inclination_rad` gives the
inclination, and `AnalyticSun` gives the node that faces the Sun:

```rust,ignore
{{#include ../../heavenly/examples/orbital_datacenter.rs:orbit}}
```

`cargo run -p heavenly --example orbital_datacenter` registers Apple's own
data centres in a `MemoryRegistry`, at city-level, approximate coordinates,
next to two orbits at 550 km. One is the dawn–dusk orbit; the other is a
53° orbit for comparison. It then compares their sunlight and lists each
data centre's contacts on 20 November 2026:

```text
Sunlight over 24 hours from 2026-11-20T00:00:00Z:
  sat:orbital-dc    0 eclipses, in the Earth's shadow  0.0 % of the day
  sat:53-degree    15 eclipses, in the Earth's shadow 36.5 % of the day

Contacts with the orbital data centre above 10°, over 24 hours:
  data centre                passes    in view  longest gap   best one-way
  Maiden, North Carolina          2       15 m       11.7 h        2.23 ms
  Prineville, Oregon              4       26 m       11.5 h        2.72 ms
  Reno, Nevada                    4       25 m       11.5 h        2.73 ms
  Mesa, Arizona                   3       20 m       11.6 h        1.93 ms
  Waukee, Iowa                    4       26 m       11.5 h        3.26 ms
  Viborg, Denmark                 5       32 m        7.6 h        1.92 ms
```

The orbit is illustrative. Published reports of such satellites give only a
range of orbits, "500 to 2,000 kilometers" and "30 degrees to
Sun-Synchronous Orbit"; this is one choice from that range, not a real
satellite's orbit.

The contacts cluster around local dawn and dusk, because that is when a
dawn–dusk orbit passes over any site. That leaves gaps of about half a day,
which is why a constellation, not one satellite, would carry the traffic.
The [playground](playground.md)'s *Orbital data centre* preset shows the
same orbit live.

## Checked against SPICE

`heavenly-query/tests/reference/generate_shadow.py` has SPICE find the same
eclipses with `gfoclt`, which treats the Earth and the Sun as pck00011
ellipsoids. It does this for three two-body orbits: low (51.6°),
geostationary in the March eclipse season, and highly eccentric (63.4°).
SPICE propagates each orbit itself with `prop2b`, which `heavenly-orbit`
matches to 1e-12.

| Orbit | Eclipses | Sun from DE440 | `AnalyticSun` |
|-------|----------|----------------|---------------|
| Low Earth orbit, 2 days | 31 | 3 ms | 0.08 s |
| Geostationary, 5 days | 5 | 28 ms | 1.6 s |
| Highly eccentric, 3 days | 8 | 39 ms | 0.09 s |

The table gives the worst difference in any partial or total eclipse edge.
The geostationary edges move more for the same error in the Sun's
direction, because the satellite crosses the shadow's edge slowly.

The visible fraction of the Sun agrees with brute-force sampling of the
solar disk to 1.5e-4.

**Against Skyfield.** Skyfield's `is_sunlit` treats the Earth as a sphere
of equatorial radius. On a grazing exit from the shadow, like the ISS in
July, the true (oblate) Earth lets the satellite into sunlight earlier.
For the first pass in the example, the ISS leaves the shadow 36 s before
Skyfield says it does. With a spherical `Occulter`, the two agree to the
second.
