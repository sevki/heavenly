//! Questions about many objects at once: which ground sites can reach which
//! satellites, which satellites can see each other, and which come close.
//!
//! The functions take positions through two closures: `all(t)` for every
//! object at once (a snapshot, which is where batching pays), and
//! `one(t, i)` for a single object, used when refining an event's time.
//! `heavenly_orbit::Catalog::positions_at` and `position_of` fit them
//! directly, but any source of positions works. `None` marks an object with
//! no position at that time (for example a decayed satellite).

use alloc::vec;
use alloc::vec::Vec;

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Transform, Vec3};

use crate::{Occulter, Window};

/// An object above a site's elevation mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Index into the `sites` slice.
    pub site: usize,
    /// Index of the object.
    pub object: usize,
    pub window: Window,
    /// The highest elevation among the samples and the edges.
    pub max_elevation_rad: f64,
}

/// Validates the time grid shared by the searches.
fn grid(start: Epoch, end: Epoch, step_s: f64, tolerance_s: f64) -> Result<Vec<Epoch>, Error> {
    if step_s.is_nan() || step_s <= 0.0 || tolerance_s.is_nan() || tolerance_s <= 0.0 {
        return Err(Error::InvalidInput("step and tolerance must be positive"));
    }
    let span = end.seconds_since(start);
    if span < 0.0 {
        return Err(Error::InvalidInput("end is before start"));
    }
    let n = libm::ceil(span / step_s) as usize;
    let mut out = Vec::with_capacity(n + 1);
    for k in 0..n {
        out.push(start.add_seconds(k as f64 * step_s)?);
    }
    out.push(end);
    Ok(out)
}

fn elevation(t: &Transform, p: Vec3) -> f64 {
    let q = t.apply_position(p);
    libm::asin((q.z / q.norm()).clamp(-1.0, 1.0))
}

