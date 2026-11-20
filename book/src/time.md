# Time and epochs

An `Epoch` is an instant. It does not belong to any one time scale. Internally
it stores integer nanoseconds of Terrestrial Time (TT) since J2000.0
(`2000-01-01T12:00:00 TT`). Conversions to other scales happen only when a
value is read in or written out.

| Scale | Relation | Notes |
|-------|----------|-------|
| TAI | `TT = TAI + 32.184 s` | exact |
| UTC | `TAI = UTC + ΔAT` | built-in IERS leap-second table, 1972 onwards |
| TT  | — | internal representation |
| TDB | `TDB ≈ TT + 1.657 ms·sin g + 13.85 µs·sin 2g` | ≈ 30 µs; use an ephemeris for better |

```rust,ignore
{{#include ../../heavenly-core/examples/time_scales.rs:scales}}
```

<figure class="hv-figure">
{{#heavenly-diagram time-scales}}
<figcaption>Computed by <code>Epoch</code>: leap seconds step TAI − UTC; TT is a fixed 32.184 s after TAI; TDB wobbles about TT by ±1.66 ms over a year.</figcaption>
</figure>

## Parsing

`Epoch::parse` accepts `YYYY-MM-DDTHH:MM:SS[.fffffffff]` followed by a
**mandatory** time scale: `Z`, ` UTC`, ` TAI`, ` TT` or ` TDB`. A timestamp
with no scale is rejected, because guessing the scale silently introduces
errors of tens of seconds.

## Leap seconds

UTC times such as `2016-12-31T23:59:60.5Z` parse and format correctly. A
`:60` that is not a real leap second is rejected. UTC before 1972-01-01
returns `Error::UtcOutOfRange`. TT, TAI and TDB have no such limit.

The leap-second table must be updated when IERS announces a new leap second.
UTC instants after the last entry use the last known offset.

## Precision

Integer nanoseconds give exact arithmetic for all of recorded history. The
`*_seconds_since_j2000` accessors return `f64`, which resolves about 0.1 µs
for present-day epochs.
