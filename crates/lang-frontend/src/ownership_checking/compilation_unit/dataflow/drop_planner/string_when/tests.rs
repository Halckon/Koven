//! Mutate only a real public producer inventory to test the completeness guard.
use super::same_cleanup_inventory;
use crate::{
    analysis::{UnitSourceDescriptor, analyze_basic_unit_ownership, analyze_unit_names},
    ownership_checking::{UnitDropFact, UnitDropPoint, UnitDropTarget},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn real_named_inventory_rejects_missing_or_wrong_target_match_and_miss() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "entry.ko",
            "fun entry(): Unit { val subject = \"s\"\nwhen (subject) { \"a\", \"b\" -> {} } }",
        )
        .unwrap();
    let (ne, te) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::new("root", "entry.ko", source)],
        ne,
    )
    .unwrap();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &te).unwrap();
    let (_, owned) = analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, &te, typed)
        .unwrap()
        .into_result()
        .unwrap();
    assert!(owned.clone().validate().is_ok());
    let required = owned.drops();
    assert!(same_cleanup_inventory(required, required));
    let named: Vec<_> = required
        .iter()
        .enumerate()
        .filter(|(_, fact)| {
            matches!(fact.target(), UnitDropTarget::Named(_))
                && matches!(
                    fact.point(),
                    UnitDropPoint::WhenAlternativeMatch { .. }
                        | UnitDropPoint::WhenAlternativeNoMatch { .. }
                )
        })
        .collect();
    assert_eq!(named.len(), 3, "two actual matches and the final miss");
    let wrong_target = required
        .iter()
        .find_map(|fact| match fact.target() {
            target @ UnitDropTarget::Temporary(_) => Some(target),
            _ => None,
        })
        .unwrap();
    for (index, fact) in named {
        let mut missing = required.to_vec();
        missing.remove(index);
        assert!(!same_cleanup_inventory(required, &missing));
        let mut wrong = required.to_vec();
        wrong[index] = UnitDropFact::new(fact.point(), wrong_target, fact.value_origin());
        assert!(!same_cleanup_inventory(required, &wrong));
    }
}
