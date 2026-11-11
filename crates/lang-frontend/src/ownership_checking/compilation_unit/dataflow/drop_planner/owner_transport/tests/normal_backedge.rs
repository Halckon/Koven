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
