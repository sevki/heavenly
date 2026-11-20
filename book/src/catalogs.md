# Catalogues and constellations

One satellite is a `Locator`. A catalogue is thousands of them, and the
questions change: which ground sites can reach which satellites, which
satellites can see each other, and which come close to each other.

## OMM: the successor to the TLE

The TLE has room for only five catalogue digits, and they are running out.
CelesTrak and Space-Track publish the same SGP4 mean elements as CCSDS
**Orbit Mean-Elements Messages** (OMM). `heavenly::orbit::Omm` reads the three
forms they serve, without serde:

| Form | Function | CelesTrak |
|------|----------|-----------|
| KVN (`KEY = value`), one or more messages | `Omm::parse_kvn` | `FORMAT=kvn` |
| JSON array of objects | `Omm::parse_json` | `FORMAT=json` |
| CSV with a header row | `Omm::parse_csv` | `FORMAT=csv` |

A message whose `MEAN_ELEMENT_THEORY` is not SGP4, whose `REF_FRAME` is
not TEME, whose `CENTER_NAME` is not EARTH or whose `TIME_SYSTEM` is not
UTC is rejected, because SGP4 cannot use it.

`Sgp4::from_omm` follows the reference implementation's own OMM path
(`sgp4.omm.initialize`). That path converts the mean motion and the epoch
slightly differently from the TLE path, so the results match it to the
last digits.

## A catalogue

`Catalog::from_tle_text` loads two- or three-line sets, and
`Catalog::from_omms` loads OMMs. A set that does not parse (a bad checksum)
or does not initialise (an orbit that has already decayed) comes back as a
`Rejected` entry, which says where it was and why. One bad set does not
fail the whole catalogue.

`Catalog::states_at(epoch, frame, resolver)` propagates every object to
one epoch. Each is propagated in TEME, then all of them move into `frame`
with one transform, using `StateVector::to_frame_batch`. That uses SIMD
with the `simd` feature, and the results are bit-identical to one object
at a time. Objects SGP4 cannot place come back as `None`.

## Questions about many objects

`heavenly::query::catalog` takes positions through two functions: `all(t)`
for a snapshot of every object, and `one(t, i)` for one object while
refining an event's time. A `Catalog`'s `positions_at` and `position_of`
fit directly, but so does any other source of positions. The `constellation`
example uses two-body Walker shells:

```rust,ignore
{{#include ../../heavenly/examples/constellation.rs:snapshot}}
```

| Function | Finds | How |
|----------|-------|-----|
| `contacts` | when each object is above a mask angle from each site | samples, then bisects each edge. Each site's transform is resolved once and applied to all objects as a batch |
| `links` | which pairs are in range and see each other past the Earth | range, then `Occulter::clearance_m`, with a minimum clearance (for example 80 km of atmosphere) |
| `conjunctions` | closest approaches under a threshold | samples each pair's distance, then refines every local minimum that could hide a close approach with golden-section search |

A conjunction sampled every `step_s` can sit up to (relative speed ×
`step_s`) above its true miss distance. So each pair's guard uses its own
measured relative speed, and a fast encounter between samples is not
missed. `links` and `conjunctions` check every pair, so their cost grows
with the square of the number of objects.

`cargo run -p heavenly --example constellation` runs all three for a
24-satellite Walker shell (24/4/1 at 550 km, 53°) and Apple's data centres:

```text
Inter-satellite links at 2026-11-20T00:00:00Z (≤ 5 000 km, clearing the Earth by 80 km): 18 links, 1–2 per satellite

Coverage of Apple's data centres over 24 h (a satellite above 25°):
  Maiden       15.8 % of the day, longest gap 169.9 min
  Prineville   25.1 % of the day, longest gap  99.1 min
  ...

Close approaches under 100 km over 6 h, with a crossing 97.6° shell 1 km higher:
  within the 53° shell:                0
  within the 97.6° shell:              180 (its planes all cross near the poles, and a
    phasing of 1 brings satellites of different planes 5.3 km apart there)
  between the two shells:              142, the closest:
  ...
```

The second shell shows why conjunction screening matters. A sun-synchronous
Walker pattern with phasing 1 brings its own satellites from different
planes within 5.3 km of each other over the poles, every few minutes.

The [playground](playground.md)'s *Constellation* preset shows the shell
live: every satellite, its links, and which data centres it covers.

## Checked

| Check | Against | Result |
|-------|---------|--------|
| SGP4 from OMM, 656 states | Vallado's C++ through `sgp4.omm.initialize` | 2.8e-10 km |
| An OMM and its TLE twin | each other | 4 mm (the formats round the epoch and mean motion differently) |
| KVN, JSON, CSV | each other | identical |
| `Catalog::states_at` | each object propagated and converted on its own | bit-identical |
| `contacts`, 3 sites × 12 satellites, 6 h | `find_windows` + `look_angles` for each pair | the same windows, edges within 3 ms |
| `conjunctions`, 48 satellites in crossing shells, 6 h | every pair's distance every second | the same 324 approaches; times within the 0.5 s sampling |
