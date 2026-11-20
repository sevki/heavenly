//! Placement topology: failure domains, jurisdictions and their
//! dependencies, kept separate from coordinates.
//!
//! [Book](https://sevki.github.io/heavenly/) · [Playground](https://sevki.github.io/heavenly/playground.html) · [API docs](https://sevki.github.io/heavenly/api/heavenly_topology/)
//!
//! **Placement topology is policy.** Where something *is* is a matter for
//! frames and locators. What it *shares a fate with*, and whose rules
//! apply to it, is declared here:
//!
//! * a [`Domain`] is anything whose failure or authority spans locations:
//!   an availability zone, a power feed, a network, a launch provider, a
//!   ground segment, a jurisdiction;
//! * locations (by [`LocationId`]) are *assigned* to domains;
//! * domains *depend on* other domains (a zone on a grid, a satellite
//!   constellation on its ground segment), forming a directed acyclic graph.
//!
//! Nothing here is derived from position. Two physically distant nodes can
//! share a failure domain, and one region can span many sites. Jurisdiction
//! is an assignment you make, not a function of coordinates.
//!
//! # Example
//!
//! ```
//! use heavenly_core::LocationId;
//! use heavenly_topology::{DomainKind, Topology};
//!
//! # fn main() -> Result<(), heavenly_topology::TopologyError> {
//! let mut t = Topology::new();
//! t.add_domain("grid:uk-south", DomainKind::Power, "UK south grid")?;
//! t.add_domain("net:ground-uk", DomainKind::Network, "UK ground segment")?;
//! t.add_domain("constellation:example", DomainKind::Constellation, "Example constellation")?;
//! t.depends_on("net:ground-uk", "grid:uk-south")?;
//! t.depends_on("constellation:example", "net:ground-uk")?;
//!
//! let (rack, sat) = (LocationId(1), LocationId(2));
//! t.assign(rack, "grid:uk-south")?;
//! t.assign(sat, "constellation:example")?;
//!
//! // 7 000 km apart, but one grid failure takes out both.
//! assert!(t.shared_failure_domains(rack, sat).contains("grid:uk-south"));
//! # Ok(()) }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

pub use heavenly_core::LocationId;

/// What a domain represents.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainKind {
    /// A scheduling or marketing region (e.g. a cloud region).
    Region,
    /// An availability zone or other isolated failure unit.
    Zone,
    /// A legal jurisdiction or regulatory regime.
    Jurisdiction,
    Power,
    Network,
    Cooling,
    /// A provider or operator whose outage or policy affects members.
    Provider,
    /// A satellite constellation or fleet.
    Constellation,
    Other(String),
}

/// A named failure or authority domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Domain {
    pub id: String,
    pub kind: DomainKind,
    pub name: String,
}

