use heavenly_core::{Epoch, Error, TimeScale};

fn p(s: &str) -> Epoch {
    Epoch::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn j2000_in_every_scale() {
    assert_eq!(p("2000-01-01T12:00:00 TT"), Epoch::J2000);
    assert_eq!(p("2000-01-01T11:59:27.816 TAI"), Epoch::J2000);
    // TAI − UTC was 32 s in 2000.
    assert_eq!(p("2000-01-01T11:58:55.816Z"), Epoch::J2000);
    assert_eq!(p("2000-01-01T11:58:55.816 UTC"), Epoch::J2000);
    // TDB − TT at J2000 is about −0.1 ms; the epoch must differ from J2000 by that.
    let d = p("2000-01-01T12:00:00 TDB").seconds_since(Epoch::J2000);
    assert!(d.abs() < 2e-3 && d != 0.0, "{d}");
}

#[test]
fn leap_second_2016() {
    let before = p("2016-12-31T23:59:59Z");
    let leap = p("2016-12-31T23:59:60Z");
    let after = p("2017-01-01T00:00:00Z");
    assert_eq!(leap.seconds_since(before), 1.0);
    assert_eq!(after.seconds_since(leap), 1.0);
    assert_eq!(after.seconds_since(before), 2.0);

    let mid = p("2016-12-31T23:59:60.5Z");
    assert_eq!(
        mid.to_calendar(TimeScale::Utc).unwrap().to_string(),
        "2016-12-31T23:59:60.5Z"
    );
    assert_eq!(
        after.to_calendar(TimeScale::Utc).unwrap().to_string(),
        "2017-01-01T00:00:00Z"
    );
    assert_eq!(
        before
            .add_seconds(0.999_999_999)
            .unwrap()
            .to_calendar(TimeScale::Utc)
            .unwrap()
            .to_string(),
        "2016-12-31T23:59:59.999999999Z"
    );
    // TAI − UTC = 37 s after the leap.
    assert_eq!(
        after.to_calendar(TimeScale::Tai).unwrap().to_string(),
        "2017-01-01T00:00:37 TAI"
    );
}

#[test]
fn invalid_leap_seconds_rejected() {
    assert_eq!(
        Epoch::parse("2016-12-30T23:59:60Z"),
        Err(Error::InvalidCalendar)
    );
    assert_eq!(
        Epoch::parse("2016-12-31T23:58:60Z"),
        Err(Error::InvalidCalendar)
    );
    assert_eq!(
        Epoch::parse("2016-12-31T23:59:60 TAI"),
        Err(Error::InvalidCalendar)
    );
    assert_eq!(
        Epoch::parse("2016-12-31T23:59:61Z"),
        Err(Error::InvalidCalendar)
    );
}

#[test]
fn calendar_validation() {
    assert_eq!(
        Epoch::parse("2026-02-29T00:00:00Z"),
        Err(Error::InvalidCalendar)
    );
    assert!(Epoch::parse("2024-02-29T00:00:00Z").is_ok());
    assert_eq!(
        Epoch::parse("2026-13-01T00:00:00Z"),
        Err(Error::InvalidCalendar)
    );
    assert_eq!(
        Epoch::parse("2026-11-19T24:00:00Z"),
        Err(Error::InvalidCalendar)
    );
}

#[test]
fn utc_before_1972_is_rejected_but_tt_is_not() {
    assert_eq!(
        Epoch::parse("1969-07-20T20:17:40Z"),
        Err(Error::UtcOutOfRange)
    );
    let e = p("1969-07-20T20:17:40 TT");
    assert_eq!(e.to_calendar(TimeScale::Utc), Err(Error::UtcOutOfRange));
    assert_eq!(
        e.to_calendar(TimeScale::Tt).unwrap().to_string(),
        "1969-07-20T20:17:40 TT"
    );
    // First instant of the table.
    assert_eq!(
        p("1972-01-01T00:00:00Z")
            .to_calendar(TimeScale::Tai)
            .unwrap()
            .to_string(),
        "1972-01-01T00:00:10 TAI"
    );
}

#[test]
fn parse_requires_explicit_scale() {
    assert!(matches!(
        Epoch::parse("2026-11-19T17:00:00"),
        Err(Error::Parse(_))
    ));
    assert!(matches!(
        Epoch::parse("2026-11-19T17:00:00 GPS"),
        Err(Error::Parse(_))
    ));
    assert!(matches!(
        Epoch::parse("2026-11-19 17:00:00Z"),
        Err(Error::Parse(_))
    ));
    assert!(matches!(
        Epoch::parse("2026-11-19T17:00:00.Z"),
        Err(Error::Parse(_))
    ));
    assert!(matches!(
        Epoch::parse("2026-11-19T17:00:00.0123456789Z"),
        Err(Error::Parse(_))
    ));
}

#[test]
fn display_round_trips_in_every_scale() {
    let base = p("2026-11-19T17:00:00.123456789Z");
    for scale in [
        TimeScale::Utc,
        TimeScale::Tai,
        TimeScale::Tt,
        TimeScale::Tdb,
    ] {
        for dt in [-1.0e9, -3.5, 0.0, 0.25, 86_400.0, 3.2e9] {
            let e = base.add_seconds(dt).unwrap();
            let s = e.to_calendar(scale).unwrap().to_string();
            let back = p(&s);
            let err = back.seconds_since(e).abs();
            // TDB inversion is iterative; allow a nanosecond.
            assert!(err <= 1e-9, "{scale} {s}: {err}");
        }
    }
}

#[test]
fn tdb_seconds() {
    let e = p("2026-11-19T17:00:00Z");
    let et = e.tdb_seconds_since_j2000();
    assert!((et - e.tt_seconds_since_j2000()).abs() < 1.7e-3);
    let back = Epoch::from_tdb_seconds_since_j2000(et).unwrap();
    // f64 seconds near 8.5e8 resolve ~0.1 µs.
    assert!(back.seconds_since(e).abs() <= 1e-6);
}

#[test]
fn non_finite_rejected() {
    assert_eq!(
        Epoch::from_tt_seconds_since_j2000(f64::NAN),
        Err(Error::NonFinite)
    );
    assert_eq!(
        Epoch::J2000.add_seconds(f64::INFINITY),
        Err(Error::NonFinite)
    );
}

#[test]
fn negative_and_ancient_dates() {
    let e = p("0001-01-01T00:00:00 TT");
    assert_eq!(e.to_string(), "0001-01-01T00:00:00 TT");
    let e = Epoch::J2000.add_seconds(-0.5).unwrap();
    assert_eq!(e.to_string(), "2000-01-01T11:59:59.5 TT");
}

#[test]
fn tai_minus_utc() {
    assert_eq!(p("2016-12-31T23:59:59Z").tai_minus_utc_seconds(), Ok(36));
    assert_eq!(p("2016-12-31T23:59:60.5Z").tai_minus_utc_seconds(), Ok(36));
    assert_eq!(p("2017-01-01T00:00:00Z").tai_minus_utc_seconds(), Ok(37));
    assert_eq!(p("1972-01-01T00:00:00Z").tai_minus_utc_seconds(), Ok(10));
    assert_eq!(
        p("1971-12-31T23:59:59 TAI").tai_minus_utc_seconds(),
        Err(Error::UtcOutOfRange)
    );
}

#[test]
fn precise_tdb_intervals() {
    let a = p("2026-11-19T00:00:00Z");
    let b = a.add_nanoseconds(1);
    // One nanosecond of TT is one nanosecond of TDB to within 1e-17 s.
    assert!((b.tdb_seconds_since(a) - 1e-9).abs() < 1e-17);
    // Over half a year, TDB − TT changes by up to ~3.3 ms.
    let c = a.add_seconds(182.6 * 86400.0).unwrap();
    let d = c.tdb_seconds_since(a) - c.seconds_since(a);
    assert!(d.abs() < 3.4e-3 && d.abs() > 1e-4, "{d}");
    assert!(a.tdb_minus_tt_seconds().abs() < 1.7e-3);
    // add_tdb_seconds inverts tdb_seconds_since.
    for s in [-1e7, -716.9, 0.0, 1e-3, 3600.0, 2e8] {
        let e = a.add_tdb_seconds(s).unwrap();
        assert!((e.tdb_seconds_since(a) - s).abs() < 1e-9, "{s}");
    }
}
