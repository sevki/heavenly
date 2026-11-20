# TLEs and SGP4

Most satellites are published as NORAD **two-line element sets** (TLEs),
for example by [CelesTrak](https://celestrak.org) and Space-Track. A TLE is
only meaningful through the propagator it was fitted with, **SGP4**: its
elements are SGP4 *mean* elements, not osculating ones. Feeding them to a
two-body propagator gives errors of tens of kilometres within hours.

`heavenly::orbit` has both parts:

* `Tle::parse(line1, line2)` reads a TLE. It checks the line numbers, the
  matching catalogue numbers and both checksums, and decodes Alpha-5
  catalogue numbers (`A0001` = 100001) and the implied-decimal fields
  (`-31515-4` = −0.31515 × 10⁻⁴). `Tle::parse_unchecked` skips the
  length and checksum checks, for old or hand-edited sets.
* `Sgp4::new(&tle, gravity, opsmode)` initialises the propagator, and
  `Sgp4` is a `Locator`. Its states are in [`FrameId::TEME`](frames.md),
  in metres and m/s. `propagate(minutes)` gives the raw output in km and
  km/s, as the reference implementation does.

```rust,ignore
{{#include ../../heavenly/examples/tle_passes.rs:sgp4}}
```

`cargo run -p heavenly --example tle_passes` prints the passes. Their rise
and set times match Skyfield's to the second.

## The implementation

This is a port of David Vallado's reference implementation, from
*Revisiting Spacetrack Report #3* (AIAA 2006-6753, revised 2020), the
version CelesTrak, the `sgp4` Python package and Skyfield use. It includes
the deep-space branch (SDP4), used for orbits with periods of 225 minutes or
more:

* lunar and solar perturbations, with the Lyddane modification at low
  inclination;
* resonance integration for 12-hour orbits (such as Molniya and GPS) and
  for synchronous (geostationary) orbits.

| Option | Values |
|--------|--------|
| `Gravity` | `Wgs72` (default: the model TLEs are fitted with), `Wgs72Old`, `Wgs84` |
| `OpsMode` | `Improved` (default), `Afspc` (matches legacy AFSPC output) |

Propagation is stateless. Each call integrates the resonance terms from the
epoch in the reference's fixed 720-minute steps, so results don't depend on
the order of the calls, and they are the same as the reference's.

SGP4 reports failures with the reference's error codes as `Sgp4Error`. For
example `Decayed` (code 6) means the orbit radius fell below one Earth
radius.

### Time

TLE epochs are UTC. SGP4 counts time in minutes from the epoch on a UTC
day count, so a leap second in between is not counted.
`Sgp4::minutes_since_epoch` does the same, so states agree with other SGP4
software across leap seconds.

## TEME

SGP4 works in **TEME** (True Equator, Mean Equinox), an Earth-centred
quasi-inertial frame of date. `earth::register` links it to ITRF, as in
Vallado et al. (2006), appendix C:

```text
r_ITRF = W · R3(GMST82) · r_TEME
```

where GMST82 is the IAU 1982 Greenwich Mean Sidereal Time from UT1
(`earth::gmst82`), and `W` is polar motion from the same
`EarthOrientation` the ITRF → GCRF edge uses. From ITRF, the frame graph
reaches GCRF and every site and body frame, so a TLE satellite works with
`look_angles`, `light_time` and `find_windows` like any other locator.

To get GCRF from TEME, go through ITRF as the graph does. TEME is defined
by the equinox SGP4 uses, not by a rotation from GCRF, and treating it as
a precession–nutation rotation of GCRF introduces errors of up to tens of
metres.

## Accuracy

The **implementation** matches the reference to rounding. **SGP4 itself**
is much less accurate. A fresh TLE is typically good to about 1 km at
epoch, and the error grows by 1–3 km per day. For better than that, use
ephemerides or tracking data as a [`SampledTrajectory`](orbits.md).

| Check | Reference | Worst difference |
|-------|-----------|------------------|
| SGP4/SDP4, 2,636 states, 33 TLEs × 4 gravity/mode combinations | Vallado's C++ (`sgp4` 2.27) | 1.5e-9 km, 8e-13 km/s |
| Error codes and decay | the same | identical |
| TLE fields | the same parser | ≤ 1e-15 relative |
| GMST (IAU 1982) | SOFA `iauGmst82` | 4.4e-14 rad |
| TEME → ITRF with polar motion | Skyfield `TEME_to_ITRF` | 5.9e-9 m, 4.5e-12 m/s |

The 33 TLEs are the official verification set (`SGP4-VER.TLE`). It covers
near-Earth and deep-space orbits, 12-hour and synchronous resonances, high
eccentricities, low inclinations, negative times and decays.
