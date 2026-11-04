//! Producer regressions: no drop count/span inference and no Single witness retirement.
pub(super) mod branch_boundaries;
mod capture;
pub(super) mod choice_model;
pub(super) mod combination;
pub(super) mod if_observation;
pub(super) mod loop_boundaries;

use super::formed_tests::Fixture;
use super::*;

fn checked(text: &str) -> CompilationUnitOwnership {
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    owned
}

fn literal_owner(
    fixture: &Fixture,
    source: usize,
    observation: &UnitOwnerObservation,
    text: &str,
) -> UnitOwnerId {
    let expression =
        UnitExpressionId::new(fixture.source(source), fixture.expression(source, text));
    observation
        .definitions()
        .iter()
        .find(|definition| definition.origin() == UnitOwnerOrigin::String(expression))
        .expect("owner definition at the independent AST literal")
        .id()
}

#[test]
fn name_group_binding_and_nested_replace_transport_actual_definitions() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
var a = "old a"
var b = "old b"
val incoming = "new"
val forwarded = (incoming)
val old = replace(&a, replace(&b, (forwarded)))
val moved = (old)
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert_eq!(observation.definitions().len(), 3);
    assert!(observation.is_consistent());
    let a = literal_owner(&fixture, 0, observation, "\"old a\"");
    let b = literal_owner(&fixture, 0, observation, "\"old b\"");
    let incoming = literal_owner(&fixture, 0, observation, "\"new\"");
    assert_ne!(a, b);
    assert_ne!(b, incoming);
    let commits: Vec<_> = observation
        .events()
        .iter()
        .filter_map(|event| {
            if let UnitOwnerTransportEvent::Replace {
                call,
                argument,
                previous,
                incoming,
                ..
            } = event
            {
                Some((*call, *argument, previous.clone(), incoming.clone()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(commits.len(), 2);
    assert_ne!(commits[0].0, commits[1].0);
    assert_eq!(
        (commits[0].2.clone(), commits[0].3.clone()),
        (b.clone(), incoming.clone())
    );
    assert_eq!(
        (commits[1].2.clone(), commits[1].3.clone()),
        (a.clone(), b.clone())
    );
    for (call, argument, _, incoming) in commits {
        assert!(
            observation
                .events()
                .contains(&UnitOwnerTransportEvent::Pending {
                    call,
                    argument,
                    owner: incoming
                })
        );
    }
    let bindings: Vec<_> = observation
        .events()
        .iter()
        .filter_map(|event| {
            if let UnitOwnerTransportEvent::Binding { owner, .. } = event {
                Some(owner.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        bindings,
        vec![a.clone(), b, incoming.clone(), incoming, a.clone(), a]
    );
}

#[test]
fn unsupported_suffix_rolls_back_observation_not_cleanup_or_other_callable() {
    let owned = checked(
        r#"fun unsupported(flag: Boolean): Unit {
var a = "old"
val old = replace(&a, "new")
if (flag) { val other = "branch" }
}
fun independent(): Unit {
var a = "old"
val old = replace(&a, "new")
}"#,
    );
    let observations = owned.owner_observations();
    assert_eq!(observations.len(), 2);
    assert!(matches!(
        observations[0].status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(observations[0].events().is_empty());
    assert!(observations[0].definitions().is_empty());
    assert_eq!(
        observations[1].status(),
        UnitOwnerObservationStatus::Complete
    );
    assert!(!owned.drops().is_empty());
}

#[test]
fn successive_replace_keeps_previous_owner_and_parameter_identity() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(own seed: String): Unit {
var a = seed
val first = replace(&a, "one")
val second = replace(&a, "two")
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let seed = observation
        .definitions()
        .iter()
        .find(|definition| {
            definition.origin() == UnitOwnerOrigin::Parameter(fixture.symbol(0, "seed"))
        })
        .unwrap()
        .id();
    let one = literal_owner(&fixture, 0, observation, "\"one\"");
    let two = literal_owner(&fixture, 0, observation, "\"two\"");
    let transfers = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Replace {
                previous, incoming, ..
            } => Some((previous.clone(), incoming.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(transfers, vec![(seed, one.clone()), (one, two)]);
}

#[test]
fn source_qualification_input_order_and_same_text_do_not_collapse_definitions() {
    let p = "package p\nfun run(): Unit { var a = \"same\"; val old = replace(&a, \"same\") }";
    let q = "package q\nfun run(): Unit { var a = \"same\"; val old = replace(&a, \"same\") }";
    for texts in [
        [("p/root.ko", p), ("q/root.ko", q)],
        [("q/root.ko", q), ("p/root.ko", p)],
    ] {
        let fixture = Fixture::new(&texts);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned.owner_observations().len(), 2);
        assert_ne!(owned.owner_observations()[0], owned.owner_observations()[1]);
        let mut ids = Vec::new();
        for source in 0..2 {
            let observation = owned
                .owner_observations()
                .iter()
                .find(|observation| match observation.callable() {
                    UnitOwnerCallable::Function(item) => {
                        item.source_unit() == fixture.source(source)
                    }
                    _ => false,
                })
                .unwrap();
            assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
            assert!(observation.is_consistent());
            let literals = fixture.parsed[source]
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| {
                    matches!(node.payload(), crate::parser::Expression::String { .. })
                        .then_some(UnitExpressionId::new(fixture.source(source), id))
                })
                .collect::<Vec<_>>();
            assert_eq!(literals.len(), 2);
            let actual = literals
                .iter()
                .map(|expression| {
                    observation
                        .definitions()
                        .iter()
                        .find(|definition| {
                            definition.origin() == UnitOwnerOrigin::String(*expression)
                        })
                        .unwrap()
                        .id()
                })
                .collect::<Vec<_>>();
            assert_ne!(actual[0], actual[1]);
            assert!(observation.events().iter().any(|event| matches!(event,
                UnitOwnerTransportEvent::Replace { previous, incoming, .. } if *previous == actual[0] && *incoming == actual[1])));
            ids.extend(actual);
        }
        for (index, id) in ids.iter().enumerate() {
            assert!(!ids[..index].contains(id));
        }
    }
}

#[test]
fn observation_validator_rejects_foreign_tokens_swapped_commit_and_missing_pending() {
    let text = "fun run(): Unit { var a = \"old\"; val incoming = \"new\"; val old = replace(&a, incoming) }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let foreign = fixture.shared().owner_observations()[0].definitions()[0].id();
    assert!(observation.definition(foreign.clone()).is_none());
    let mut damaged = observation.clone();
    damaged.definitions[0].id = foreign;
    assert!(!damaged.is_consistent());
    let mut damaged = observation.clone();
    for event in &mut damaged.events {
        if let UnitOwnerTransportEvent::Replace {
            previous, incoming, ..
        } = event
        {
            std::mem::swap(previous, incoming);
        }
    }
    assert!(!damaged.is_consistent());
    let mut damaged = observation.clone();
    damaged
        .events
        .retain(|event| !matches!(event, UnitOwnerTransportEvent::Pending { .. }));
    assert!(!damaged.is_consistent());
    let mut damaged = observation.clone();
    let repeated = damaged
        .events
        .iter()
        .find(|event| matches!(event, UnitOwnerTransportEvent::Move { .. }))
        .cloned();
    damaged
        .events
        .push(repeated.expect("fixture has a real Name move"));
    assert!(!damaged.is_consistent());
}

#[test]
fn loop_and_opaque_suffixes_are_capability_rejections_not_ownership_errors() {
    for suffix in ["while (flag) { break }", "val opaque = make()"] {
        let text = format!(
            r#"fun make(): String = "opaque"
fun run(flag: Boolean): Unit {{
var a = "old"
val old = replace(&a, "new")
{suffix}
}}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[1];
        let expected = if suffix.starts_with("while") {
            let statement = fixture.parsed[0]
                .ast()
                .statements()
                .iter()
                .find_map(|(id, node)| {
                    matches!(node.payload(), crate::parser::Statement::While { .. }).then_some(id)
                })
                .unwrap();
            (
                UnitDropPoint::AfterStatement(UnitStatementId::new(fixture.source(0), statement)),
                UnitOwnerUnsupportedReason::Syntax,
            )
        } else {
            (
                UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "make()"),
                )),
                UnitOwnerUnsupportedReason::OpaqueCall,
            )
        };
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: expected.0,
                reason: expected.1
            }
        );
        assert!(observation.definitions().is_empty());
        assert!(observation.events().is_empty());
        assert!(observation.is_consistent());
        let mut damaged = observation.clone();
        damaged
            .events
            .extend(owned.owner_observations()[0].events().iter().cloned());
        assert!(!damaged.events.is_empty());
        assert!(
            !damaged.is_consistent(),
            "Unsupported cannot retain a successful prefix"
        );
    }
}

#[test]
fn memory_and_rooted_entries_preserve_ast_qualified_replace_origins() {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::{Expression, parse_file},
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "root.ko",
            r#"fun run(): Unit {
var a = "old"
val incoming = "new"
val old = replace(&a, incoming)
}"#,
        )
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let literals = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::String { .. }).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(literals.len(), 2);
    let (environment, types) = standard_environments();
    let mut normalized = Vec::new();
    for input in [
        SourceUnitInput::new("root", "root.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let [indexed] = names.names().index().source_units() else {
            panic!("one actual source");
        };
        assert_eq!(indexed.source_id(), source);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.clone().validate().is_ok());
        let [observation] = owned.owner_observations() else {
            panic!("one observed function");
        };
        assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
        assert!(observation.is_consistent());
        let transfers = observation
            .events()
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Replace {
                    previous, incoming, ..
                } => Some((
                    observation.definition(previous.clone()).unwrap().origin(),
                    observation.definition(incoming.clone()).unwrap().origin(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            transfers,
            vec![(
                UnitOwnerOrigin::String(UnitExpressionId::new(indexed.id(), literals[0])),
                UnitOwnerOrigin::String(UnitExpressionId::new(indexed.id(), literals[1])),
            )]
        );
        normalized.push(transfers);
    }
    assert_eq!(normalized[0], normalized[1]);
}

#[test]
fn reanalysis_keeps_complete_facts_equal_without_making_tokens_interchangeable() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(own seed: String): Unit { var a = seed; val incoming = \"new\"; val old = replace(&a, incoming) }",
    )]);
    let first = fixture.shared();
    let second = fixture.shared();
    assert_eq!(
        first, second,
        "all ownership facts survive independent analysis"
    );
    assert!(!first.is_same_analysis(&second));
    let left = &first.owner_observations()[0];
    let right = &second.owner_observations()[0];
    assert!(left.is_consistent());
    assert!(right.is_consistent());
    assert_eq!(left, right);
    for (a, b) in left.definitions().iter().zip(right.definitions()) {
        assert_ne!(a.id(), b.id());
        assert!(left.definition(b.id()).is_none());
        assert!(right.definition(a.id()).is_none());
    }
    let mut damaged = right.clone();
    damaged.definitions[0].id = left.definitions()[0].id();
    assert_ne!(
        left, &damaged,
        "foreign definition identity is a fact mismatch"
    );
    assert_ne!(right, &damaged);
    assert_eq!(
        damaged,
        damaged.clone(),
        "equality remains reflexive for invalid facts"
    );
    let mut damaged = right.clone();
    let event = damaged
        .events
        .iter_mut()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::Move { owner, .. } => Some(owner),
            _ => None,
        })
        .expect("the seed is moved into a");
    *event = left.definitions()[0].id();
    assert_ne!(left, &damaged, "foreign event identity is a fact mismatch");
    assert_ne!(right, &damaged);
    let mut damaged = right.clone();
    damaged.definitions[0].origin = right.definitions()[1].origin();
    assert_ne!(left, &damaged, "definition origins are included");
    let mut damaged = right.clone();
    for event in &mut damaged.events {
        if let UnitOwnerTransportEvent::Replace {
            previous, incoming, ..
        } = event
        {
            std::mem::swap(previous, incoming);
        }
    }
    assert_ne!(left, &damaged, "transport direction is included");
    let mut damaged = right.clone();
    damaged.events.pop();
    assert_ne!(left, &damaged, "the full ordered event stream is included");
}
