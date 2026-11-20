//! A heap-backed [`FrameResolver`] over a graph of known transformations.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::fmt;

use crate::{Epoch, Error, FrameId, FrameResolver, FrameTransform, Transform};

type EdgeFn = dyn Fn(Epoch) -> Result<Transform, Error> + Send + Sync;

struct Edge {
    child: FrameId,
    parent: FrameId,
    eval: Box<EdgeFn>,
}

/// Frames as nodes; known (possibly time-dependent) transforms as edges.
///
/// Each edge is registered as *child → parent* (it maps child coordinates to
/// parent coordinates) and may be traversed in either direction. A frame may
/// have several parents; the resolver uses the path with the fewest edges.
///
/// The graph does not assume every frame has a unique parent, nor that every
/// edge can be evaluated at every epoch: edge closures report
/// [`Error::OutOfRange`] (or any other error) and resolution fails with it.
#[derive(Default)]
pub struct FrameGraph {
    edges: Vec<Edge>,
}

impl fmt::Debug for FrameGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.edges.iter().map(|e| (e.child, e.parent)))
            .finish()
    }
}

impl FrameGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a time-dependent edge. `eval(epoch)` returns the transform
    /// from `child` to `parent` at `epoch`.
    pub fn add_edge<F>(&mut self, child: FrameId, parent: FrameId, eval: F) -> Result<(), Error>
    where
        F: Fn(Epoch) -> Result<Transform, Error> + Send + Sync + 'static,
    {
        if child == parent
            || self.edges.iter().any(|e| {
                (e.child == child && e.parent == parent) || (e.child == parent && e.parent == child)
            })
        {
            return Err(Error::DuplicateEdge {
                a: child,
                b: parent,
            });
        }
        self.edges.push(Edge {
            child,
            parent,
            eval: Box::new(eval),
        });
        Ok(())
    }

    /// Registers an edge whose transform does not change with time.
    pub fn add_fixed(
        &mut self,
        child: FrameId,
        parent: FrameId,
        transform: Transform,
    ) -> Result<(), Error> {
        self.add_edge(child, parent, move |_| Ok(transform))
    }

    /// Removes the edge between `a` and `b`, whichever way it was registered.
    /// Returns whether there was one. With [`add_fixed`](Self::add_fixed),
    /// this moves a frame, such as a ground station the user relocates.
    pub fn remove_edge(&mut self, a: FrameId, b: FrameId) -> bool {
        let before = self.edges.len();
        self.edges
            .retain(|e| !((e.child == a && e.parent == b) || (e.child == b && e.parent == a)));
        self.edges.len() != before
    }

    /// Returns `(edge index, traversed child→parent)` steps from `source` to
    /// `destination`, by breadth-first search.
    fn path(&self, source: FrameId, destination: FrameId) -> Option<Vec<(usize, bool)>> {
        let mut prev: BTreeMap<FrameId, (FrameId, usize, bool)> = BTreeMap::new();
        let mut queue = VecDeque::from([source]);
        while let Some(node) = queue.pop_front() {
            if node == destination {
                let mut steps = Vec::new();
                let mut at = destination;
                while at != source {
                    let (from, idx, fwd) = prev[&at];
                    steps.push((idx, fwd));
                    at = from;
                }
                steps.reverse();
                return Some(steps);
            }
            for (idx, e) in self.edges.iter().enumerate() {
                let (next, fwd) = if e.child == node {
                    (e.parent, true)
                } else if e.parent == node {
                    (e.child, false)
                } else {
                    continue;
                };
                if next != source && !prev.contains_key(&next) {
                    prev.insert(next, (node, idx, fwd));
                    queue.push_back(next);
                }
            }
        }
        None
    }
}

impl FrameResolver for FrameGraph {
    type Error = Error;

    fn transform(
        &self,
        source: FrameId,
        destination: FrameId,
        epoch: Epoch,
    ) -> Result<FrameTransform, Error> {
        let steps = self.path(source, destination).ok_or(Error::NoPath {
            source,
            destination,
        })?;
        let mut acc = Transform::IDENTITY;
        for (idx, forward) in steps {
            let edge = &self.edges[idx];
            let t = (edge.eval)(epoch)?;
            let step = if forward { t } else { t.inverse() };
            acc = acc.then(&step);
        }
        Ok(FrameTransform {
            source,
            destination,
            epoch,
            transform: acc,
        })
    }
}
