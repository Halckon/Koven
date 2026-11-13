//! Ordinary source-driven checks; no fault injection is executed locally.
use super::tests::SOURCE;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{UnitDropPoint, UnitDropTarget};
#[test]
fn parent_boundary_records_distinct_parent_cleanup_and_complete_roots() {
    for (suffix, live_count, drop_count) in [
        ("println(a); println(b)", 2, 0),
        ("println(b)", 1, 1),
        ("", 0, 2),
    ] {
        let source = SOURCE.replace("println(a); println(b)", suffix);
        let fixture = Fixture::new(&[("parent-boundary.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        let nested = owned
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        let parent = nested.parent_boundary_v1().expect("actual parent boundary");
        assert_eq!(parent.control(), nested.parent());
        assert_ne!(parent.condition(), nested.condition());
        assert_ne!(parent.body(), nested.body());
        assert_eq!(parent.roots().len(), 2);
        let (live, drops) = parent.false_exit_partition();
        assert_eq!((live.len(), drops.len()), (live_count, drop_count));
        for drop in drops {
            assert_eq!(drop.point(), UnitDropPoint::LoopExit(nested.parent()));
            assert!(
                parent
                    .roots()
                    .iter()
                    .any(|root| drop.target() == UnitDropTarget::Named(*root))
            );
        }
    }
}

#[test]
fn parent_boundary_rejects_non_normal_parent_paths() {
    for tail in [
        "break",
        "return",
        "if (g0) { break }",
        "while (g0) { break }",
    ] {
        let source = SOURCE.replace("n0=n0-1; g0=n0>0", tail);
        let fixture = Fixture::new(&[("parent-reject.ko", &source)]);
        let owned = fixture.shared();
        assert!(
            owned.owner_observations().iter().all(|o| o
                .nested_normal_body_v1()
                .and_then(|n| n.parent_boundary_v1())
                .is_none()),
            "{tail}"
        );
    }
}

#[test]
fn parent_boundary_retires_resumed_root_and_preserves_other_root() {
    let source = SOURCE.replace("println(a); println(b)", "println(a)");
    let fixture = Fixture::new(&[("parent-retire-resumed.ko", &source)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    let nested = owned
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .unwrap();
    let resumed = nested.parent_resume_write().unwrap().root();
    let parent = nested.parent_boundary_v1().unwrap();
    let (live, drops) = parent.false_exit_partition();
    let other = parent
        .roots()
        .iter()
        .copied()
        .filter(|root| *root != resumed)
        .collect::<Vec<_>>();
    assert_eq!(other.len(), 1);
    assert_eq!(live, other);
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].target(), UnitDropTarget::Named(resumed));
    assert_eq!(drops[0].point(), UnitDropPoint::LoopExit(parent.control()));
}
