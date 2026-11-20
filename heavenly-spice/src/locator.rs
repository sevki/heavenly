//! A [`Locator`] for a body in an SPK ephemeris.

use alloc::sync::Arc;

use heavenly_core::{Epoch, FrameId, FrameResolver, Locator, StateVector};

use crate::frames::SSB;
use crate::{SpiceError, Spk};

/// Locates a NAIF body (planet, moon, spacecraft) from an SPK kernel.
///
/// The state is computed relative to the Solar System barycentre in
/// [`FrameId::SSB_ICRF`], then transformed into the requested frame by the
/// resolver. For geocentric work, register the ephemeris with
/// [`crate::frames::register_ephemeris`] so the resolver can reach `GCRF`.
/// Routing through the barycentre costs ~1e-5 m of f64 rounding.
#[derive(Clone, Debug)]
pub struct SpkLocator {
    pub spk: Arc<Spk>,
    /// NAIF ID of the body, e.g. [`crate::frames::MOON`].
    pub body: i32,
}

impl SpkLocator {
    /// The body's state relative to the Solar System barycentre.
    pub fn state(&self, epoch: Epoch) -> Result<StateVector, SpiceError> {
        let (p, v) = self.spk.state(self.body, SSB, epoch)?;
        Ok(StateVector::new(FrameId::SSB_ICRF, epoch, p, v))
    }
}

impl Locator for SpkLocator {
    type Error = SpiceError;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, SpiceError>
    where
        SpiceError: From<R::Error>,
    {
        let s = self.state(epoch)?;
        Ok(s.to_frame(frame, resolver)?)
    }
}
