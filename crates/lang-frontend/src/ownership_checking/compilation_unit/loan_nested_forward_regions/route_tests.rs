//! Literal declaration-route equivalence tests; these do not manufacture execution evidence.
use super::UnitBorrowNestedForwardTopology as T;

#[test]
fn ordinary_nested_route_literals_preserve_all_existing_shapes() {
    let then = [
        [Some((0, true)), Some((1, true))],
        [Some((0, true)), Some((1, false))],
        [Some((0, false)), None],
    ];
    let otherwise = [
        [Some((0, true)), None],
        [Some((0, false)), Some((1, true))],
        [Some((0, false)), Some((1, false))],
    ];
    let balanced = [
        [Some((0, true)), Some((1, true))],
        [Some((0, true)), Some((1, false))],
        [Some((0, false)), Some((2, true))],
        [Some((0, false)), Some((2, false))],
    ];
    for (topology, paths) in [
        (T::NestedThen, then.as_slice()),
        (T::NestedThenTail, then.as_slice()),
        (T::NestedElse, otherwise.as_slice()),
        (T::NestedElseCascadeTail, otherwise.as_slice()),
        (T::BalancedBoth, balanced.as_slice()),
    ] {
        for (leaf, path) in paths.iter().enumerate() {
            assert_eq!(topology.leaf_path(leaf), Some(*path));
        }
    }
}

#[test]
fn ordinary_nested_route_parents_roles_and_bounds_are_explicit() {
    for (topology, splits, leaves, parents, roles) in [
        (
            T::NestedThen,
            2,
            3,
            vec![None, Some((0, true))],
            vec![[0, 3], [1, 2]],
        ),
        (
            T::NestedThenTail,
            2,
            3,
            vec![None, Some((0, true))],
            vec![[0, 3], [1, 2]],
        ),
        (
            T::NestedElse,
            2,
            3,
            vec![None, Some((0, false))],
            vec![[3, 0], [1, 2]],
        ),
        (
            T::NestedElseCascadeTail,
            2,
            3,
            vec![None, Some((0, false))],
            vec![[3, 0], [1, 2]],
        ),
        (
            T::BalancedBoth,
            3,
            4,
            vec![None, Some((0, true)), Some((0, false))],
            vec![[0, 3], [1, 2], [4, 5]],
        ),
    ] {
        for i in 0..splits {
            assert_eq!(topology.split_depth(i), Some(if i == 0 { 0 } else { 1 }));
            assert_eq!(topology.split_parent(i), Some(parents[i]));
            assert_eq!(topology.entry_role(i, true), Some(roles[i][0]));
            assert_eq!(topology.entry_role(i, false), Some(roles[i][1]));
        }
        for invalid in [splits, splits + 1, usize::MAX] {
            assert_eq!(topology.split_depth(invalid), None);
            assert_eq!(topology.split_parent(invalid), None);
            assert_eq!(topology.entry_role(invalid, true), None);
            assert_eq!(topology.entry_role(invalid, false), None);
        }
        for invalid in [leaves, leaves + 1, usize::MAX] {
            assert_eq!(topology.leaf_path(invalid), None);
        }
    }
}
