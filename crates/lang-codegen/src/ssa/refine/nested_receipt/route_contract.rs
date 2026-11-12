//! Fixed structural routes only: not a substitute for live producer evidence.
use super::{
    UnitBorrowNestedForwardFlagLayout as Layout, UnitBorrowNestedForwardTopology as Topology,
};

#[derive(Clone, Copy)]
pub(super) struct RouteContract {
    parents: &'static [Option<usize>],
    shared_inner: bool,
    routes: &'static [&'static [(usize, usize)]],
}
impl RouteContract {
    pub(super) fn new(topology: Topology) -> Self {
        match topology {
            Topology::NestedThen | Topology::NestedThenTail => Self {
                parents: &[None, Some(0)],
                shared_inner: false,
                routes: &[&[(0, 0), (1, 0)], &[(0, 0), (1, 1)], &[(0, 1)]],
            },
            Topology::NestedElse | Topology::NestedElseCascadeTail => Self {
                parents: &[None, Some(1)],
                shared_inner: false,
                routes: &[&[(0, 0)], &[(0, 1), (1, 0)], &[(0, 1), (1, 1)]],
            },
            Topology::BalancedBoth => Self {
                parents: &[None, Some(0), Some(1)],
                shared_inner: true,
                routes: &[
                    &[(0, 0), (1, 0)],
                    &[(0, 0), (1, 1)],
                    &[(0, 1), (2, 0)],
                    &[(0, 1), (2, 1)],
                ],
            },
        }
    }
    pub(super) fn counts(self) -> (usize, usize) {
        (self.parents.len(), self.routes.len())
    }
    pub(super) fn parent_arm(self, split: usize) -> Option<usize> {
        self.parents.get(split).copied().flatten()
    }
    pub(super) fn route(self, leaf: usize) -> Option<&'static [(usize, usize)]> {
        self.routes.get(leaf).copied()
    }
    pub(super) fn flag_indices(self, layout: Layout) -> Option<Vec<usize>> {
        match layout {
            Layout::Distinct => Some((0..self.parents.len()).collect()),
            Layout::SharedInner if self.shared_inner => Some(vec![0, 1, 1]),
            Layout::SharedInner => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_nested_route_contract_literal_goldens() {
        let then: &[&[(usize, usize)]] = &[&[(0, 0), (1, 0)], &[(0, 0), (1, 1)], &[(0, 1)]];
        let other: &[&[(usize, usize)]] = &[&[(0, 0)], &[(0, 1), (1, 0)], &[(0, 1), (1, 1)]];
        let balanced: &[&[(usize, usize)]] = &[
            &[(0, 0), (1, 0)],
            &[(0, 0), (1, 1)],
            &[(0, 1), (2, 0)],
            &[(0, 1), (2, 1)],
        ];
        for (topology, routes, parents, flags) in [
            (Topology::NestedThen, then, vec![None, Some(0)], vec![0, 1]),
            (
                Topology::NestedThenTail,
                then,
                vec![None, Some(0)],
                vec![0, 1],
            ),
            (Topology::NestedElse, other, vec![None, Some(1)], vec![0, 1]),
            (
                Topology::NestedElseCascadeTail,
                other,
                vec![None, Some(1)],
                vec![0, 1],
            ),
            (
                Topology::BalancedBoth,
                balanced,
                vec![None, Some(0), Some(1)],
                vec![0, 1, 2],
            ),
        ] {
            let c = RouteContract::new(topology);
            assert_eq!(c.counts(), (parents.len(), routes.len()));
            for (i, expected) in routes.iter().enumerate() {
                assert_eq!(c.route(i), Some(*expected));
            }
            for (i, expected) in parents.iter().enumerate() {
                assert_eq!(c.parent_arm(i), *expected);
            }
            assert_eq!(c.route(routes.len()), None);
            assert_eq!(c.route(usize::MAX), None);
            assert_eq!(c.parent_arm(parents.len()), None);
            assert_eq!(c.parent_arm(usize::MAX), None);
            assert_eq!(c.flag_indices(Layout::Distinct), Some(flags));
            assert_eq!(
                c.flag_indices(Layout::SharedInner),
                if topology == Topology::BalancedBoth {
                    Some(vec![0, 1, 1])
                } else {
                    None
                }
            );
        }
    }
}
