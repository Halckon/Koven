//! Independent finite graph and replay of actual producer edges; never frontend execution inference.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    Split(usize),
    Leaf(usize),
    Join(usize),
    Goto(usize, usize),
}
impl Plan {
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.splits.len(), self.paths.len())
    }
    pub(super) fn events(&self) -> Option<Vec<Event>> {
        let (s, l) = self.counts();
        if !(1..=7).contains(&s)
            || !(2..=8).contains(&l)
            || !(2..=4).contains(&self.hole_routes.len())
            || l + self.hole_routes.len() != s + 2
            || self.flags.len() != s
            || self.flags.contains(&self.root)
            || self
                .flags
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != s
            || self.splits.first()?.parent.is_some()
            || !self.paths.last()?.route.is_empty()
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
        if self
            .hole_routes
            .iter()
            .any(|r| r.is_empty() || r.len() > 3 || r.last().is_none_or(|(_, arm)| *arm != 1))
            || self
                .hole_routes
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.hole_routes.len()
        {
            return None;
        }
        let mut ss = vec![false; s];
        let mut ls = vec![false; l];
        let mut hs = Vec::new();
        let mut out = Vec::new();
        self.walk(0, &[], &mut ss, &mut ls, &mut hs, &mut out)?;
        if hs != self.hole_routes || *ls.last()? {
            return None;
        }
        *ls.last_mut()? = true;
        out.push(Event::Leaf(l - 1));
        (ss.into_iter().all(|v| v) && ls.into_iter().all(|v| v)).then_some(out)
    }
    fn walk(
        &self,
        split: usize,
        route: &[(usize, usize)],
        ss: &mut [bool],
        ls: &mut [bool],
        hs: &mut Vec<Vec<(usize, usize)>>,
        out: &mut Vec<Event>,
    ) -> Option<()> {
        if route.len() >= 3 || *ss.get(split)? {
            return None;
        }
        ss[split] = true;
        out.push(Event::Split(split));
        let mut live = [false; 2];
        for (arm, continues) in live.iter_mut().enumerate() {
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
            if self.hole_routes.contains(&child_route) {
                if !children.is_empty() || !leaves.is_empty() {
                    return None;
                }
                hs.push(child_route);
                *continues = true;
            } else {
                match (children.as_slice(), leaves.as_slice()) {
                    ([child], []) => {
                        self.walk(*child, &child_route, ss, ls, hs, out)?;
                        *continues = self.hole_routes.iter().any(|r| r.starts_with(&child_route));
                    }
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
        }
        out.push(Event::Join(split));
        for (arm, live) in live.into_iter().enumerate() {
            if live {
                out.push(Event::Goto(split, arm));
            }
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
    pub(super) fn predecessor(&self, i: usize, arm: usize) -> Option<BlockRef> {
        let mut route = self.split_route(i)?;
        route.push((i, arm));
        if !self.hole_routes.iter().any(|r| r.starts_with(&route)) {
            return None;
        }
        if self.hole_routes.contains(&route) {
            return self.splits.get(i)?.arms?.get(arm).copied();
        }
        let mut children = self.splits.iter().filter(|s| s.parent == Some((i, arm)));
        let result = children.next()?.join;
        if children.next().is_some() {
            return None;
        }
        result
    }
    pub(super) fn expected_gotos(&self) -> Option<Vec<(BlockRef, BlockRef)>> {
        self.events()?
            .into_iter()
            .filter_map(|e| match e {
                Event::Goto(i, a) => Some((i, a)),
                _ => None,
            })
            .map(|(i, a)| Some((self.predecessor(i, a)?, self.splits.get(i)?.join?)))
            .collect()
    }
}
impl Plans {
    pub(super) fn canonical(&self, mut block: BlockRef) -> BlockRef {
        for (from, to) in &self.folds {
            if block == *to {
                block = *from;
            }
        }
        block
    }
    pub(super) fn remaining_edges(&self) -> Option<Vec<(BlockRef, BlockRef)>> {
        let p = self.plan.as_ref()?;
        let mut edges = p.expected_gotos()?;
        if edges != p.gotos
            || edges.iter().any(|(a, b)| a == b)
            || edges
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != edges.len()
        {
            return None;
        }
        // Conditional targets are not legal fold targets even if no Goto targets them.
        let branch_targets = p
            .splits
            .iter()
            .flat_map(|s| s.arms.into_iter().flatten())
            .collect::<Vec<_>>();
        let mut removed = Vec::new();
        for (from, to) in &self.folds {
            if removed.contains(from)
                || removed.contains(to)
                || branch_targets.contains(to)
                || edges.iter().filter(|(a, _)| a == from).count() != 1
                || edges.iter().filter(|(_, b)| b == to).count() != 1
            {
                return None;
            }
            let index = edges.iter().position(|edge| edge == &(*from, *to))?;
            edges.remove(index);
            removed.push(*to);
            for (a, b) in &mut edges {
                if *a == *to {
                    *a = *from;
                }
                if *b == *to {
                    *b = *from;
                }
            }
            if edges.iter().any(|(a, b)| a == b)
                || edges
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != edges.len()
            {
                return None;
            }
        }
        Some(edges)
    }
}
