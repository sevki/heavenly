//! Compact binary encoding with [jetstream](https://github.com/sevki/jetstream)'s
//! `WireFormat` (feature `jetstream`, which implies `std`).
//!
//! Every type has a fixed size and a little-endian layout:
//!
//! | Type | Bytes | Layout |
//! |------|------:|--------|
//! | [`Epoch`] | 16 | `i128` nanoseconds of TT since J2000.0 |
//! | [`FrameId`] | 12 | `u32` namespace, `i64` code |
//! | [`LocationId`] | 16 | `u128` |
//! | [`Vec3`] | 24 | `f64` x, y, z |
//! | [`Rotation`] | 32 | `f64` quaternion w, x, y, z |
//! | [`Transform`] | 104 | rotation, translation (m), origin velocity (m/s), angular velocity (rad/s) |
//! | [`StateVector`] | 77 | `u8` version (1), frame, epoch, position (m), velocity (m/s) |
//!
//! Units are always SI, and an epoch is an exact instant, so nothing is lost
//! in a round trip. Decoding validates what it reads: frames in the reserved
//! namespace must be well-known, quaternions must be normalisable, and
//! unknown `StateVector` versions are rejected.

use std::io::{self, ErrorKind, Read, Write};

pub use jetstream_wireformat::WireFormat;

use crate::{Epoch, FrameId, LocationId, Namespace, Rotation, StateVector, Transform, Vec3};

/// Version byte written before every [`StateVector`].
pub const STATE_VERSION: u8 = 1;

fn invalid(msg: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, msg)
}

impl WireFormat for Epoch {
    fn byte_size(&self) -> u32 {
        16
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        self.tt_nanoseconds_since_j2000().encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        Ok(Epoch::from_tt_nanoseconds_since_j2000(i128::decode(r)?))
    }
}

impl WireFormat for FrameId {
    fn byte_size(&self) -> u32 {
        12
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        self.namespace().0.encode(w)?;
        self.code().encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        let ns = u32::decode(r)?;
        let code = i64::decode(r)?;
        if ns == Namespace::HEAVENLY.0 {
            let mut s = String::new();
            use std::fmt::Write as _;
            let _ = write!(s, "0:{code}");
            FrameId::parse(&s).map_err(|_| invalid("unknown frame code in the HEAVENLY namespace"))
        } else {
            FrameId::custom(Namespace(ns), code).ok_or_else(|| invalid("invalid frame"))
        }
    }
}

impl WireFormat for LocationId {
    fn byte_size(&self) -> u32 {
        16
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        self.0.encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        Ok(LocationId(u128::decode(r)?))
    }
}

impl WireFormat for Vec3 {
    fn byte_size(&self) -> u32 {
        24
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        self.x.encode(w)?;
        self.y.encode(w)?;
        self.z.encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        Ok(Vec3::new(f64::decode(r)?, f64::decode(r)?, f64::decode(r)?))
    }
}

impl WireFormat for Rotation {
    fn byte_size(&self) -> u32 {
        32
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        for q in self.quaternion() {
            q.encode(w)?;
        }
        Ok(())
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        let q = [
            f64::decode(r)?,
            f64::decode(r)?,
            f64::decode(r)?,
            f64::decode(r)?,
        ];
        Rotation::from_quaternion(q[0], q[1], q[2], q[3])
            .ok_or_else(|| invalid("quaternion is zero or not finite"))
    }
}

impl WireFormat for Transform {
    fn byte_size(&self) -> u32 {
        104
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        self.rotation.encode(w)?;
        self.translation_m.encode(w)?;
        self.origin_velocity_m_s.encode(w)?;
        self.angular_velocity_rad_s.encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        Ok(Transform::new(
            Rotation::decode(r)?,
            Vec3::decode(r)?,
            Vec3::decode(r)?,
            Vec3::decode(r)?,
        ))
    }
}

impl WireFormat for StateVector {
    fn byte_size(&self) -> u32 {
        1 + 12 + 16 + 24 + 24
    }
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        STATE_VERSION.encode(w)?;
        self.frame.encode(w)?;
        self.epoch.encode(w)?;
        self.position_m.encode(w)?;
        self.velocity_m_s.encode(w)
    }
    fn decode<R: Read>(r: &mut R) -> io::Result<Self> {
        let version = u8::decode(r)?;
        if version != STATE_VERSION {
            return Err(invalid("unsupported StateVector wire version"));
        }
        Ok(StateVector::new(
            FrameId::decode(r)?,
            Epoch::decode(r)?,
            Vec3::decode(r)?,
            Vec3::decode(r)?,
        ))
    }
}
