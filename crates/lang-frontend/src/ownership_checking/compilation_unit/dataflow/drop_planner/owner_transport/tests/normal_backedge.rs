//! A live header is a source equation, not the zero-iteration P projection.
use super::*;
use crate::ownership_checking::UnitOwnerCallable;
use crate::type_checking::UnitItemId;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) { val previous = replace(&held, "next"); again = false }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn actual_normal_backedge_requires_a_header_source_product() {
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let callable = UnitOwnerCallable::Function(UnitItemId::new(
        fixture.source(0),
        fixture.parsed[0].roots()[0],
    ));
    let observation = owned
        .owner_observations()
        .iter()
        .find(|value| value.callable() == callable)
        .unwrap();
    assert!(observation.replace_break_observation_v1().is_none());
    assert!(observation.loop_observation_v1().is_none());
    assert!(observation.invariant_loop_observation_v1().is_none());
    assert!(
        observation.open_loop_observation_v1().is_some(),
        "normal backedge requires its actual header-source product, not fixed P/Q exits"
    );
}

#[test]
fn normal_backedge_keeps_two_real_catalogs_and_header_source_equation() {
    use crate::ownership_checking::{
        UnitOwnerHeaderSourceV1 as H, UnitOwnerOpenLoopObservationV1 as O, UnitOwnerOrigin,
    };
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let products = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.open_loop_observation_v1())
        .collect::<Vec<_>>();
    let [O::NormalBackedge(product)] = products.as_slice() else {
        panic!("one changing header product")
    };
    assert!(product.is_consistent());
    let [H::Entry(p), H::Backedge(q)] = product.incoming() else {
        panic!("ordered entry/backedge equation")
    };
    assert_ne!(p, q);
    assert_eq!(product.input_producers().definitions().len(), 1);
    assert_eq!(product.body_producers().definitions().len(), 1);
    assert_eq!(
        product.input_producers().definition(p).unwrap().origin(),
        UnitOwnerOrigin::String(product.initial())
    );
    assert_eq!(
        product.body_producers().definition(q).unwrap().origin(),
        UnitOwnerOrigin::String(product.argument())
    );
    assert_eq!(product.prefix_events().len(), 2);
    assert_eq!(product.body_events().len(), 6);
    assert_ne!(product.return_expression(), product.return_payload());
}

#[test]
fn normal_backedge_declines_unproved_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        SOURCE.replace("again = false", "again = flag"),
        SOURCE.replace("while(again)", "while((again))"),
        SOURCE.replace("again = false", "again = false; continue"),
        SOURCE.replace("return held", "return (held)"),
    ] {
        let fixture = Fixture::new(&[("neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| !matches!(o.open_loop_observation_v1(), Some(O::NormalBackedge(_))))
        );
    }
}

#[test]
fn ci_only_normal_backedge_rejects_damaged_completion() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(v)) => Some(v),
            _ => None,
        })
        .unwrap();
    for damage in 0..4 {
        let mut product = (**original).clone();
        match damage {
            0 => product.assignment_completed = false,
            1 => product.body_completed = false,
            2 => product.cleaned = false,
            _ => {
                product.events.pop();
            }
        }
        assert!(!product.is_consistent());
    }
}
