//! Ordinary source requires two independently delivered terminal endpoints.
use super::*;
use crate::ownership_checking::UnitDropTarget;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); return held }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn loop_owned_return_requires_open_transport() {
    let fixture = Fixture::new(&[("open-loop-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let held = fixture.local(0, "held");
    let previous = fixture.local(0, "previous");
    assert!(owned.drops().iter().all(|drop| drop.target() != UnitDropTarget::Named(held)));
    assert_eq!(owned.drops().iter().filter(|drop| drop.target() == UnitDropTarget::Named(previous)).count(), 1);
    compare(&fixture, &fixture.parts(&owned));
    assert_eq!(
        owned.owner_observations().iter().filter(|observation| observation.replace_break_value.is_some()).count(),
        1,
        "the real loop body Return and separate false-path Return need an open transport product"
    );
}