/// Errors from building or querying a [`Topology`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TopologyError {
    InvalidDomainId {
        id: String,
        reason: &'static str,
    },
    DuplicateDomain(String),
    UnknownDomain(String),
    /// Adding `from → to` would make a domain depend on itself.
    Cycle {
        from: String,
        to: String,
    },
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopologyError::InvalidDomainId { id, reason } => {
                write!(f, "invalid domain id {id:?}: {reason}")
            }
            TopologyError::DuplicateDomain(id) => write!(f, "domain {id:?} already exists"),
            TopologyError::UnknownDomain(id) => write!(f, "no domain {id:?}"),
            TopologyError::Cycle { from, to } => {
                write!(f, "{from:?} depending on {to:?} would create a cycle")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TopologyError {}

/// Domains, their dependencies, and which locations belong to which.
#[derive(Clone, Debug, Default)]
pub struct Topology {
    domains: BTreeMap<String, Domain>,
    /// domain → the domains it depends on directly.
    depends: BTreeMap<String, BTreeSet<String>>,
    /// location → the domains it is assigned to directly.
    members: BTreeMap<LocationId, BTreeSet<String>>,
}

fn check_id(id: &str) -> Result<(), TopologyError> {
    let bad = |reason| TopologyError::InvalidDomainId {
        id: id.to_string(),
        reason,
    };
    if id.is_empty() || id.len() > 128 {
        return Err(bad("must be 1 to 128 characters"));
    }
    if !id
        .bytes()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_:./".contains(&c))
    {
        return Err(bad(
            "allowed characters are a-z, 0-9, '-', '_', ':', '.', '/'",
        ));
    }
    Ok(())
}

impl Topology {
    pub fn new() -> Topology {
        Topology::default()
    }

    pub fn add_domain(
        &mut self,
        id: &str,
        kind: DomainKind,
        name: &str,
    ) -> Result<(), TopologyError> {
        check_id(id)?;
        if self.domains.contains_key(id) {
            return Err(TopologyError::DuplicateDomain(id.into()));
        }
        self.domains.insert(
            id.into(),
            Domain {
                id: id.into(),
                kind,
                name: name.into(),
            },
        );
        Ok(())
    }

    pub fn domain(&self, id: &str) -> Result<&Domain, TopologyError> {
        self.domains
            .get(id)
            .ok_or_else(|| TopologyError::UnknownDomain(id.into()))
    }

    pub fn domains(&self) -> impl Iterator<Item = &Domain> {
        self.domains.values()
    }

    /// Declares that `domain` fails (or is governed) whenever `on` is.
    pub fn depends_on(&mut self, domain: &str, on: &str) -> Result<(), TopologyError> {
        self.domain(domain)?;
        self.domain(on)?;
        if domain == on || self.closure([on]).contains(domain) {
            return Err(TopologyError::Cycle {
                from: domain.into(),
                to: on.into(),
            });
        }
        self.depends
            .entry(domain.into())
            .or_default()
            .insert(on.into());
        Ok(())
    }

    /// Assigns a location to a domain (a location may be in many).
    pub fn assign(&mut self, location: LocationId, domain: &str) -> Result<(), TopologyError> {
        self.domain(domain)?;
        self.members
            .entry(location)
            .or_default()
            .insert(domain.into());
        Ok(())
    }

    /// Removes a location from a domain. Returns whether it was assigned.
    pub fn unassign(&mut self, location: LocationId, domain: &str) -> bool {
        self.members
            .get_mut(&location)
            .is_some_and(|s| s.remove(domain))
    }

    /// The domains reachable from `start` through dependencies, including
    /// `start` itself.
    fn closure<'a>(&self, start: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = start.into_iter().map(String::from).collect();
        while let Some(d) = stack.pop() {
            if seen.insert(d.clone()) {
                if let Some(next) = self.depends.get(&d) {
                    stack.extend(next.iter().cloned());
                }
            }
        }
        seen
    }

    /// Every domain whose failure affects `location`: its direct domains
    /// and everything they depend on, transitively.
    pub fn failure_domains(&self, location: LocationId) -> BTreeSet<String> {
        match self.members.get(&location) {
            Some(direct) => self.closure(direct.iter().map(String::as_str)),
            None => BTreeSet::new(),
        }
    }

    /// Failure domains of `location` of one kind (e.g. its jurisdictions).
    pub fn domains_of_kind(&self, location: LocationId, kind: &DomainKind) -> BTreeSet<String> {
        self.failure_domains(location)
            .into_iter()
            .filter(|d| self.domains.get(d).is_some_and(|x| &x.kind == kind))
            .collect()
    }

    /// Domains whose single failure affects both locations.
    pub fn shared_failure_domains(&self, a: LocationId, b: LocationId) -> BTreeSet<String> {
        let fb = self.failure_domains(b);
        self.failure_domains(a)
            .into_iter()
            .filter(|d| fb.contains(d))
            .collect()
    }

    /// Whether no single domain's failure affects both locations, ignoring
    /// domains of the kinds in `ignore` (e.g. ignore `Jurisdiction` when
    /// asking only about outages).
    pub fn independent(&self, a: LocationId, b: LocationId, ignore: &[DomainKind]) -> bool {
        self.shared_failure_domains(a, b).iter().all(|d| {
            self.domains
                .get(d)
                .is_some_and(|x| ignore.contains(&x.kind))
        })
    }

    /// Every location affected if `domain` fails.
    pub fn blast_radius(&self, domain: &str) -> Result<BTreeSet<LocationId>, TopologyError> {
        self.domain(domain)?;
        Ok(self
            .members
            .keys()
            .filter(|loc| self.failure_domains(**loc).contains(domain))
            .copied()
            .collect())
    }

    /// The locations assigned to any domain.
    pub fn locations(&self) -> impl Iterator<Item = LocationId> + '_ {
        self.members.keys().copied()
    }
}
