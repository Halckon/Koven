//! Remaining original container witnesses, independently observed through both Unit entries.
use super::{ElementIndexIdentity, LoanKind, OwnershipDeferredReason, codes};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        check_compilation_unit_ownership,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, complete_compilation_unit_types},
};

fn checked(text: &str, deferred: bool) -> Vec<(SourceMap, ParsedFile, CompilationUnitOwnership)> {
    [false, true]
        .into_iter()
        .map(|rooted| {
            let mut sources = SourceMap::new();
            let source = sources.add_source("ownership-containers.ko", text).unwrap();
            let parsed = super::parse_file_twice(&sources, source, "container ownership source");
            assert!(parsed.diagnostics().is_empty());
            assert_eq!(sources.source_text(source), Ok(text));
            let (environment, types) = super::environments();
            let input = if rooted {
                SourceUnitInput::new("root", "ownership-containers.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            };
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            assert!(index.diagnostics().is_empty());
            let [entry] = index.source_units() else {
                panic!("one original source")
            };
            assert_eq!(entry.source_id(), source);
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                    .expect("final type gate");
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            for loan in owned.loans() {
                assert_eq!(loan.call().source_unit(), entry.id());
            }
            if deferred {
                assert!(!owned.deferred().is_empty());
                assert!(owned.clone().validate().is_err());
            } else {
                assert!(owned.deferred().is_empty());
                assert_eq!(
                    owned.clone().validate().is_ok(),
                    owned.diagnostics().is_empty()
                );
            }
            (sources, parsed, owned)
        })
        .collect()
}

#[test]
fn field_container_paths_preserve_parent_element_overlap() {
    let passing = "class Resource {}\n\
                   class Holder(var items: MutableList<Resource>)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun valid(own holder: Holder): Unit {\n\
                       val result = mutate(&holder.items[0])\n\
                   }";
    for (_, _, passing) in checked(passing, false) {
        assert!(
            passing.diagnostics().is_empty(),
            "{:?}",
            passing.diagnostics()
        );
        let UnitLoanTarget::Place(place) = passing.loans()[0].target() else {
            panic!("field-backed element must have a stable place");
        };
        assert_eq!(place.fields().len(), 1);
        assert_eq!(place.element(), Some(ElementIndexIdentity::Known(0)));
    }
    let failing = "class Resource {}\n\
                   class Holder(var items: MutableList<Resource>)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun outer(items: MutableList<Resource>, own effect: Unit): Unit {}\n\
                   fun invalid(own holder: Holder): Unit {\n\
                       val result = outer(holder.items, mutate(&holder.items[0]))\n\
                   }";
    for (_, _, failing) in checked(failing, false) {
        assert_eq!(codes(failing.diagnostics()), ["L0135"]);
    }
}

#[test]
fn replacement_rejects_moved_owner_and_drops_the_old_element_after_commit() {
    let failing = "class Resource {}\n\
                   fun consumeAndCreate(own list: MutableList<Resource>): Resource\n\
                   fun invalid(own list: MutableList<Resource>): Unit {\n\
                       val result = (list[0] = consumeAndCreate(list))\n\
                   }";
    for (sources, _, failing) in checked(failing, false) {
        assert_eq!(codes(failing.diagnostics()), ["L0131"]);
        assert_eq!(
            sources
                .slice(failing.diagnostics()[0].primary_span())
                .unwrap(),
            "list[0]"
        );
    }
    let passing = "class Resource {}\n\
                   fun valid(own list: MutableList<Resource>, own replacement: Resource): Unit {\n\
                       val result = (list[0] = replacement)\n\
                   }";
    for (_, _, passing) in checked(passing, false) {
        assert!(
            passing.diagnostics().is_empty(),
            "{:?}",
            passing.diagnostics()
        );
        assert!(passing.drops().iter().any(|fact| {
            matches!(fact.point(), UnitDropPoint::AfterReplacement(_))
                && matches!(fact.target(), UnitDropTarget::ReplacedElement(_))
        }));
    }
}

#[test]
fn temporary_container_lifetime_matches_element_access_mode() {
    let text = "class Resource {}\n\
                fun makeNumbers(): List<Int>\n\
                fun makeResources(): MutableList<Resource>\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun temporary(): Unit {\n\
                    val copied = makeNumbers()[0]\n\
                    val borrowed = inspect(makeResources()[0])\n\
                    val exclusive = mutate(&makeResources()[0])\n\
                }";
    let first = checked(text, false);
    for (_, _, owned) in &first {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.loans().iter().any(|loan| {
            matches!(loan.target(), UnitLoanTarget::Temporary(_)) && loan.kind() == LoanKind::Shared
        }));
        assert!(owned.loans().iter().any(|loan| {
            matches!(loan.target(), UnitLoanTarget::Temporary(_))
                && loan.kind() == LoanKind::Exclusive
        }));
        assert!(owned.drops().iter().any(|fact| {
            matches!(fact.point(), UnitDropPoint::AfterExpression(_))
                && matches!(fact.target(), UnitDropTarget::Temporary(_))
        }));
        assert!(owned.drops().iter().any(|fact| {
            matches!(fact.point(), UnitDropPoint::CallReturn(_))
                && matches!(fact.target(), UnitDropTarget::Temporary(_))
        }));
        assert!(
            !owned
                .deferred()
                .iter()
                .any(|fact| fact.reason() == OwnershipDeferredReason::IndexPlace)
        );
    }
    let repeated = checked(text, false);
    for ((_, _, owned), (_, _, repeated)) in first.iter().zip(&repeated) {
        assert_eq!(
            format!("{:?}", owned.loans()),
            format!("{:?}", repeated.loans())
        );
        assert_eq!(
            format!("{:?}", owned.drops()),
            format!("{:?}", repeated.drops())
        );
    }
}

#[test]
fn post_index_field_projection_stays_deferred_without_an_early_drop_plan() {
    let text = "class Resource {}\n\
                class Holder(var payload: Resource)\n\
                fun deferred(own list: List<Holder>): Unit {\n\
                    val projected = list[0].payload\n\
                }";
    for (_, _, owned) in checked(text, true) {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(
            owned
                .deferred()
                .iter()
                .any(|fact| fact.reason() == OwnershipDeferredReason::IndexPlace)
        );
        assert!(owned.drops().is_empty());
    }
}
