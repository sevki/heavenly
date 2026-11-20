# Ephemerides (SPICE)

`heavenly::spice` reads NAIF SPICE kernels, the format JPL uses to publish
planetary and lunar ephemerides, and connects them to the frame graph. It
has no dependencies beyond `libm` and works without `std` (it needs `alloc`).

This is the `heavenly::spice` module of the `heavenly` crate (cargo feature `spice`, on by default).

Kernels are not bundled. Download them from
<https://naif.jpl.nasa.gov/pub/naif/generic_kernels/>. For example:

| File | Contents |
|------|----------|
| `spk/planets/de440s.bsp` (32 MB) | Sun, planet barycentres, Earth, Moon; 1849–2150 |
| `pck/moon_pa_de440_200625.bpc` (13 MB) | DE440 lunar orientation (`MOON_PA`) |

## Putting it together

With `heavenly::geo` for Earth orientation, a rack in London and a lander at
the lunar south pole can be compared directly:

```rust,ignore
{{#include ../../heavenly/examples/earth_moon.rs:earth_moon}}
```

The frame graph this builds is:

<figure class="hv-figure">
{{#heavenly-diagram frame-graph}}
<figcaption>Frames registered by the example. The SPK supplies the translations to the barycentre, and the PCK and FK supply the lunar orientation.</figcaption>
</figure>

## The file format

<figure class="hv-figure">
{{#heavenly-diagram daf}}
<figcaption>How an SPK file is laid out, and how one Chebyshev record is evaluated.</figcaption>
</figure>

## What is supported

* **SPK types 2 and 3** (Chebyshev position; position and velocity). These
  are the types JPL's DE ephemerides use. Other types are listed by
  `Spk::segments()`, and evaluating them returns `UnsupportedType`.
* **Binary PCK type 2** (Chebyshev Euler angles).
* Little- and big-endian files.
* SPICE's segment precedence: the last segment in the file that covers the
  epoch wins.
* Chains of centres, e.g. Moon → Earth–Moon barycentre → Solar System
  barycentre ← Earth.

Segments must be relative to the `J2000` frame. DE ephemerides use that frame
and it is aligned with the ICRF.

## Accuracy

| Check | Reference | Worst difference |
|-------|-----------|------------------|
| Synthetic SPK (types 2+3, overlapping segments, both byte orders) | SPICE `spkgeo` | 7e-5 m (f64 at 1.5e11 m) |
| `de440s.bsp`: Moon–Earth, Earth–SSB, Mars bc–SSB, Sun–Earth, 1900–2140 | SPICE `spkgeo` | 7e-5 m, 2e-11 m/s |
| Synthetic binary PCK | SPICE `sxform` | 5e-15 rad |
| `moon_pa_de440_200625.bpc`, 1900–2140 | SPICE `sxform` | 5e-12 rad (≈9 µm), 1e-17 rad/s |
| `MOON_ME` → `MOON_PA` fixed rotation | SPICE `pxform` | 1e-19 |
| London rack ↔ Shackleton lander, end to end | SPICE with `earth_latest_high_prec.bpc` | 26 m in 382 666 km |

The kernel readers reproduce SPICE to rounding. The 26 m end-to-end
difference comes from the Earth side. The example uses illustrative EOP
values, while SPICE uses JPL's measured Earth orientation; see
[Earth, Moon and Mars](earth-and-moon.md).

### Time

Kernels are indexed by TDB seconds past J2000 ("ET"). `Epoch` converts TT to
TDB with a two-term series that is good to about 30 µs. That shifts the Earth
relative to the barycentre by up to about 1 m, and Earth–Moon geometry by
about 3 cm. Two methods, `Spk::state_at_et` and `Pck::body_fixed_to_icrf_at_et`,
take ET directly. Use them when you must match SPICE exactly, for example at
segment boundaries, where rounding an `Epoch` to the nanosecond can move the
instant across the boundary.

## Testing

Two kinds of test run in CI:

* **Synthetic kernels.** `tests/reference/generate.py` writes small SPK and
  PCK files with SPICE itself (`spkw02`, `spkw03`, `pckw02`), plus a
  big-endian copy. It also records SPICE's answers. These run on every build.
* **Real DE440.** The `kernels` CI job downloads `de440s.bsp` and the lunar
  PCK and runs the ignored tests and the example. To run them locally:
  `HEAVENLY_KERNELS=<dir> cargo test -p heavenly-spice -p heavenly-query -- --ignored`.