/// Finds every interval in which an object is above `mask_rad` from a site.
///
/// `sites` are east-north-up frames fixed in `body_fixed` (as made by
/// `Geodetic::enu_to_body_fixed` or a geodetic registry mount); positions
/// come in `body_fixed`. Each site's transform is resolved once and applied
/// to all objects as a batch. The search samples every `step_s`, which must
/// be shorter than the shortest contact of interest, and refines each edge
/// to `tolerance_s` by bisection.
#[allow(clippy::too_many_arguments)]
pub fn contacts<R, E>(
    sites: &[FrameId],
    body_fixed: FrameId,
    resolver: &R,
    start: Epoch,
    end: Epoch,
    step_s: f64,
    tolerance_s: f64,
    mask_rad: f64,
    mut all: impl FnMut(Epoch) -> Result<Vec<Option<Vec3>>, E>,
    mut one: impl FnMut(Epoch, usize) -> Result<Option<Vec3>, E>,
) -> Result<Vec<Contact>, E>
where
    R: FrameResolver + ?Sized,
    E: From<Error> + From<R::Error>,
{
    let times = grid(start, end, step_s, tolerance_s)?;
    let transforms = sites
        .iter()
        .map(|s| {
            resolver
                .transform(body_fixed, *s, start)
                .map(|t| t.transform)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    // Per (site, object): when the current contact opened, and its highest
    // elevation so far.
    let mut open: Vec<Vec<Option<(Epoch, f64)>>> = Vec::new();
    let mut prev: Vec<Vec<f64>> = Vec::new();
    let mut batch: Vec<Vec3> = Vec::new();
    for (k, &t) in times.iter().enumerate() {
        let pos = all(t)?;
        if k == 0 {
            open = vec![vec![None; pos.len()]; sites.len()];
            prev = vec![vec![f64::NEG_INFINITY; pos.len()]; sites.len()];
        } else if pos.len() != prev[0].len() {
            return Err(
                Error::InvalidInput("the number of objects changed between samples").into(),
            );
        }
        for (s, tr) in transforms.iter().enumerate() {
            // Elevations of every object from this site, in one batch.
            batch.clear();
            batch.extend(pos.iter().map(|p| p.unwrap_or(Vec3::ZERO)));
            tr.apply_positions(&mut batch);
            for (o, q) in batch.iter().enumerate() {
                let el = if pos[o].is_some() {
                    libm::asin((q.z / q.norm()).clamp(-1.0, 1.0))
                } else {
                    f64::NEG_INFINITY
                };
                let (was_up, is_up) = (prev[s][o] > mask_rad, el > mask_rad);
                if k == 0 && is_up {
                    open[s][o] = Some((t, el));
                } else if was_up != is_up {
                    // Refine the edge between the two samples.
                    let mut f = |x: Epoch| -> Result<bool, E> {
                        Ok(one(x, o)?.is_some_and(|p| elevation(tr, p) > mask_rad))
                    };
                    let (mut lo, mut hi) = (times[k - 1], t);
                    while hi.seconds_since(lo) > tolerance_s {
                        let mid = lo.add_seconds(hi.seconds_since(lo) / 2.0)?;
                        if f(mid)? == was_up {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let edge = lo.add_seconds(hi.seconds_since(lo) / 2.0)?;
                    if is_up {
                        open[s][o] = Some((edge, el));
                    } else if let Some((a, top)) = open[s][o].take() {
                        out.push(Contact {
                            site: s,
                            object: o,
                            window: Window {
                                start: a,
                                end: edge,
                            },
                            max_elevation_rad: top,
                        });
                    }
                }
                if let Some((_, top)) = &mut open[s][o] {
                    *top = top.max(el);
                }
                prev[s][o] = el;
            }
        }
    }
    for (s, row) in open.into_iter().enumerate() {
        for (o, w) in row.into_iter().enumerate() {
            if let Some((a, top)) = w {
                out.push(Contact {
                    site: s,
                    object: o,
                    window: Window { start: a, end },
                    max_elevation_rad: top,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.window
            .start
            .cmp(&b.window.start)
            .then(a.site.cmp(&b.site))
            .then(a.object.cmp(&b.object))
    });
    Ok(out)
}

/// A line of sight between two objects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub range_m: f64,
    /// How far the line clears the body ([`Occulter::clearance_m`]).
    pub clearance_m: f64,
}

/// Every pair of objects within `max_range_m` whose line of sight clears
/// `body` by more than `min_clearance_m` (for example 80 km, to keep a
/// laser link above the atmosphere). Positions are in `body.body_fixed`.
/// This checks all pairs, so it takes time proportional to the square of
/// the number of objects.
pub fn links(
    positions: &[Option<Vec3>],
    body: &Occulter,
    max_range_m: f64,
    min_clearance_m: f64,
) -> Vec<Link> {
    let mut out = Vec::new();
    for (a, pa) in positions.iter().enumerate() {
        let Some(pa) = pa else { continue };
        for (b, pb) in positions.iter().enumerate().skip(a + 1) {
            let Some(pb) = pb else { continue };
            let range_m = (*pb - *pa).norm();
            if range_m > max_range_m {
                continue;
            }
            let clearance_m = body.clearance_m(*pa, *pb);
            if clearance_m > min_clearance_m {
                out.push(Link {
                    a,
                    b,
                    range_m,
                    clearance_m,
                });
            }
        }
    }
    out
}

/// A close approach between two objects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conjunction {
    pub a: usize,
    pub b: usize,
    /// The time of closest approach.
    pub epoch: Epoch,
    pub miss_distance_m: f64,
}

/// Finds the close approaches, closer than `threshold_m`, between every pair
/// of objects from `start` to `end`.
///
/// Positions must all be in one frame (any frame: distances do not depend
/// on it). Each pair's distance is sampled every `step_s`. Each local
/// minimum that could hide an approach closer than the threshold is refined
/// to `tolerance_s` by golden-section search. A sample can sit up to (the
/// pair's relative speed × `step_s`) above the true minimum, so the guard
/// uses each pair's own measured relative speed. A step of a minute is
/// typical for low orbits.
///
/// Every pair is checked, so the work grows with the square of the number
/// of objects.
#[allow(clippy::too_many_arguments)]
pub fn conjunctions<E>(
    start: Epoch,
    end: Epoch,
    step_s: f64,
    tolerance_s: f64,
    threshold_m: f64,
    mut all: impl FnMut(Epoch) -> Result<Vec<Option<Vec3>>, E>,
    mut one: impl FnMut(Epoch, usize) -> Result<Option<Vec3>, E>,
) -> Result<Vec<Conjunction>, E>
where
    E: From<Error>,
{
    let times = grid(start, end, step_s, tolerance_s)?;
    let mut snaps: Vec<Vec<Option<Vec3>>> = Vec::with_capacity(3);
    let mut out = Vec::new();
    for (k, &t) in times.iter().enumerate() {
        snaps.push(all(t)?);
        if snaps.len() > 3 {
            snaps.remove(0);
        }
        if snaps.iter().any(|s| s.len() != snaps[0].len()) {
            return Err(
                Error::InvalidInput("the number of objects changed between samples").into(),
            );
        }
        let last = k + 1 == times.len();
        // A minimum at the middle sample of three, or at either end of the
        // span.
        let windows: &[(usize, usize, usize)] = match (snaps.len(), last) {
            (2, false) => &[(0, 0, 1)],
            (2, true) => &[(0, 0, 1), (0, 1, 1)],
            (3, false) => &[(0, 1, 2)],
            (3, true) => &[(0, 1, 2), (1, 2, 2)],
            _ => &[],
        };
        let n = snaps[0].len();
        for &(i, m, j) in windows {
            for a in 0..n {
                for b in a + 1..n {
                    let d = |s: &Vec<Option<Vec3>>| match (s[a], s[b]) {
                        (Some(pa), Some(pb)) => Some(pb - pa),
                        _ => None,
                    };
                    let (Some(di), Some(dm), Some(dj)) = (d(&snaps[i]), d(&snaps[m]), d(&snaps[j]))
                    else {
                        continue;
                    };
                    let (ni, nm, nj) = (di.norm(), dm.norm(), dj.norm());
                    if !(nm <= ni && nm <= nj) {
                        continue;
                    }
                    let (ti, tj) = (
                        times[k + i + 1 - snaps.len()],
                        times[k + j + 1 - snaps.len()],
                    );
                    let span = tj.seconds_since(ti).max(f64::MIN_POSITIVE);
                    let speed = ((dj - di).norm() / span).max((dm - di).norm() / span.max(step_s));
                    if nm > threshold_m + speed * step_s {
                        continue;
                    }
                    // Golden-section search for the minimum on [ti, tj].
                    let mut dist = |x: Epoch| -> Result<f64, E> {
                        Ok(match (one(x, a)?, one(x, b)?) {
                            (Some(pa), Some(pb)) => (pb - pa).norm(),
                            _ => f64::INFINITY,
                        })
                    };
                    let g = 0.618_033_988_749_894_9;
                    let (mut lo, mut hi) = (0.0, span);
                    let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
                    let (mut f1, mut f2) = (dist(ti.add_seconds(x1)?)?, dist(ti.add_seconds(x2)?)?);
                    while hi - lo > tolerance_s {
                        if f1 < f2 {
                            hi = x2;
                            (x2, f2) = (x1, f1);
                            x1 = hi - g * (hi - lo);
                            f1 = dist(ti.add_seconds(x1)?)?;
                        } else {
                            lo = x1;
                            (x1, f1) = (x2, f2);
                            x2 = lo + g * (hi - lo);
                            f2 = dist(ti.add_seconds(x2)?)?;
                        }
                    }
                    let x = (lo + hi) / 2.0;
                    let at = ti.add_seconds(x)?;
                    let miss = dist(at)?;
                    // The same minimum can be bracketed twice at the ends.
                    let dup = out.iter().any(|c: &Conjunction| {
                        c.a == a
                            && c.b == b
                            && c.epoch.seconds_since(at).abs() < 2.0 * tolerance_s.max(1e-3)
                    });
                    if miss < threshold_m && !dup {
                        out.push(Conjunction {
                            a,
                            b,
                            epoch: at,
                            miss_distance_m: miss,
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|c| c.epoch);
    Ok(out)
}
