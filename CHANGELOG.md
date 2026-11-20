# Changelog

All crates in the workspace share one version. The minor version increases
with each roadmap milestone (M1 = 0.1, M2 = 0.2, …); patch releases fix bugs
within a milestone. Before 1.0, a minor bump may break the API.

## Unreleased

### Added
- Sunlight and eclipses in `heavenly::query`:
  - `AnalyticSun`, a kernel-free Sun `Locator`, good to 32″ against DE440
    over 1950–2050;
  - `Occulter::shadow` / `shadow_at`, returning a `Shadow` with its kind
    (sunlit, penumbra, umbra, antumbra), the visible fraction of the solar
    disk, and continuous margins for `find_windows`;
  - `eclipses()`, which finds partial and total eclipse windows.

  Eclipse edges match SPICE `gfoclt` (pck00011 ellipsoids, DE440) to
  3–39 ms for LEO, GEO and HEO orbits. The new conformance generator is
  `generate_shadow.py`.
- Example `visible_passes`: ISS passes over London that are actually
  visible (sunlit satellite, dark sky). It is cross-checked with Skyfield.
- Playground:
  - TLE input propagated with SGP4, plus an ISS TLE preset;
  - the orbit and ground track are greyed where the satellite is in the
    Earth's shadow;
  - the night side is shaded, and the Sun is marked;
  - the readout shows the satellite's sunlight and London's Sun elevation;
  - a clickable list of passes over London marks which are visible.

  All of it is computed by the library in WebAssembly.

- **Batch operations with SIMD.** These give results bit-identical to the
  single-item methods, on every platform, with or without SIMD:
  - `Rotation::apply_batch`;
  - `Transform::apply_positions` and `Transform::apply_batch`;
  - `FrameTransform::apply_batch`;
  - `StateVector::to_frame_batch`, which resolves one transform per run of
    states sharing a frame and epoch.

  The new default `simd` feature (via `wide` 0.7, `no_std`, MSRV 1.61)
  processes position-and-velocity transforms four at a time with AVX,
  SSE2, NEON or WebAssembly `simd128`. Each lane does the same IEEE
  operations in the same order as the scalar code, with no fused
  multiply-add. Measured on x86_64 (`cargo bench -p heavenly-core --bench
  batch`), SIMD makes `Transform::apply_batch` 1.6× faster with SSE2 and
  3.1× with AVX-512, and `to_frame_batch` 2.7× and 4.0×.
  `to_frame_batch` is 14–56× faster than `to_frame` in a loop.

  Rotation-only batches stay plain loops: the compiler already vectorises
  them, and an explicit SIMD kernel measured no faster. Single-vector
  operations are unchanged, because there isn't enough work in one 3-vector
  operation for SIMD to pay.

  CI checks parity with AVX2, NEON (arm64 runners) and `simd128`
  (`wasm-pack`), and builds `simd` without std.

### Changed
- **`heavenly::id::Registry` is now a trait.** Implement its four lookup
  methods (`namespace`, `entity`, `lookup_alias`, `for_each_entity`) over
  any store, such as a database or an inventory service. Resolving, frames,
  paths, validation, frame-graph registration and locators are provided
  methods, identical for every store. A store reports its own failures
  through an associated `Error` type.
  - The in-memory struct is now `MemoryRegistry`, and keeps the write
    operations.
  - `children` returns owned entities and `path` returns `Cow`s.
  - `validate` now also detects frame-code collisions, which matters for
    stores that don't check on insert.
  - `entity_frame(namespace, id)` is public.

### Added
- `heavenly::geo::eop::EopTable`: IERS Earth orientation parameters from
  `finals2000A` (including predictions, Bulletin B values where available) or
  EOP 20 C04 files. It interpolates with four-point Lagrange polynomials on
  UT1 − TAI, so leap seconds are handled exactly, never extrapolates, and
  implements `EopProvider`, so it plugs into `earth::register`. It is checked
  against astropy's IERS readers. Works without std (it needs `alloc`).
- CI: WebAssembly. A `wasm32-unknown-unknown` job builds `heavenly` without
  std, with alloc and with std, plus every crate's tests and examples. A
  `wasm-pack` job runs a portable conformance suite (`heavenly/tests/
  conformance.rs`: SGP4, TEME, Earth orientation, two-body, SPK and light
  time against their references, with the synthetic kernel compiled in)
  in Node with wasm-bindgen-test. The same suite runs natively.

