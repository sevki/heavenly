//! Connecting kernels to a [`FrameGraph`].

use alloc::sync::Arc;
use alloc::vec::Vec;

use heavenly_core::{Error, FrameGraph, FrameId, Rotation, Transform};

use crate::rot;
use crate::{Pck, SpiceError, Spk};

/// NAIF ID of the Solar System barycentre.
pub const SSB: i32 = 0;
/// NAIF ID of the Earth.
pub const EARTH: i32 = 399;
/// NAIF ID of the Moon.
pub const MOON: i32 = 301;
/// NAIF ID of the Mars barycentre (DE ephemerides such as `de440s.bsp`
/// carry this, not the planet; the two differ by ~20 cm).
pub const MARS_BARYCENTER: i32 = 4;
/// NAIF ID of Mars itself (in satellite kernels such as `mar099.bsp`).
pub const MARS: i32 = 499;

/// NAIF frame class ID of `MOON_PA_DE440` in `moon_pa_de440_*.bpc`.
pub const MOON_PA_DE440: i32 = 31008;

/// The body-centred ICRF frames `register_ephemeris` attaches, with the
/// NAIF body whose centre each is at. [`FrameId::MARS_ICRF`] is centred on
/// Mars itself, so it is only registered if the kernel contains body 499.
pub const STANDARD_BODIES: &[(FrameId, i32)] = &[
    (FrameId::GCRF, EARTH),
    (FrameId::MOON_ICRF, MOON),
    (FrameId::MARS_ICRF, MARS),
];

/// Attaches each `(frame, NAIF body)` in `bodies` that the kernel contains
/// to [`FrameId::SSB_ICRF`], as a pure translation (all of these frames
/// have ICRF axes). Returns the frames that were registered.
///
/// Together with `heavenly-geo`'s Earth orientation and the lunar frames
/// below, this connects body-fixed frames on different bodies.
pub fn register_ephemeris(
    graph: &mut FrameGraph,
    spk: Arc<Spk>,
    bodies: &[(FrameId, i32)],
) -> Result<Vec<FrameId>, Error> {
    let mut added = Vec::new();
    for &(frame, body) in bodies {
        if !spk.segments().iter().any(|s| s.target == body) {
            continue;
        }
        let spk = Arc::clone(&spk);
        graph.add_edge(frame, FrameId::SSB_ICRF, move |epoch| {
            let (p, v) = spk
                .state(body, SSB, epoch)
                .map_err(|e| to_core(e, frame, FrameId::SSB_ICRF, epoch))?;
            Ok(Transform::new(
                Rotation::IDENTITY,
                p,
                v,
                heavenly_core::Vec3::ZERO,
            ))
        })?;
        added.push(frame);
    }
    Ok(added)
}

/// Attaches `MOON_PA` to `MOON_ICRF` using a DE440 lunar binary PCK, and
/// `MOON_ME` to `MOON_PA` using the fixed DE440 PA→ME rotation from NAIF's
/// `moon_de440_250416.tf`.
pub fn register_moon(graph: &mut FrameGraph, pck: Arc<Pck>) -> Result<(), Error> {
    graph.add_edge(FrameId::MOON_PA, FrameId::MOON_ICRF, move |epoch| {
        pck.body_fixed_to_icrf(MOON_PA_DE440, epoch)
            .map_err(|e| to_core(e, FrameId::MOON_PA, FrameId::MOON_ICRF, epoch))
    })?;
    graph.add_fixed(FrameId::MOON_ME, FrameId::MOON_PA, moon_me_to_pa())
}

/// The fixed rotation from `MOON_ME` (DE440/ME421) to `MOON_PA` (DE440).
///
/// The FK defines it as `TKFRAME` angles (67.8526″, 78.6944″, 0.2785″) about
/// axes (3, 2, 1), relative to `MOON_PA_DE440`. Agrees with SPICE to 1e-19.
pub fn moon_me_to_pa() -> Transform {
    const AS: f64 = core::f64::consts::PI / 180.0 / 3600.0;
    // TKFRAME angles give the matrix from this frame (ME) to RELATIVE (PA)
    // as [a₁]ᵢ₁ [a₂]ᵢ₂ [a₃]ᵢ₃ (checked against SPICE `pxform`).
    let me_to_pa = rot::mul(
        &rot::axis(3, 67.8526 * AS),
        &rot::mul(&rot::axis(2, 78.6944 * AS), &rot::axis(1, 0.2785 * AS)),
    );
    let rotation = Rotation::from_matrix(me_to_pa).unwrap_or(Rotation::IDENTITY);
    Transform::fixed(rotation, heavenly_core::Vec3::ZERO)
}

fn to_core(
    e: SpiceError,
    source: FrameId,
    destination: FrameId,
    epoch: heavenly_core::Epoch,
) -> Error {
    match e {
        SpiceError::NoCoverage { .. } => Error::OutOfRange {
            source,
            destination,
            epoch,
        },
        SpiceError::Core(e) => e,
        SpiceError::NoPath { .. } => Error::NoPath {
            source,
            destination,
        },
        _ => Error::InvalidInput("SPICE kernel could not be evaluated"),
    }
}
