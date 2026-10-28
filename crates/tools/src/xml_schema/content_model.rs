//! Content models, shared by DTDs and XML Schemas: particles, how often they occur, and whether
//! an element's children are what a model allows.

use std::collections::BTreeSet;

/// How many times a particle may occur; `max` `None` is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Occurs {
    pub(super) min: u32,
    pub(super) max: Option<u32>,
}

pub(super) const ONCE: Occurs = Occurs {
    min: 1,
    max: Some(1),
};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Particle {
    /// An element: in a DTD its name, in a schema its declaration.
    Element(usize),
    /// Any element (`ANY`, `xs:any`).
    Any,
    Sequence(Vec<(Particle, Occurs)>),
    Choice(Vec<(Particle, Occurs)>),
    /// Each at most once, in any order (`xs:all`).
    All(Vec<(Particle, Occurs)>),
}

/// The ends a particle can reach from each of `starts` over `children` (indices of names, as
/// `matches` tells which child a particle element takes).
pub(super) fn reach(
    particle: &Particle,
    occurs: Occurs,
    starts: &BTreeSet<usize>,
    children: usize,
    matches: &dyn Fn(usize, usize) -> bool,
) -> BTreeSet<usize> {
    let once = |from: &BTreeSet<usize>| -> BTreeSet<usize> {
        match particle {
            Particle::Element(element) => from
                .iter()
                .filter(|&&at| at < children && matches(*element, at))
                .map(|at| at + 1)
                .collect(),
            Particle::Any => from
                .iter()
                .filter(|&&at| at < children)
                .map(|at| at + 1)
                .collect(),
            Particle::Sequence(items) => items.iter().fold(from.clone(), |at, (item, occurs)| {
                reach(item, *occurs, &at, children, matches)
            }),
            Particle::Choice(items) => items
                .iter()
                .flat_map(|(item, occurs)| reach(item, *occurs, from, children, matches))
                .collect(),
            Particle::All(items) => from
                .iter()
                .filter_map(|&start| {
                    // Each member takes the children it matches, in any order, at most once.
                    let mut used = vec![false; items.len()];
                    let mut at = start;
                    while at < children {
                        let next = items.iter().enumerate().find(|(index, (item, _))| {
                            !used[*index]
                                && !reach(item, ONCE, &BTreeSet::from([at]), children, matches)
                                    .is_empty()
                        });
                        match next {
                            Some((index, _)) => {
                                used[index] = true;
                                at += 1;
                            }
                            None => break,
                        }
                    }
                    let complete = items
                        .iter()
                        .zip(&used)
                        .all(|((_, occurs), used)| *used || occurs.min == 0);
                    complete.then_some(at)
                })
                .collect(),
        }
    };
    let mut current = starts.clone();
    for _ in 0..occurs.min {
        current = once(&current);
        if current.is_empty() {
            return current;
        }
    }
    let mut reached = current.clone();
    let mut extra = 0;
    // Each further occurrence consumes a child at least, so this ends.
    while occurs.max.is_none_or(|max| occurs.min + extra < max) && extra as usize <= children {
        let next: BTreeSet<usize> = once(&current)
            .into_iter()
            .filter(|at| !reached.contains(at))
            .collect();
        if next.is_empty() {
            break;
        }
        reached.extend(next.iter().copied());
        current = next;
        extra += 1;
    }
    reached
}

/// Whether `children` children, as `matches` sees them, are what `model` allows.
pub(super) fn fits(
    model: &(Particle, Occurs),
    children: usize,
    matches: &dyn Fn(usize, usize) -> bool,
) -> bool {
    reach(&model.0, model.1, &BTreeSet::from([0]), children, matches).contains(&children)
}

/// The elements a model names.
pub(super) fn named(particle: &Particle, found: &mut Vec<usize>, any: &mut bool) {
    match particle {
        Particle::Element(element) => found.push(*element),
        Particle::Any => *any = true,
        Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
            for (item, _) in items {
                named(item, found, any);
            }
        }
    }
}
