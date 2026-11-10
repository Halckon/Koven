//! Negation preserves the actual Boolean condition identity, not its operand identity.
use super::*;
const ORIGINAL: &str = r#"fun run(flag: Boolean) { while (!flag) { val local = "x" } }"#;
#[test]
fn negated_parameter_retains_actual_loop_condition() {
    let fixture = Fixture::new(&[("root.ko", ORIGINAL)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observed = owned.owner_observations()[0]
        .loop_observation_v1()
        .expect("closed negated condition");
    assert!(observed.is_consistent());
    assert_eq!(
        observed.condition(),
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "!flag"))
    );
    assert_eq!(observed.body_transport().producers().definitions().len(), 1);
    compare(&fixture, &fixture.parts(&owned));
}
