# Concepts

The central abstraction is:

> A physical state expressed relative to a reference frame at a specified epoch.

## Identity

A `LocationId` is an opaque, immutable identifier for a facility, spacecraft,
rack or any other object. It never encodes a position. Human-readable aliases
such as `dc:lhr:01` live in a registry (planned for `heavenly::id`). You can
rename an alias without changing the object's identity.

## Epoch

Every dynamic quantity is evaluated at an explicit `Epoch`. Two states are
comparable only if their epochs are equal; anything else is an
`Error::EpochMismatch`.

## Reference frame

A frame defines an **origin**, a set of **axes**, and how both evolve with
time. `GCRF` (Earth-centred, ICRF axes) and `ITRF` (Earth-centred, rotating
with the Earth) share an origin but are different frames.

## State

A `StateVector` holds a position (m) and velocity (m/s) relative to a frame at
an epoch. `Coordinate<F>` holds the same data with the frame in the type
system.

## Locator

A `Locator` answers one question: *where is this object, in frame X, at time
T?* One object might answer from fixed coordinates, another from an orbit
propagator or an ephemeris. Callers use the same call either way.

## Placement topology

Operational regions and failure domains are **separate metadata**, planned for
`heavenly::topology`. Two physically distant nodes can share a failure
dependency, and one logical region may span several physical sites, so a
coordinate type should not try to encode either.
