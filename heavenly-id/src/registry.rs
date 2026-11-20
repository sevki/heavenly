//! Registries: entities, their aliases, and the frames they define.
//!
//! [`Registry`] is the read side, as a trait, so the entities can live
//! wherever you keep them (a database, an inventory service, a config file).
//! Everything derived from them, such as frames, paths, validation, frame
//! graph edges and locators, comes with the trait, so every store behaves
//! the same. [`MemoryRegistry`] is the in-memory implementation, with the
//! write operations.

use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use heavenly_core::{
    Epoch, FixedLocator, FrameGraph, FrameId, LocationId, Locator, Namespace, StateVector, Vec3,
};

use crate::mount::platform_transform;
use crate::{Alias, IdError, Mount, Parent};

/// What kind of thing an entity is. Informational: it does not change how
/// the entity is located.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Site,
    Facility,
    Building,
    Floor,
    Room,
    Rack,
    Device,
    Spacecraft,
    Module,
    Antenna,
    Other(String),
}

/// A registered location: a stable identity, a name, and how its local
/// frame is mounted.
#[derive(Clone, Debug)]
pub struct Entity {
    pub id: LocationId,
    pub kind: Kind,
    pub name: String,
    pub mount: Mount,
}

impl Entity {
    pub fn new(id: LocationId, kind: Kind, name: impl Into<String>, mount: Mount) -> Entity {
        Entity {
            id,
            kind,
            name: name.into(),
            mount,
        }
    }
}

/// Something that names an entity: its ID or one of its aliases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    Id(LocationId),
    Alias(&'a str),
}

impl From<LocationId> for Key<'_> {
    fn from(id: LocationId) -> Self {
        Key::Id(id)
    }
}

impl<'a> From<&'a str> for Key<'a> {
    fn from(a: &'a str) -> Self {
        Key::Alias(a)
    }
}

/// The frame of an entity: in `namespace`, with a code derived from the ID.
///
/// Every [`Registry`] uses this, so the same ID always maps to the same
/// frame, whichever store holds it.
pub fn entity_frame(namespace: Namespace, id: LocationId) -> Option<FrameId> {
    FrameId::custom(namespace, frame_code(id))
}

/// Folds the 128-bit ID into a non-negative i64.
fn frame_code(id: LocationId) -> i64 {
    let folded = (id.0 as u64) ^ ((id.0 >> 64) as u64);
    (folded & (i64::MAX as u64)) as i64
}

/// Read access to a set of entities and their aliases.
///
/// Implement the four required methods over your own store; the rest
/// (resolving keys, frames, paths, validation, frame-graph edges, locators)
/// is provided, so it is the same for every store. [`MemoryRegistry`] is
/// the in-memory implementation.
///
/// Every entity with a [`Mount`] other than [`Mount::None`] has a frame,
/// [`Registry::frame_of`], in the registry's namespace; see
/// [`entity_frame`]. [`Registry::register_frames`] adds one frame-graph edge
/// per entity, after which any entity can be located in any connected frame.
///
/// # Implementing
///
/// ```
/// use std::borrow::Cow;
/// use std::collections::HashMap;
/// use heavenly_core::Namespace;
/// use heavenly_id::{Alias, Entity, IdError, LocationId, Registry};
///
/// /// Entities loaded from somewhere else, read-only.
/// struct Snapshot {
///     entities: HashMap<LocationId, Entity>,
///     aliases: HashMap<String, LocationId>,
/// }
///
/// impl Registry for Snapshot {
///     type Error = IdError;
///     fn namespace(&self) -> Namespace {
///         Namespace(7)
///     }
///     fn entity(&self, id: LocationId) -> Result<Option<Cow<'_, Entity>>, IdError> {
///         Ok(self.entities.get(&id).map(Cow::Borrowed))
///     }
///     fn lookup_alias(&self, alias: &Alias) -> Result<Option<LocationId>, IdError> {
///         Ok(self.aliases.get(alias.as_str()).copied())
///     }
///     fn for_each_entity(
///         &self,
///         f: &mut dyn FnMut(&Entity) -> Result<(), IdError>,
///     ) -> Result<(), IdError> {
///         self.entities.values().try_for_each(f)
///     }
/// }
/// ```
pub trait Registry {
    /// The store's error type. It must be able to carry registry errors
    /// (unknown IDs, cycles, …); a store that cannot fail can use
    /// [`IdError`] itself.
    type Error: From<IdError>;

