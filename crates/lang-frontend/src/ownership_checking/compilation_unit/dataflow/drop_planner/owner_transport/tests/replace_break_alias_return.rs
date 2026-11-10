//! Ordinary finite source: selected owner lives through one real immutable binding.
use super::*;
use crate::ownership_checking::UnitDropTarget;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    val forwarded = held
    return forwarded
}"#;

#[test]
fn alias_return_requires_live_selected_owner_receipt() {
    let fixture = Fixture::new(&[("alias-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundaries = owned.owner_observations().iter()
        .filter_map(|o| o.replace_break_observation_v1()).collect::<Vec<_>>();
    assert_eq!(boundaries.len(), 1, "one-hop alias must retain a real P/Q return receipt");
    let boundary = boundaries[0];
    let consumer = boundary.suffix_return_consumer_v1().expect("actual alias Return handoff");
    assert_eq!(consumer.actions().len(), 4, "Take root, Bind alias, Take alias, Return");
    assert_eq!(consumer.incoming(), &[boundary.condition_false_exit().clone(), boundary.break_boundary().clone()]);
    assert!(boundary.input_producers().definition(consumer.incoming()[0].owner()).is_some());
    assert!(boundary.body_producers().definition(consumer.incoming()[1].owner()).is_some());
    let drops = owned.drops().iter().map(|d| d.target()).collect::<Vec<_>>();
    assert_eq!(drops, [UnitDropTarget::Named(fixture.local(0, "previous"))]);
    compare(&fixture, &fixture.parts(&owned));
}
