# Changelog

All crates in the workspace share one version. The minor version increases
with each roadmap milestone (M1 = 0.1, M2 = 0.2, …); patch releases fix bugs
within a milestone. Before 1.0, a minor bump may break the API.

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
- Book: an *Orbits and trajectories* chapter, plus ten diagrams generated
  from the library by `cargo run -p heavenly-diagrams`.

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