    /// The namespace of the entity frames. It must not be
    /// [`Namespace::HEAVENLY`].
    fn namespace(&self) -> Namespace;

    /// The entity with this ID, if there is one.
    fn entity(&self, id: LocationId) -> Result<Option<Cow<'_, Entity>>, Self::Error>;

    /// The ID an alias names, if any.
    fn lookup_alias(&self, alias: &Alias) -> Result<Option<LocationId>, Self::Error>;

    /// Calls `f` for every entity, stopping at the first error.
    fn for_each_entity(
        &self,
        f: &mut dyn FnMut(&Entity) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;

    /// The ID named by an ID or alias.
    fn id_of<'a>(&self, key: impl Into<Key<'a>>) -> Result<LocationId, Self::Error> {
        match key.into() {
            Key::Id(id) => match self.entity(id)? {
                Some(_) => Ok(id),
                None => Err(IdError::UnknownId(id).into()),
            },
            Key::Alias(a) => self
                .lookup_alias(&Alias::new(a)?)?
                .ok_or_else(|| IdError::UnknownAlias(a.into()).into()),
        }
    }

    /// Looks up an entity by ID or alias.
    fn resolve<'a>(&self, key: impl Into<Key<'a>>) -> Result<Cow<'_, Entity>, Self::Error> {
        let id = self.id_of(key)?;
        self.entity(id)?
            .ok_or_else(|| IdError::UnknownId(id).into())
    }

    /// The local frame of a locatable entity.
    fn frame_of<'a>(&self, key: impl Into<Key<'a>>) -> Result<FrameId, Self::Error> {
        let e = self.resolve(key)?;
        if matches!(e.mount, Mount::None) {
            return Err(IdError::NotLocatable(e.id).into());
        }
        entity_frame(self.namespace(), e.id).ok_or_else(|| {
            IdError::Core(heavenly_core::Error::InvalidInput(
                "the HEAVENLY namespace is reserved",
            ))
            .into()
        })
    }

    /// Entities mounted directly in `id`.
    fn children(&self, id: LocationId) -> Result<Vec<Entity>, Self::Error> {
        let mut out = Vec::new();
        self.for_each_entity(&mut |e| {
            if e.mount.parent_entity() == Some(id) {
                out.push(e.clone());
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// The chain of entities from the outermost ancestor down to `id`.
    fn path(&self, id: LocationId) -> Result<Vec<Cow<'_, Entity>>, Self::Error> {
        let mut chain: Vec<Cow<'_, Entity>> = Vec::new();
        let mut current = id;
        loop {
            if chain.iter().any(|c| c.id == current) {
                return Err(IdError::Cycle(current).into());
            }
            let e = self.entity(current)?.ok_or(IdError::UnknownId(current))?;
            let parent = e.mount.parent_entity();
            chain.push(e);
            match parent {
                Some(p) if self.entity(p)?.is_some() => current = p,
                Some(p) => {
                    return Err(IdError::MissingParent {
                        entity: current,
                        parent: p,
                    }
                    .into())
                }
                None => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// Checks that every parent exists and is locatable, that no entity is
    /// mounted in itself (directly or indirectly), and that no two entities
    /// map to the same frame code.
    fn validate(&self) -> Result<(), Self::Error> {
        let mut ids = Vec::new();
        let mut codes: BTreeMap<i64, LocationId> = BTreeMap::new();
        self.for_each_entity(&mut |e| {
            ids.push(e.id);
            if !matches!(e.mount, Mount::None) {
                if let Some(other) = codes.insert(frame_code(e.id), e.id) {
                    return Err(IdError::FrameCollision { a: other, b: e.id }.into());
                }
            }
            Ok(())
        })?;
        let mut checked = BTreeSet::new();
        for id in ids {
            let e = self.entity(id)?.ok_or(IdError::UnknownId(id))?;
            if let Some(p) = e.mount.parent_entity() {
                match self.entity(p)? {
                    None => {
                        return Err(IdError::MissingParent {
                            entity: id,
                            parent: p,
                        }
                        .into())
                    }
                    Some(pe) if matches!(pe.mount, Mount::None) => {
                        return Err(IdError::NotLocatable(p).into())
                    }
                    Some(_) => {}
                }
            }
            if checked.insert(id) {
                for c in self.path(id)? {
                    checked.insert(c.id);
                }
            }
        }
        Ok(())
    }

    /// Adds one edge per locatable entity to `graph`: its frame → its
    /// parent's frame. Call again on a fresh graph after changing mounts.
    fn register_frames(&self, graph: &mut FrameGraph) -> Result<(), Self::Error> {
        self.validate()?;
        let mut ids = Vec::new();
        self.for_each_entity(&mut |e| {
            if !matches!(e.mount, Mount::None) {
                ids.push(e.id);
            }
            Ok(())
        })?;
        for id in ids {
            let e = self.entity(id)?.ok_or(IdError::UnknownId(id))?;
            let child = self.frame_of(id)?;
            let added = match &e.mount {
                Mount::None => Ok(()),
                Mount::Fixed { parent, transform } => {
                    let parent_frame = match parent {
                        Parent::Frame(f) => *f,
                        Parent::Entity(p) => self.frame_of(*p)?,
                    };
                    graph.add_fixed(child, parent_frame, *transform)
                }
                Mount::Geodetic {
                    body_fixed,
                    ellipsoid,
                    position,
                } => graph.add_fixed(child, *body_fixed, position.enu_to_body_fixed(ellipsoid)),
                Mount::Trajectory {
                    inertial,
                    trajectory,
                    attitude,
                } => {
                    let trajectory = Arc::clone(trajectory);
                    let attitude = *attitude;
                    graph.add_edge(child, *inertial, move |epoch| {
                        let (r, v) = trajectory(epoch)?;
                        platform_transform(r, v, attitude)
                    })
                }
            };
            added.map_err(IdError::from)?;
        }
        Ok(())
    }

    /// A [`Locator`] for the origin of an entity's frame.
    fn locator<'a>(&self, key: impl Into<Key<'a>>) -> Result<FixedLocator, Self::Error> {
        Ok(FixedLocator {
            frame: self.frame_of(key)?,
            position_m: Vec3::ZERO,
        })
    }

    /// The state of an entity's frame origin in `frame` at `epoch`, using a
    /// graph that [`Registry::register_frames`] has populated.
    fn locate<'a>(
        &self,
        key: impl Into<Key<'a>>,
        epoch: Epoch,
        frame: FrameId,
        graph: &FrameGraph,
    ) -> Result<StateVector, Self::Error> {
        let locator = self.locator(key)?;
        Ok(locator
            .state_at(epoch, frame, graph)
            .map_err(IdError::from)?)
    }
}

/// An in-memory [`Registry`], with the write operations: adding and
/// removing entities, and adding, moving, renaming and removing aliases.
///
/// Aliases are a separate, mutable mapping: renaming or re-pointing an alias
/// never changes an ID or a frame.
#[derive(Debug)]
pub struct MemoryRegistry {
    namespace: Namespace,
    entities: BTreeMap<LocationId, Entity>,
    aliases: BTreeMap<Alias, LocationId>,
    frames: BTreeMap<i64, LocationId>,
}

impl MemoryRegistry {
    /// A registry whose entity frames live in `namespace`, which must not be
    /// [`Namespace::HEAVENLY`].
    pub fn new(namespace: Namespace) -> Result<MemoryRegistry, IdError> {
        FrameId::custom(namespace, 0).ok_or(heavenly_core::Error::InvalidInput(
            "the HEAVENLY namespace is reserved",
        ))?;
        Ok(MemoryRegistry {
            namespace,
            entities: BTreeMap::new(),
            aliases: BTreeMap::new(),
            frames: BTreeMap::new(),
        })
    }

    /// Adds an entity. Its parent need not be registered yet; that is
    /// checked by [`Registry::validate`] and [`Registry::register_frames`].
    pub fn add(&mut self, entity: Entity) -> Result<LocationId, IdError> {
        let id = entity.id;
        if self.entities.contains_key(&id) {
            return Err(IdError::DuplicateId(id));
        }
        if !matches!(entity.mount, Mount::None) {
            let code = frame_code(id);
            if let Some(other) = self.frames.get(&code) {
                return Err(IdError::FrameCollision { a: *other, b: id });
            }
            self.frames.insert(code, id);
        }
        self.entities.insert(id, entity);
        Ok(id)
    }
    /// Removes an entity and its aliases. Entities mounted in it are left
    /// in place and will fail validation until re-mounted.
    pub fn remove(&mut self, id: LocationId) -> Result<Entity, IdError> {
        let e = self.entities.remove(&id).ok_or(IdError::UnknownId(id))?;
        self.aliases.retain(|_, v| *v != id);
        self.frames.retain(|_, v| *v != id);
        Ok(e)
    }

    /// Points `alias` at `id`, which must exist. Fails if the alias already
    /// names a different entity; use [`MemoryRegistry::repoint`] to move it.
    pub fn alias(&mut self, alias: &str, id: LocationId) -> Result<(), IdError> {
        let a = Alias::new(alias)?;
        if !self.entities.contains_key(&id) {
            return Err(IdError::UnknownId(id));
        }
        match self.aliases.get(&a) {
            Some(existing) if *existing != id => Err(IdError::DuplicateAlias {
                alias: a.as_str().into(),
                existing: *existing,
            }),
            _ => {
                self.aliases.insert(a, id);
                Ok(())
            }
        }
    }

    /// Moves an existing alias to a different entity, returning the entity
    /// it named before.
    pub fn repoint(&mut self, alias: &str, id: LocationId) -> Result<LocationId, IdError> {
        let a = Alias::new(alias)?;
        if !self.entities.contains_key(&id) {
            return Err(IdError::UnknownId(id));
        }
        let slot = self
            .aliases
            .get_mut(&a)
            .ok_or_else(|| IdError::UnknownAlias(alias.into()))?;
        Ok(core::mem::replace(slot, id))
    }

    /// Renames an alias, keeping what it points at.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), IdError> {
        let new = Alias::new(to)?;
        let old = Alias::new(from)?;
        if let Some(existing) = self.aliases.get(&new) {
            return Err(IdError::DuplicateAlias {
                alias: to.into(),
                existing: *existing,
            });
        }
        let id = self
            .aliases
            .remove(&old)
            .ok_or_else(|| IdError::UnknownAlias(from.into()))?;
        self.aliases.insert(new, id);
        Ok(())
    }

    /// Removes an alias.
    pub fn unalias(&mut self, alias: &str) -> Result<LocationId, IdError> {
        let a = Alias::new(alias)?;
        self.aliases
            .remove(&a)
            .ok_or_else(|| IdError::UnknownAlias(alias.into()))
    }

    /// The aliases of an entity.
    pub fn aliases_of(&self, id: LocationId) -> Vec<&Alias> {
        self.aliases
            .iter()
            .filter(|(_, v)| **v == id)
            .map(|(k, _)| k)
            .collect()
    }

    /// Every entity, in ID order.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }
}

impl Registry for MemoryRegistry {
    type Error = IdError;

    fn namespace(&self) -> Namespace {
        self.namespace
    }

    fn entity(&self, id: LocationId) -> Result<Option<Cow<'_, Entity>>, IdError> {
        Ok(self.entities.get(&id).map(Cow::Borrowed))
    }

    fn lookup_alias(&self, alias: &Alias) -> Result<Option<LocationId>, IdError> {
        Ok(self.aliases.get(alias).copied())
    }

    fn for_each_entity(
        &self,
        f: &mut dyn FnMut(&Entity) -> Result<(), IdError>,
    ) -> Result<(), IdError> {
        self.entities.values().try_for_each(f)
    }
}
