//! Ordinary finite source requires a closed selected old-value Return receipt.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    return replace(&held, "last")
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn suffix_replace_return_requires_closed_path_receipt() {
    let fixture = Fixture::new(&[("suffix-replace-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundaries = owned
        .owner_observations()
        .iter()
        .filter_map(|observation| observation.replace_break_observation_v1())
        .collect::<Vec<_>>();
    assert_eq!(
        boundaries.len(),
        1,
        "real selected old payload requires a closed path receipt"
    );
    assert!(boundaries[0].suffix_return_consumer_v1().is_some());
}
