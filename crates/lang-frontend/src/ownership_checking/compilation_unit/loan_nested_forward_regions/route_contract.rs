//! Declaration structure only. No cursor, state, completion, or exit protocol lives here.
use super::UnitBorrowNestedForwardTopology as Topology;
type Edge = (usize, bool);
type Path = [Option<Edge>; 2];
struct Split {
    parent: Option<Edge>,
    depth: usize,
    // Historical observation slots, in true/false order, not execution stages.
    roles: [usize; 2],
}
struct Routes {
    splits: &'static [Split],
    leaves: &'static [Path],
}
const THEN: Routes = Routes {
    splits: &[
        Split {
            parent: None,
            depth: 0,
            roles: [0, 3],
        },
        Split {
            parent: Some((0, true)),
            depth: 1,
            roles: [1, 2],
        },
    ],
    leaves: &[
        [Some((0, true)), Some((1, true))],
        [Some((0, true)), Some((1, false))],
        [Some((0, false)), None],
    ],
};
const ELSE: Routes = Routes {
    splits: &[
        Split {
            parent: None,
            depth: 0,
            roles: [3, 0],
        },
        Split {
            parent: Some((0, false)),
            depth: 1,
            roles: [1, 2],
        },
    ],
    leaves: &[
        [Some((0, true)), None],
        [Some((0, false)), Some((1, true))],
        [Some((0, false)), Some((1, false))],
    ],
};
const BALANCED: Routes = Routes {
    splits: &[
        Split {
            parent: None,
            depth: 0,
            roles: [0, 3],
        },
        Split {
            parent: Some((0, true)),
            depth: 1,
            roles: [1, 2],
        },
        Split {
            parent: Some((0, false)),
            depth: 1,
            roles: [4, 5],
        },
    ],
    leaves: &[
        [Some((0, true)), Some((1, true))],
        [Some((0, true)), Some((1, false))],
        [Some((0, false)), Some((2, true))],
        [Some((0, false)), Some((2, false))],
    ],
};
impl Topology {
    fn routes(self) -> &'static Routes {
        match self {
            Self::NestedThen | Self::NestedThenTail => &THEN,
            Self::NestedElse | Self::NestedElseCascadeTail => &ELSE,
            Self::BalancedBoth => &BALANCED,
        }
    }
    pub(crate) fn leaf_path(self, leaf: usize) -> Option<Path> {
        self.routes().leaves.get(leaf).copied()
    }
    // Some(None) is the root split; None is an invalid index.
    pub(crate) fn split_parent(self, split: usize) -> Option<Option<Edge>> {
        self.routes().splits.get(split).map(|s| s.parent)
    }
    pub(crate) fn split_depth(self, split: usize) -> Option<usize> {
        self.routes().splits.get(split).map(|s| s.depth)
    }
    pub(crate) fn entry_role(self, split: usize, then: bool) -> Option<usize> {
        self.routes()
            .splits
            .get(split)
            .map(|s| s.roles[usize::from(!then)])
    }
}
