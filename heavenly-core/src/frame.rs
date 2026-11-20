//! Reference-frame identifiers.
//!
//! In `heavenly` a *frame* fixes both an **origin** and a set of **axes**
//! (and how each evolves with time). This differs from SPICE, where frame IDs
//! name only axes and the origin is a separate body ID; a SPICE adapter maps
//! `(center, frame)` pairs onto [`FrameId`]s.
//!
//! Frame IDs are namespaced. Namespace [`Namespace::HEAVENLY`] holds the
//! well-known frames defined as constants on [`FrameId`]; every other
//! namespace is available to applications and adapters (for example one
//! namespace per facility registry or per ephemeris provider).

use core::fmt;

/// A frame-ID namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Namespace(pub u32);

impl Namespace {
    /// The namespace of the well-known frames built into this crate.
    pub const HEAVENLY: Namespace = Namespace(0);
}

/// Identifies a reference frame (origin + axes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId {
    namespace: Namespace,
    code: i64,
}

macro_rules! well_known {
    ($( $(#[$doc:meta])* $name:ident = $code:literal, $label:literal; )*) => {
        impl FrameId {
            $( $(#[$doc])* pub const $name: FrameId = FrameId { namespace: Namespace::HEAVENLY, code: $code }; )*

            /// Short label of a well-known frame, if this is one.
            pub const fn well_known_name(self) -> Option<&'static str> {
                if self.namespace.0 != Namespace::HEAVENLY.0 {
                    return None;
                }
                match self.code {
                    $( $code => Some($label), )*
                    _ => None,
                }
            }
        }
    };
}

well_known! {
    /// Origin: Solar System barycentre. Axes: ICRF. (The BCRS spatial frame.)
    SSB_ICRF = 1, "SSB_ICRF";
    /// Origin: Earth centre of mass. Axes: ICRF. (GCRF.)
    GCRF = 2, "GCRF";
    /// Origin: Earth centre of mass. Axes: ITRF (Earth-fixed, rotating).
    ITRF = 3, "ITRF";
    /// Origin: Moon centre of mass. Axes: ICRF.
    MOON_ICRF = 10, "MOON_ICRF";
    /// Origin: Moon centre of mass. Axes: lunar principal axes (PA).
    MOON_PA = 11, "MOON_PA";
    /// Origin: Moon centre of mass. Axes: lunar mean-Earth / polar axes (ME).
    MOON_ME = 12, "MOON_ME";
    /// Origin: Moon centre of mass. Axes: IAU/WGCCRE Moon body-fixed model.
    /// Differs from the DE440 [`FrameId::MOON_ME`] by up to ≈150 m at the surface.
    MOON_IAU = 13, "MOON_IAU";
    /// Origin: Mars centre of mass. Axes: ICRF.
    MARS_ICRF = 20, "MARS_ICRF";
    /// Origin: Mars centre of mass. Axes: IAU Mars body-fixed.
    MARS_IAU = 21, "MARS_IAU";
}

impl FrameId {
    /// A frame in an application- or adapter-defined namespace.
    ///
    /// Returns `None` for [`Namespace::HEAVENLY`], which is reserved.
    pub const fn custom(namespace: Namespace, code: i64) -> Option<FrameId> {
        if namespace.0 == Namespace::HEAVENLY.0 {
            None
        } else {
            Some(FrameId { namespace, code })
        }
    }

    pub const fn namespace(self) -> Namespace {
        self.namespace
    }

    pub const fn code(self) -> i64 {
        self.code
    }
}

impl fmt::Display for FrameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.well_known_name() {
            Some(name) => f.write_str(name),
            None => write!(f, "{}:{}", self.namespace.0, self.code),
        }
    }
}

impl fmt::Debug for FrameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameId({self})")
    }
}

/// A frame known at compile time, used as the type parameter of
/// [`crate::Coordinate`].
pub trait Frame {
    const ID: FrameId;
}

macro_rules! marker {
    ($( $(#[$doc:meta])* $ty:ident => $id:ident; )*) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $ty {}
        impl Frame for $ty { const ID: FrameId = FrameId::$id; }
    )*};
}

marker! {
    /// Type-level [`FrameId::SSB_ICRF`].
    SsbIcrf => SSB_ICRF;
    /// Type-level [`FrameId::GCRF`].
    Gcrf => GCRF;
    /// Type-level [`FrameId::ITRF`].
    EarthFixed => ITRF;
    /// Type-level [`FrameId::MOON_ICRF`].
    MoonIcrf => MOON_ICRF;
    /// Type-level [`FrameId::MOON_PA`].
    MoonFixed => MOON_PA;
    /// Type-level [`FrameId::MARS_ICRF`].
    MarsIcrf => MARS_ICRF;
    /// Type-level [`FrameId::MARS_IAU`].
    MarsFixed => MARS_IAU;
}
