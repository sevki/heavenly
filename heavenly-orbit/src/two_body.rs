//! A [`Locator`] for an object in a two-body (Keplerian) orbit.

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector};

use crate::ConicElements;

/// An object following a fixed conic about a central body, for example a
/// spacecraft between manoeuvres, to first order.
///
/// `frame` must be inertial and centred on the attracting body, with the axes
/// the elements are expressed in. Typically that is [`FrameId::GCRF`] for an
/// Earth orbit, or [`FrameId::MOON_ICRF`] / [`FrameId::SSB_ICRF`]. The
/// locator does not check this, because it cannot know.
///
/// Two-body motion ignores every perturbation. For an Earth orbit, the
/// Earth's oblateness (J2) alone moves a low orbit by kilometres per day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBodyLocator {
    pub frame: FrameId,
    pub elements: ConicElements,
}

impl TwoBodyLocator {
    /// The state in the locator's own frame.
    pub fn state(&self, epoch: Epoch) -> Result<StateVector, Error> {
        let (p, v) = self.elements.state_at(epoch)?;
        Ok(StateVector::new(self.frame, epoch, p, v))
    }
}

impl Locator for TwoBodyLocator {
    type Error = Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Error>
    where
        Error: From<R::Error>,
    {
        Ok(self.state(epoch)?.to_frame(frame, resolver)?)
    }
}