## 0.6.0: M6, TLEs and SGP4

### Added
- `heavenly::orbit::Tle`: NORAD two-line element sets, with checksum,
  line-number and catalogue-number checks, Alpha-5 catalogue numbers and
  implied-decimal fields (`Tle::parse`, `Tle::parse_unchecked`, `checksum`).
- `heavenly::orbit::Sgp4`: a port of Vallado's SGP4/SDP4 reference
  implementation (AIAA 2006-6753, 2020 revision), including lunar–solar
  perturbations and 12-hour and synchronous resonances. It supports WGS-72,
  WGS-72 (old) and WGS-84 gravity and the improved and AFSPC modes, reports
  the reference error codes (`Sgp4Error`), and is a `Locator`. It matches
  Vallado's C++ to 1.5e-9 km over the official verification set in all four
  gravity/mode combinations.
- `FrameId::TEME` (code 4), and a TEME → ITRF edge in `earth::register`
  (GMST 1982 plus polar motion), checked against Skyfield. `earth::gmst82`
  and `earth::teme_to_itrf` are public.
- Example `tle_passes`: ISS passes over London from a TLE.
- Book chapter *TLEs and SGP4*.
- CI: a conformance job (the old reference-data workflow, now also run on
  every push and pull request) regenerates the SOFA, SPICE, Vallado SGP4
  and Skyfield reference values, checks the committed copies, and runs the
  conformance tests, including those on real DE440 kernels, on Linux x64
  and arm64. It writes the worst differences to the job summary. The test
  matrix now covers Linux, macOS and Windows on both x64 and arm64.
