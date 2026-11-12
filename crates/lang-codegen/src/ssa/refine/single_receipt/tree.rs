//! Independently checked finite tree, never an execution certificate.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    Split(usize),
    Leaf(usize),
    Join(usize),
    Goto(usize),
}
impl Plan {
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.splits.len(), self.paths.len())
    }
    pub(super) fn events(&self) -> Option<Vec<Event>> {
        let (s, l) = self.counts();
        if !(1..=7).contains(&s)
            || !(2..=8).contains(&l)
            || l != s + 1
            || self.flags.len() != s
            || self.flags.contains(&self.root)
            || self
                .flags
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != s
            || self.splits.first()?.parent.is_some()
        {
            return None;
        }
        let expressions = self
            .splits
            .iter()
            .flat_map(|s| [s.split, s.condition])
            .chain(self.paths.iter().flat_map(|p| [p.control, p.value]))
            .collect::<Vec<_>>();
        if expressions
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != expressions.len()
        {
            return None;
        }
        if self.hole_route.last().is_none_or(|(_, arm)| *arm != 1)
            || self.hole_route.is_empty()
            || self.hole_route.len() > 3
            || self.paths.last()?.route != self.hole_route
        {
            return None;
        }
        let mut seen_s = vec![false; s];
        let mut seen_l = vec![false; l];
        let mut events = Vec::new();
        self.walk(0, &[], &mut seen_s, &mut seen_l, &mut events)?;
        if *seen_l.last()? {
            return None;
        }
        *seen_l.last_mut()? = true;
        events.push(Event::Leaf(l - 1));
        (seen_s.into_iter().all(|v| v) && seen_l.into_iter().all(|v| v)).then_some(events)
    }
    fn walk(
        &self,
        split: usize,
        route: &[(usize, usize)],
        ss: &mut [bool],
        ls: &mut [bool],
        out: &mut Vec<Event>,
    ) -> Option<()> {
        if route.len() >= 3 || *ss.get(split)? {
            return None;
        }
        ss[split] = true;
        out.push(Event::Split(split));
        for arm in 0..2 {
            let mut child_route = route.to_vec();
            child_route.push((split, arm));
            let children = self
                .splits
                .iter()
                .enumerate()
                .filter(|(_, s)| s.parent == Some((split, arm)))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let leaves = self
                .paths
                .iter()
                .enumerate()
                .filter(|(_, p)| p.route == child_route)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if child_route == self.hole_route {
                if !children.is_empty() || leaves.as_slice() != [self.paths.len() - 1] {
                    return None;
                }
                continue;
            }
            match (children.as_slice(), leaves.as_slice()) {
                ([child], []) => self.walk(*child, &child_route, ss, ls, out)?,
                ([], [leaf]) => {
                    if ls[*leaf] {
                        return None;
                    }
                    ls[*leaf] = true;
                    out.push(Event::Leaf(*leaf));
                }
                _ => return None,
            }
        }
        out.push(Event::Join(split));
        if self.hole_route.iter().any(|(i, _)| *i == split) {
            out.push(Event::Goto(split));
        }
        Some(())
    }
    pub(super) fn next(&self) -> Option<Event> {
        self.events()?.get(self.cursor).copied()
    }
    pub(super) fn valid_shape(&self) -> bool {
        self.events().is_some()
    }
    pub(super) fn split_route(&self, i: usize) -> Option<Vec<(usize, usize)>> {
        let mut route = Vec::new();
        let mut current = i;
        while let Some((parent, arm)) = self.splits.get(current)?.parent {
            if route.len() >= 3 || arm > 1 {
                return None;
            }
            route.push((parent, arm));
            current = parent;
        }
        if current != 0 {
            return None;
        }
        route.reverse();
        Some(route)
    }
    pub(super) fn needed_flags(&self, i: usize) -> Option<Vec<usize>> {
        let route = self.split_route(i)?;
        (0..self.splits.len())
            .map(|j| Some((j, self.split_route(j)?)))
            .collect::<Option<Vec<_>>>()
            .map(|v| {
                v.into_iter()
                    .filter(|(_, r)| r.starts_with(&route))
                    .map(|(j, _)| j)
                    .collect()
            })
    }
}

impl Plan {
    pub(super) fn expected_gotos(&self) -> Option<Vec<(BlockRef, BlockRef)>> {
        let mut previous = None;
        let mut edges = Vec::new();
        for (i, arm) in self.hole_route.iter().rev() {
            let split = self.splits.get(*i)?;
            let from = match previous {
                Some(block) => block,
                None => *split.arms?.get(*arm)?,
            };
            let to = split.join?;
            if from == to {
                return None;
            }
            edges.push((from, to));
            previous = Some(to);
        }
        Some(edges)
    }
}
impl Plans {
    /// Replay only recorded producer chain edges, never discover a chain from final CFG.
    pub(super) fn remaining_chain(&self) -> Option<Vec<BlockRef>> {
        let p = self.plan.as_ref()?;
        let expected = p.expected_gotos()?;
        if p.gotos != expected {
            return None;
        }
        let mut chain = vec![expected.first()?.0];
        for (from, to) in expected {
            if chain.last() != Some(&from) || chain.contains(&to) {
                return None;
            }
            chain.push(to);
        }
        for (from, to) in &self.folds {
            let index = chain.windows(2).position(|pair| pair == [*from, *to])?;
            chain.remove(index + 1);
        }
        Some(chain)
    }
}
