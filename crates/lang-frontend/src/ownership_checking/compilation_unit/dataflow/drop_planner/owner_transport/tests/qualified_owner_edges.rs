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