- Optional `jetstream` feature (off by default): `heavenly::wire` implements
  [jetstream](https://github.com/sevki/jetstream)'s `WireFormat` for
  `Epoch`, `FrameId`, `LocationId`, `Vec3`, `Rotation`, `Transform` and
  `StateVector`. Each type has a fixed little-endian layout (a state is
  77 bytes) and decoding validates its input. The feature needs std and
  jetstream's toolchain (Rust 1.91). Default builds keep the 1.70 MSRV and
  their libm-only dependencies.
- `FrameId::parse` / `FromStr` (the inverse of `Display`). It rejects the
  ambiguous bare `ICRF` / `J2000`.
- `LocationId::hyphenated()`: the canonical UUID form.

### Changed
- The README and book install from crates.io.

## 0.5.0: M5, spatial and operational queries

### Changed
- **One crate to depend on: `heavenly`.** It re-exports the core types at
  its root and the rest as feature-gated modules: `heavenly::geo`, `::spice`,
  `::orbit`, `::id`, `::query` and `::topology`, all on by default. The
  `heavenly-*` crates are now internal building blocks. The examples moved
  to `heavenly/examples/`, and the book and README use
  `heavenly = { git = … }` and `use heavenly::…`.

### Added
- `heavenly::query` (crate `heavenly-query`):
  - `light_time`: converged Newtonian light time in both directions
    (`Direction::Reception` / `Transmission`, SPICE's CN / XCN), with the
    corrected relative position and range rate;
  - `look_angles`: azimuth, elevation, range and range rate from a site;
  - `Occulter`: line of sight past the oblate Earth or the Moon;
  - `find_windows`: rise/set intervals of any condition, by bisection.
- `heavenly::topology` (crate `heavenly-topology`): failure domains and
  jurisdictions over `LocationId`s. Domain dependencies form a DAG, and the
  queries are `failure_domains`, `shared_failure_domains`, `independent`,
  `blast_radius` and `domains_of_kind`.
- `heavenly-core`: `Epoch::add_tdb_seconds`. Light time is a TDB interval;
  stepping by TT instead put fast bodies up to 2 cm out.
- Book: *Signals and visibility* and *Placement topology* chapters, with
  computed light-time, visibility and topology diagrams; the `placement`
  example.

### Fixed
- Book build: mdbook silently skips a preprocessor whose `supports` probe
  fails, so a diagram crate that did not compile produced pages with raw
  directives. `book/heavenly-preprocessor.sh` now answers the probe without
  building, and CI fails if any directive survives into the HTML.

### Validation
- Light time against SPICE `spkezr` CN / XCN: 0.44 mm on the synthetic
  kernel; on DE440, 1.4 cm (limited by SPICE's f64 epoch near 1909) and
  4.7e-11 s.
- Visibility windows: every rise and set within 1 s of brute-force 1 s
  sampling.
- Line of sight past the Earth: the sign agrees exactly with elevation for a
  site on the ellipsoid.

## 0.4.0: M4, local frames and identity

### Added
- `heavenly-id`, a new crate:
  - `Registry` of entities (sites, buildings, rooms, racks, devices,
    spacecraft, modules…) keyed by stable `LocationId`;
  - `Alias` names that can be renamed or re-pointed without changing
    identity;
  - `Mount`s that attach each entity's local frame to its parent: fixed,
    geodetic (ENU on a body), or on a trajectory with `Attitude::Inertial`
    or `Attitude::Lvlh`;
  - `register_frames` builds the frame-graph edges for the whole tree, and
    `locate` and `locator` resolve any entity by ID or alias;
  - explicit errors for duplicates, unknown keys, missing parents, cycles,
    non-locatable groups and frame-code collisions.
- `heavenly-core`: `LocationId::parse` and `FromStr` (32 hex digits or a
  hyphenated UUID), plus `to_bytes`, `from_bytes` and `From<u128>`.
- Project logo in the README, the book (introduction page and favicon) and
  every crate's rustdoc (`html_logo_url` / `html_favicon_url`).
- Book: an *Identity and local frames* chapter with a registry diagram, and
  the design proposal's London-rack-to-orbiting-rack example
  (`cargo run -p heavenly-id --example racks`).

## 0.3.0: M3, moving objects

### Added
- `heavenly-orbit`, a new crate:
  - `kepler::propagate`: universal-variable two-body propagation, using one
    algorithm for elliptic, parabolic and hyperbolic orbits.
  - `ConicElements`: elements ↔ state vectors, in SPICE `conics` / `oscelt`
    conventions.
  - `TwoBodyLocator` and `SampledTrajectory` (cubic Hermite) locators.
  - `gm`: DE440 gravitational parameters.
- `heavenly-spice`: `SpkLocator`, which turns any body in an SPK kernel into
  a `Locator`.
- `heavenly-core`:
  - `Error::OutsideCoverage` and `Error::DataGap` for missing trajectory
    data;
  - `Epoch::tdb_seconds_since` and `Epoch::tdb_minus_tt_seconds` for precise
    TDB intervals.
- Book: an *Orbits and trajectories* chapter, plus ten diagrams drawn by
  an mdbook preprocessor (`heavenly-diagrams`) from the library at build
  time (`{{#heavenly-diagram NAME}}`).
- Book: an interactive **Playground**, with a 3D orbit view, a ground track
  and London visibility. It is computed in the browser by the heavenly crates
  compiled to WebAssembly (`heavenly-viz`, 86 KB, no JS bindings).

### Validation
- Checked against SPICE `prop2b`, `conics` and `oscelt`: worst relative error
  3.4e-12.
- Sampled-trajectory interpolation stays within 1 % of the theoretical
  h⁴/384·rω⁴ bound.

## 0.2.0: M2, Earth and Moon

### Added
- `heavenly-geo`:
  - ellipsoids and geodetic ↔ Cartesian conversion;
  - east-north-up site frames;
  - ITRF ↔ GCRF Earth orientation, within 7.6 mas of SOFA;
  - IAU Moon and Mars rotation models.
- `heavenly-spice`:
  - a dependency-free SPK (types 2/3) and binary PCK (type 2) reader;
  - frame registration for the barycentre, Earth, Moon, `MOON_PA` and
    `MOON_ME`.
  - It reproduces SPICE to rounding on DE440.
- `heavenly-core`: `Rotation::from_matrix`, `Epoch::tai_minus_utc_seconds`,
  `FrameId::MOON_IAU` and `Error::InvalidInput`.

## 0.1.0: M1, core types and invariants

### Added
- `heavenly-core`:
  - `Epoch` with UTC (including leap seconds), TAI, TT and TDB;
  - namespaced `FrameId`;
  - `Transform` / `FrameTransform`, with a documented velocity convention;
  - `StateVector` and the compile-time-framed `Coordinate<F>`;
  - `FrameGraph`;
  - `Locator`, `FixedLocator` and `LocationId`.
- The mdbook user guide and CI.
