//! Real admitted sources test edge identity without fabricating planner facts.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerLoopEdgeV1 as E, UnitOwnerOpenLoopObservationV1 as O,
};
const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) { val previous = replace(&held, "next"); again = false }
    return held
}"#;
#[test]
fn qualified_owner_edges_keep_roles_and_reanalysis_separate() {
    let fixture = Fixture::new(&[("qualified.ko", SOURCE)]);
    let first = fixture.shared();
    let second = fixture.shared();
    compare(&fixture, &fixture.parts(&first));
    for owned in [&first, &second] {
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
    }
    let product = |owned: &crate::ownership_checking::CompilationUnitOwnership| {
        owned
            .owner_observations()
            .iter()
            .find_map(|o| match o.open_loop_observation_v1() {
                Some(O::NormalBackedge(v)) => Some((**v).clone()),
                _ => None,
            })
            .unwrap()
    };
    let a = product(&first);
    let b = product(&second);
    let edges = a.qualified_edges().expect("sealed root-qualified edges");
    assert_eq!(edges, a.clone().qualified_edges().unwrap());
    assert_ne!(edges, b.qualified_edges().unwrap());
    assert_eq!(a.statement(), b.statement());
    assert_eq!(a.root(), b.root());
    assert_eq!(edges[0].edge(), E::EnterBody(a.condition(), a.body()));
    assert_eq!(
        edges[1].edge(),
        E::Backedge(UnitDropPoint::AfterStatement(a.body()), a.condition())
    );
    assert_eq!(
        edges[2].edge(),
        E::ConditionFalseExit(a.condition(), UnitDropPoint::LoopExit(a.statement()))
    );
    for (i, edge) in edges.iter().enumerate() {
        assert_eq!(edge.callable(), a.callable());
        assert_eq!(edge.control(), a.statement());
        assert_eq!(edge.root(), a.root());
        assert!(edges[..i].iter().all(|other| edge != other));
    }
}

#[test]
fn qualified_owner_edges_isolate_sources_and_callable_roots() {
    let other = SOURCE.replace("fun run", "fun other");
    let fixture = Fixture::new(&[("first.ko", SOURCE), ("second.ko", &other)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    compare(&fixture, &fixture.parts(&owned));
    let products = owned
        .owner_observations()
        .iter()
        .filter_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(v)) => Some(v),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(products.len(), 2);
    let a = products[0].qualified_edges().unwrap();
    let b = products[1].qualified_edges().unwrap();
    assert_ne!(a[0].root().source_unit(), b[0].root().source_unit());
    assert_ne!(a[0].callable(), b[0].callable());
    assert!(a.iter().all(|edge| b.iter().all(|other| edge != other)));
}

#[test]
fn qualified_owner_edges_keep_same_source_roots_separate() {
    let source = format!("{}\n{}", SOURCE, SOURCE.replace("fun run", "fun other"));
    let fixture = Fixture::new(&[("same-source.ko", &source)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    compare(&fixture, &fixture.parts(&owned));
    let edges = owned
        .owner_observations()
        .iter()
        .filter_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(v)) => v.qualified_edges(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 2);
    assert_eq!(
        edges[0][0].root().source_unit(),
        edges[1][0].root().source_unit()
    );
    assert_ne!(edges[0][0].root(), edges[1][0].root());
    assert_ne!(edges[0][0].control(), edges[1][0].control());
    assert!(
        edges[0]
            .iter()
            .all(|edge| edges[1].iter().all(|other| edge != other))
    );
}

#[test]
fn ci_only_qualified_owner_edges_require_sealed_header() {
    let fixture = Fixture::new(&[("damaged.ko", SOURCE)]);
    let owned = fixture.shared();
    let mut product = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(v)) => Some((**v).clone()),
            _ => None,
        })
        .unwrap();
    product.core.body_completed = false;
    assert!(product.qualified_edges().is_none());
}
