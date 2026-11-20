use std::collections::BTreeSet;

use heavenly_topology::{DomainKind, LocationId, Topology, TopologyError};

fn set(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// A small estate: two London zones on one grid, a lunar site, and a
/// satellite constellation that depends on the UK ground segment.
fn estate() -> (Topology, [LocationId; 4]) {
    let mut t = Topology::new();
    for (id, kind, name) in [
        ("region:europe", DomainKind::Region, "Europe"),
        ("zone:lhr-a", DomainKind::Zone, "LHR zone A"),
        ("zone:lhr-b", DomainKind::Zone, "LHR zone B"),
        ("grid:uk-south", DomainKind::Power, "UK south grid"),
        ("net:ground-uk", DomainKind::Network, "UK ground segment"),
        (
            "constellation:example",
            DomainKind::Constellation,
            "Example constellation",
        ),
        ("site:shackleton", DomainKind::Zone, "Shackleton site"),
        (
            "power:shackleton-solar",
            DomainKind::Power,
            "Shackleton solar array",
        ),
        ("law:gb", DomainKind::Jurisdiction, "United Kingdom"),
        ("law:ost", DomainKind::Jurisdiction, "Outer Space Treaty"),
    ] {
        t.add_domain(id, kind, name).unwrap();
    }
    for (a, b) in [
        ("zone:lhr-a", "grid:uk-south"),
        ("zone:lhr-b", "grid:uk-south"),
        ("zone:lhr-a", "region:europe"),
        ("zone:lhr-b", "region:europe"),
        ("net:ground-uk", "grid:uk-south"),
        ("constellation:example", "net:ground-uk"),
        ("site:shackleton", "power:shackleton-solar"),
        ("zone:lhr-a", "law:gb"),
        ("zone:lhr-b", "law:gb"),
        ("constellation:example", "law:ost"),
        ("site:shackleton", "law:ost"),
    ] {
        t.depends_on(a, b).unwrap();
    }
    let ids = [LocationId(1), LocationId(2), LocationId(3), LocationId(4)];
    let [rack_a, rack_b, sat, lunar] = ids;
    t.assign(rack_a, "zone:lhr-a").unwrap();
    t.assign(rack_b, "zone:lhr-b").unwrap();
    t.assign(sat, "constellation:example").unwrap();
    t.assign(lunar, "site:shackleton").unwrap();
    (t, ids)
}

#[test]
fn failure_domains_are_transitive() {
    let (t, [rack_a, _, sat, _]) = estate();
    assert_eq!(
        t.failure_domains(rack_a),
        set(&["grid:uk-south", "law:gb", "region:europe", "zone:lhr-a"])
    );
    assert_eq!(
        t.failure_domains(sat),
        set(&[
            "constellation:example",
            "grid:uk-south",
            "law:ost",
            "net:ground-uk"
        ])
    );
    assert!(t.failure_domains(LocationId(99)).is_empty());
}

#[test]
fn distant_nodes_can_share_a_fate() {
    let (t, [rack_a, rack_b, sat, lunar]) = estate();
    // Two zones in one building still share a grid.
    assert_eq!(
        t.shared_failure_domains(rack_a, rack_b),
        set(&["grid:uk-south", "law:gb", "region:europe"])
    );
    // A rack in London and a satellite in orbit share the grid through the
    // ground segment.
    assert_eq!(
        t.shared_failure_domains(rack_a, sat),
        set(&["grid:uk-south"])
    );
    assert!(!t.independent(rack_a, sat, &[]));
    // The lunar site shares only a legal regime with the satellite.
    assert_eq!(t.shared_failure_domains(sat, lunar), set(&["law:ost"]));
    assert!(!t.independent(sat, lunar, &[]));
    assert!(t.independent(sat, lunar, &[DomainKind::Jurisdiction]));
    assert!(t.independent(rack_a, lunar, &[]));
}

#[test]
fn blast_radius_and_jurisdictions() {
    let (t, [rack_a, rack_b, sat, lunar]) = estate();
    assert_eq!(
        t.blast_radius("grid:uk-south").unwrap(),
        [rack_a, rack_b, sat].into_iter().collect()
    );
    assert_eq!(
        t.blast_radius("power:shackleton-solar").unwrap(),
        [lunar].into_iter().collect()
    );
    assert_eq!(
        t.blast_radius("zone:lhr-a").unwrap(),
        [rack_a].into_iter().collect()
    );
    assert_eq!(
        t.domains_of_kind(rack_a, &DomainKind::Jurisdiction),
        set(&["law:gb"])
    );
    assert_eq!(
        t.domains_of_kind(lunar, &DomainKind::Jurisdiction),
        set(&["law:ost"])
    );
    assert_eq!(
        t.blast_radius("nope"),
        Err(TopologyError::UnknownDomain("nope".into()))
    );
}

#[test]
fn errors() {
    let (mut t, [rack_a, ..]) = estate();
    assert_eq!(
        t.depends_on("grid:uk-south", "constellation:example"),
        Err(TopologyError::Cycle {
            from: "grid:uk-south".into(),
            to: "constellation:example".into()
        })
    );
    assert!(matches!(
        t.depends_on("grid:uk-south", "grid:uk-south"),
        Err(TopologyError::Cycle { .. })
    ));
    assert_eq!(
        t.add_domain("law:gb", DomainKind::Jurisdiction, "again"),
        Err(TopologyError::DuplicateDomain("law:gb".into()))
    );
    assert!(matches!(
        t.add_domain("Bad Id", DomainKind::Zone, "x"),
        Err(TopologyError::InvalidDomainId { .. })
    ));
    assert!(matches!(
        t.assign(rack_a, "zone:nowhere"),
        Err(TopologyError::UnknownDomain(_))
    ));
    assert!(t.unassign(rack_a, "zone:lhr-a"));
    assert!(!t.unassign(rack_a, "zone:lhr-a"));
    assert!(t.failure_domains(rack_a).is_empty());
}
