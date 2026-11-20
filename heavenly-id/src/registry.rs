//! The registry: entities, their aliases, and the frames they define.

use alloc::collections::BTreeMap;
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

/// Entities keyed by stable [`LocationId`], with aliases and a local frame
/// per locatable entity.
///
/// Every entity with a [`Mount`] other than [`Mount::None`] gets a frame,
/// [`Registry::frame_of`], in the registry's namespace. Its code is derived
/// from the ID, so the same ID always maps to the same frame.
/// [`Registry::register_frames`] adds one frame-graph edge per entity, after
/// which any entity can be located in any connected frame.
///
/// Aliases are a separate, mutable mapping: renaming or re-pointing an alias
/// never changes an ID or a frame.
#[derive(Debug)]
pub struct Registry {
    namespace: Namespace,
    entities: BTreeMap<LocationId, Entity>,
    aliases: BTreeMap<Alias, LocationId>,
    frames: BTreeMap<i64, LocationId>,
}

impl Registry {
    /// A registry whose entity frames live in `namespace`, which must not be
    /// [`Namespace::HEAVENLY`].
    pub fn new(namespace: Namespace) -> Result<Registry, IdError> {
        FrameId::custom(namespace, 0).ok_or(heavenly_core::Error::InvalidInput(
            "the HEAVENLY namespace is reserved",
        ))?;
        Ok(Registry {
            namespace,
            entities: BTreeMap::new(),
            aliases: BTreeMap::new(),
            frames: BTreeMap::new(),
        })
    }

    fn frame_code(id: LocationId) -> i64 {
        // Fold the 128-bit ID into a non-negative i64.
        let folded = (id.0 as u64) ^ ((id.0 >> 64) as u64);
        (folded & (i64::MAX as u64)) as i64
    }

    /// Adds an entity. Its parent need not be registered yet; that is
    /// checked by [`Registry::validate`] and [`Registry::register_frames`].
    pub fn add(&mut self, entity: Entity) -> Result<LocationId, IdError> {
        let id = entity.id;
        if self.entities.contains_key(&id) {
            return Err(IdError::DuplicateId(id));
        }
        if !matches!(entity.mount, Mount::None) {
            let code = Registry::frame_code(id);
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
    /// names a different entity; use [`Registry::repoint`] to move it.
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

    /// Looks up an entity by ID or alias.
    pub fn resolve<'a>(&self, key: impl Into<Key<'a>>) -> Result<&Entity, IdError> {
        let id = self.id_of(key)?;
        self.entities.get(&id).ok_or(IdError::UnknownId(id))
    }

    /// The ID named by an ID or alias.
    pub fn id_of<'a>(&self, key: impl Into<Key<'a>>) -> Result<LocationId, IdError> {
        match key.into() {
            Key::Id(id) if self.entities.contains_key(&id) => Ok(id),
            Key::Id(id) => Err(IdError::UnknownId(id)),
            Key::Alias(a) => {
                let alias = Alias::new(a)?;
                self.aliases
                    .get(&alias)
                    .copied()
                    .ok_or_else(|| IdError::UnknownAlias(a.into()))
            }
        }
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Entities mounted directly in `id`.
    pub fn children(&self, id: LocationId) -> Vec<&Entity> {
        self.entities
            .values()
            .filter(|e| e.mount.parent_entity() == Some(id))
            .collect()
    }

    /// The chain of entities from the outermost ancestor down to `id`.
    pub fn path(&self, id: LocationId) -> Result<Vec<&Entity>, IdError> {
        let mut chain = Vec::new();
        let mut current = id;
        loop {
            let e = self
                .entities
                .get(&current)
                .ok_or(IdError::UnknownId(current))?;
            if chain.iter().any(|c: &&Entity| c.id == current) {
                return Err(IdError::Cycle(current));
            }
            chain.push(e);
            match e.mount.parent_entity() {
                Some(p) if self.entities.contains_key(&p) => current = p,
                Some(p) => {
                    return Err(IdError::MissingParent {
                        entity: current,
                        parent: p,
                    })
                }
                None => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// The local frame of a locatable entity.
    pub fn frame_of<'a>(&self, key: impl Into<Key<'a>>) -> Result<FrameId, IdError> {
        let e = self.resolve(key)?;
        if matches!(e.mount, Mount::None) {
            return Err(IdError::NotLocatable(e.id));
        }
        Ok(FrameId::custom(self.namespace, Registry::frame_code(e.id))
            .expect("namespace checked in new"))
    }

    /// Checks that every parent exists and that no entity is mounted in
    /// itself, directly or indirectly.
    pub fn validate(&self) -> Result<(), IdError> {
        for e in self.entities.values() {
            if let Some(p) = e.mount.parent_entity() {
                match self.entities.get(&p) {
                    None => {
                        return Err(IdError::MissingParent {
                            entity: e.id,
                            parent: p,
                        })
                    }
                    Some(pe) if matches!(pe.mount, Mount::None) => {
                        return Err(IdError::NotLocatable(p))
                    }
                    Some(_) => {}
                }
            }
            self.path(e.id)?;
        }
        Ok(())
    }

    /// Adds one edge per locatable entity to `graph`: its frame → its
    /// parent's frame. Call again on a fresh graph after changing mounts.
    pub fn register_frames(&self, graph: &mut FrameGraph) -> Result<(), IdError> {
        self.validate()?;
        for e in self.entities.values() {
            let child = match self.frame_of(e.id) {
                Ok(f) => f,
                Err(IdError::NotLocatable(_)) => continue,
                Err(err) => return Err(err),
            };
            match &e.mount {
                Mount::None => {}
                Mount::Fixed { parent, transform } => {
                    let parent_frame = match parent {
                        Parent::Frame(f) => *f,
                        Parent::Entity(id) => self.frame_of(*id)?,
                    };
                    graph.add_fixed(child, parent_frame, *transform)?;
                }
                Mount::Geodetic {
                    body_fixed,
                    ellipsoid,
                    position,
                } => {
                    graph.add_fixed(child, *body_fixed, position.enu_to_body_fixed(ellipsoid))?;
                }
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
                    })?;
                }
            }
        }
        Ok(())
    }

    /// A [`Locator`] for the origin of an entity's frame.
    pub fn locator<'a>(&self, key: impl Into<Key<'a>>) -> Result<FixedLocator, IdError> {
        Ok(FixedLocator {
            frame: self.frame_of(key)?,
            position_m: Vec3::ZERO,
        })
    }

    /// The state of an entity's frame origin in `frame` at `epoch`, using a
    /// graph that [`Registry::register_frames`] has populated.
    pub fn locate<'a>(
        &self,
        key: impl Into<Key<'a>>,
        epoch: Epoch,
        frame: FrameId,
        graph: &FrameGraph,
    ) -> Result<StateVector, IdError> {
        Ok(self.locator(key)?.state_at(epoch, frame, graph)?)
    }
}
