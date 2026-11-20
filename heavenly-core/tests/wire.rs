//! jetstream WireFormat encoding (feature `jetstream`).
#![cfg(feature = "jetstream")]

use std::io::Cursor;

use heavenly_core::wire::{WireFormat, STATE_VERSION};
use heavenly_core::*;

fn round_trip<T: WireFormat + PartialEq + std::fmt::Debug>(v: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    v.encode(&mut buf).unwrap();
    assert_eq!(
        buf.len() as u32,
        v.byte_size(),
        "byte_size must match the encoding"
    );
    let back = T::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(&back, v);
    buf
}

#[test]
fn every_type_round_trips_exactly() {
    let epoch = Epoch::parse("2016-12-31T23:59:60.123456789Z").unwrap();
    round_trip(&epoch);
    round_trip(&Epoch::from_tt_nanoseconds_since_j2000(i128::MIN / 3));
    for f in [
        FrameId::GCRF,
        FrameId::MOON_PA,
        FrameId::custom(Namespace(7), -42).unwrap(),
    ] {
        round_trip(&f);
    }
    round_trip(&LocationId(0x6f1c0b4e_2a43_4f8e_9d1b_5c0e7a3b9f12));
    round_trip(&Vec3::new(6_778_137.0, -1e-300, f64::MAX));
    let r = Rotation::from_axis_angle(Vec3::new(1.0, 2.0, 3.0), 0.7);
    round_trip(&r);
    round_trip(&Transform::new(
        r,
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(4.0, 5.0, 6.0),
        Vec3::new(7e-5, 0.0, 1e-3),
    ));
    let s = StateVector::new(
        FrameId::GCRF,
        epoch,
        Vec3::new(6.8e6, 0.0, 0.0),
        Vec3::new(0.0, 7.6e3, 0.0),
    );
    assert_eq!(round_trip(&s).len(), 77);
}

#[test]
fn state_vector_layout_is_stable() {
    let s = StateVector::new(
        FrameId::GCRF,
        Epoch::J2000.add_nanoseconds(1),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-1.0, 0.5, 0.0),
    );
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();
    let mut expected = vec![STATE_VERSION];
    expected.extend(0u32.to_le_bytes()); // namespace HEAVENLY
    expected.extend(2i64.to_le_bytes()); // GCRF
    expected.extend(1i128.to_le_bytes()); // 1 ns after J2000.0 TT
    for v in [1.0f64, 2.0, 3.0, -1.0, 0.5, 0.0] {
        expected.extend(v.to_le_bytes());
    }
    assert_eq!(buf, expected);
}

#[test]
fn decoding_validates() {
    let s = StateVector::at_rest(FrameId::ITRF, Epoch::J2000, Vec3::X);
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();

    let mut bad_version = buf.clone();
    bad_version[0] = 2;
    assert!(StateVector::decode(&mut Cursor::new(&bad_version)).is_err());

    // An unknown code in the reserved namespace is not a frame.
    let mut bad_frame = Vec::new();
    0u32.encode(&mut bad_frame).unwrap();
    999i64.encode(&mut bad_frame).unwrap();
    assert!(FrameId::decode(&mut Cursor::new(&bad_frame)).is_err());

    let mut zero_quat = Vec::new();
    for _ in 0..4 {
        0.0f64.encode(&mut zero_quat).unwrap();
    }
    assert!(Rotation::decode(&mut Cursor::new(&zero_quat)).is_err());

    // Truncated input is an error, not a panic.
    assert!(StateVector::decode(&mut Cursor::new(&buf[..40])).is_err());
}
