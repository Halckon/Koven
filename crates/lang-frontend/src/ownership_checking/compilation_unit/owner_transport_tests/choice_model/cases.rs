//! Synthetic transport graphs use actual source-qualified identities and producer tokens.
//! The catalog's real source has a Unit If: these graphs do not assert typed String-If
//! admission. Legacy observation remains Unsupported for real String-result consumers.
use super::*;

const SOURCE: &str = r#"fun run(): Unit {
var root = "old"
if (true) { val left = "left" } else { val right = "right" }
val incoming = "next"
val previous = replace(&root, incoming)
val forwarded = (previous)
}"#;

struct Ids {
    old: UnitOwnerId,
    left: UnitOwnerId,
    right: UnitOwnerId,
    next: UnitOwnerId,
    root: UnitSymbolId,
    incoming: UnitSymbolId,
    previous: UnitSymbolId,
    forwarded: UnitSymbolId,
    call: UnitExpressionId,
    incoming_use: UnitExpressionId,
    previous_use: UnitExpressionId,
    group: UnitExpressionId,
    old_literal: UnitExpressionId,
    next_literal: UnitExpressionId,
}

fn fixture() -> (Model, Ids) {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    model_from(&fixture, &owned)
}

fn model_from(fixture: &Fixture, owned: &CompilationUnitOwnership) -> (Model, Ids) {
    let catalog = owned
        .owner_observations()
        .iter()
        .find(|observation| {
            matches!(observation.callable(), UnitOwnerCallable::Function(item)
            if item.source_unit() == fixture.source(0))
        })
        .expect("the root source's callable observation")
        .clone();
    assert_eq!(catalog.status(), UnitOwnerObservationStatus::Complete);
    assert!(catalog.is_consistent());
    let expr = |text| UnitExpressionId::new(fixture.source(0), fixture.expression(0, text));
    let old = literal_owner(fixture, 0, &catalog, "\"old\"");
    let left = literal_owner(fixture, 0, &catalog, "\"left\"");
    let right = literal_owner(fixture, 0, &catalog, "\"right\"");
    let next = literal_owner(fixture, 0, &catalog, "\"next\"");
    let root = fixture.symbol(0, "root");
    let incoming = fixture.symbol(0, "incoming");
    let previous = fixture.symbol(0, "previous");
    let forwarded = fixture.symbol(0, "forwarded");
    let call = expr("replace(&root, incoming)");
    let control = expr("if (true) { val left = \"left\" } else { val right = \"right\" }");
    let choice = Choice {
        id: ChoiceId {
            identity: catalog.identity.clone(),
            callable: catalog.callable,
            control,
        },
        events: [
            vec![Event::Create(expr("\"left\""), left.clone())],
            vec![Event::Create(expr("\"right\""), right.clone())],
        ],
        inputs: [
            Input {
                edge: 0,
                tail: expr("\"left\""),
                owner: left.clone(),
            },
            Input {
                edge: 1,
                tail: expr("\"right\""),
                owner: right.clone(),
            },
        ],
    };
    let selected = Value::Choice(choice.id.clone());
    let model = Model {
        catalog: super::super::super::owner_transport::UnitOwnerProducerCatalogV1::from_legacy(
            &catalog,
        )
        .unwrap(),
        prefix: vec![
            Event::Create(expr("\"old\""), old.clone()),
            Event::Bind(root, expr("\"old\""), Value::Definition(old.clone())),
        ],
        choice: Some(choice),
        suffix: vec![
            Event::Pending(call, control, selected.clone()),
            Event::Replace {
                call,
                argument: control,
                root,
                previous: Value::Definition(old.clone()),
                incoming: selected.clone(),
            },
            Event::Bind(previous, call, Value::Definition(old.clone())),
            Event::Move(expr("previous"), previous, Value::Definition(old.clone())),
            Event::Forward(
                expr("previous"),
                expr("(previous)"),
                Value::Definition(old.clone()),
            ),
            Event::Bind(
                forwarded,
                expr("(previous)"),
                Value::Definition(old.clone()),
            ),
            Event::Drop(
                UnitDropPoint::AfterExpression(control),
                forwarded,
                Value::Definition(old.clone()),
            ),
            Event::Drop(UnitDropPoint::AfterExpression(control), root, selected),
            Event::Create(expr("\"next\""), next.clone()),
            Event::Bind(incoming, expr("\"next\""), Value::Definition(next.clone())),
            Event::Drop(
                UnitDropPoint::AfterExpression(control),
                incoming,
                Value::Definition(next.clone()),
            ),
        ],
    };
    (
        model,
        Ids {
            old,
            left,
            right,
            next,
            root,
            incoming,
            previous,
            forwarded,
            call,
            incoming_use: expr("incoming"),
            previous_use: expr("previous"),
            group: expr("(previous)"),
            old_literal: expr("\"old\""),
            next_literal: expr("\"next\""),
        },
    )
}

#[test]
fn choice_pending_replace_and_drop_replay_each_selected_owner_only() {
    let (model, ids) = fixture();
    let states = model.validate().expect("two complete independent traces");
    assert_eq!(
        model.catalog.definitions().len(),
        4,
        "choice is not a producer"
    );
    assert_eq!(states[0][ids.left.index], Slot::Dropped);
    assert_eq!(states[0][ids.right.index], Slot::Uncreated);
    assert_eq!(states[1][ids.left.index], Slot::Uncreated);
    assert_eq!(states[1][ids.right.index], Slot::Dropped);
    for state in states {
        assert_eq!(state[ids.old.index], Slot::Dropped);
    }
}

#[test]
fn choice_binding_becomes_previous_replace_result_without_new_definition() {
    let (mut model, ids) = fixture();
    let selected = Value::Choice(model.choice.as_ref().unwrap().id.clone());
    model.prefix.push(Event::Drop(
        UnitDropPoint::AfterExpression(ids.old_literal),
        ids.root,
        Value::Definition(ids.old),
    ));
    model.suffix = vec![
        Event::Bind(
            ids.root,
            model.choice.as_ref().unwrap().id.control,
            selected.clone(),
        ),
        Event::Create(ids.next_literal, ids.next.clone()),
        Event::Bind(
            ids.incoming,
            ids.next_literal,
            Value::Definition(ids.next.clone()),
        ),
        Event::Move(
            ids.incoming_use,
            ids.incoming,
            Value::Definition(ids.next.clone()),
        ),
        Event::Pending(
            ids.call,
            ids.incoming_use,
            Value::Definition(ids.next.clone()),
        ),
        Event::Replace {
            call: ids.call,
            argument: ids.incoming_use,
            root: ids.root,
            previous: selected.clone(),
            incoming: Value::Definition(ids.next.clone()),
        },
        Event::Bind(ids.previous, ids.call, selected.clone()),
        Event::Move(ids.previous_use, ids.previous, selected.clone()),
        Event::Forward(ids.previous_use, ids.group, selected.clone()),
        Event::Bind(ids.forwarded, ids.group, selected.clone()),
        Event::Drop(
            UnitDropPoint::AfterExpression(ids.old_literal),
            ids.forwarded,
            selected,
        ),
        Event::Drop(
            UnitDropPoint::AfterExpression(ids.old_literal),
            ids.root,
            Value::Definition(ids.next),
        ),
    ];
    assert!(model.validate().is_some());
}

#[test]
fn malformed_edges_inputs_and_identity_fail_closed() {
    let (model, ids) = fixture();
    let (foreign, foreign_ids) = fixture();
    let mut cases = Vec::new();
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().inputs.swap(0, 1);
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().inputs[0].owner = ids.right.clone();
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().inputs[0].tail = ids.old_literal;
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().inputs[0].owner = foreign_ids.left;
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().id.identity =
        foreign.choice.as_ref().unwrap().id.identity.clone();
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().events[0].clear();
    cases.push(graph);
    let mut graph = model.clone();
    let tail = graph.choice.as_ref().unwrap().inputs[0].tail;
    graph.choice.as_mut().unwrap().events[0].push(Event::Bind(
        ids.incoming,
        tail,
        Value::Definition(ids.left.clone()),
    ));
    graph.choice.as_mut().unwrap().events[0].push(Event::Drop(
        UnitDropPoint::AfterExpression(ids.old_literal),
        ids.incoming,
        Value::Definition(ids.left),
    ));
    cases.push(graph);
    for (index, graph) in cases.iter().enumerate() {
        assert!(graph.validate().is_none(), "malformed input case {index}");
    }
}

#[test]
fn forward_choice_double_consumption_and_wrong_slots_fail_closed() {
    let (model, ids) = fixture();
    let selected = Value::Choice(model.choice.as_ref().unwrap().id.clone());
    let mut cases = Vec::new();
    let mut graph = model.clone();
    graph.prefix.push(Event::Bind(
        ids.incoming,
        model.choice.as_ref().unwrap().id.control,
        selected.clone(),
    ));
    cases.push(graph);
    let mut graph = model.clone();
    let tail = graph.choice.as_ref().unwrap().inputs[0].tail;
    graph.choice.as_mut().unwrap().events[0].push(Event::Forward(
        tail,
        model.choice.as_ref().unwrap().id.control,
        selected.clone(),
    ));
    cases.push(graph); // Cyclic/self choice input is unavailable before the choice finishes.
    let mut graph = model.clone();
    graph.suffix.push(Event::Drop(
        UnitDropPoint::AfterExpression(ids.old_literal),
        ids.root,
        selected.clone(),
    ));
    cases.push(graph);
    let mut graph = model.clone();
    graph.suffix[0] = Event::Pending(ids.call, ids.next_literal, selected.clone());
    cases.push(graph);
    let mut graph = model.clone();
    graph.suffix[2] = Event::Bind(ids.previous, ids.next_literal, Value::Definition(ids.old));
    cases.push(graph);
    let mut graph = model.clone();
    graph.suffix.clear();
    cases.push(graph); // Unconsumed Result is not a proved implicit return or temporary drop.
    let mut graph = model.clone();
    graph.suffix.push(Event::Create(
        model.choice.as_ref().unwrap().inputs[0].tail,
        ids.left,
    ));
    cases.push(graph); // Must not recreate the unselected branch producer after the join.
    for (index, graph) in cases.iter().enumerate() {
        assert!(
            graph.validate().is_none(),
            "malformed continuation case {index}"
        );
    }
}

#[test]
fn real_string_result_consumers_stay_unsupported_and_keep_cleanup() {
    for (index, body) in CONSUMERS.iter().enumerate() {
        let text = consumer_source(body);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        let control = [
            "if (flag) \"new\" else \"other\"",
            "if (flag) \"old\" else \"other\"",
        ][index];
        let observations = owned.owner_observations();
        assert_eq!(
            observations[0].status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, control)
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            }
        );
        assert!(observations[0].definitions().is_empty());
        assert!(observations[0].events().is_empty());
        assert_eq!(
            observations[1].status(),
            UnitOwnerObservationStatus::Complete
        );
        assert_eq!(
            owned.drops().len(),
            3,
            "two consumer drops plus the sibling's drop"
        );
    }
}

#[test]
fn choice_join_cannot_overwrite_an_existing_result_slot() {
    let (mut model, ids) = fixture();
    model.prefix.pop();
    model.prefix.push(Event::Forward(
        ids.old_literal,
        model.choice.as_ref().unwrap().id.control,
        Value::Definition(ids.old.clone()),
    ));
    model.suffix.insert(
        1,
        Event::Bind(
            ids.root,
            model.choice.as_ref().unwrap().id.control,
            Value::Definition(ids.old),
        ),
    );
    assert!(
        model.validate().is_none(),
        "join must not alias two owners in one Result slot"
    );
}

#[test]
fn foreign_source_callable_and_unknown_choice_are_not_transport_values() {
    let sources = Fixture::new(&[("root.ko", SOURCE), ("other.ko", "fun other(): Unit {}")]);
    let owned = sources.shared();
    let (model, ids) = model_from(&sources, &owned);
    assert_ne!(
        sources.source(1),
        model.choice.as_ref().unwrap().id.control.source_unit(),
        "foreign source in the same analysis"
    );
    let other = owned
        .owner_observations()
        .iter()
        .find(|observation| observation.callable() != model.catalog.callable())
        .expect("second source callable")
        .callable();
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().id.callable = other;
    assert!(graph.validate().is_none());
    let mut graph = model.clone();
    graph.choice.as_mut().unwrap().inputs[0].tail = UnitExpressionId::new(
        sources.source(1),
        graph.choice.as_mut().unwrap().inputs[0].tail.expression(),
    );
    assert!(graph.validate().is_none());
    let mut graph = model.clone();
    let mut unknown = graph.choice.as_mut().unwrap().id.clone();
    unknown.control = ids.next_literal;
    graph.suffix[0] = Event::Pending(
        ids.call,
        graph.choice.as_mut().unwrap().id.control,
        Value::Choice(unknown),
    );
    assert!(graph.validate().is_none());
}

#[test]
fn a_producer_cannot_overwrite_an_existing_result_slot() {
    let (mut model, ids) = fixture();
    model.prefix[1] = Event::Forward(
        ids.old_literal,
        ids.next_literal,
        Value::Definition(ids.old.clone()),
    );
    let next_create = model.suffix.remove(8);
    assert!(matches!(next_create, Event::Create(..)));
    model.suffix.insert(1, next_create);
    model.suffix.insert(
        2,
        Event::Bind(ids.root, ids.next_literal, Value::Definition(ids.old)),
    );
    assert!(
        model.validate().is_none(),
        "creation must not alias two owners in one Result slot"
    );
}

#[test]
fn choice_graph_rejects_uncovered_branch_cleanup_points() {
    let (mut model, _) = fixture();
    let control = model.choice.as_ref().unwrap().id.control;
    let event = model
        .suffix
        .iter_mut()
        .find(|event| matches!(event, Event::Drop(..)))
        .unwrap();
    let Event::Drop(point, _, _) = event else {
        unreachable!()
    };
    *point = UnitDropPoint::BranchExit { control, branch: 1 };
    assert!(
        !model.is_consistent(),
        "V1 has no branch-exit cleanup proof contract yet"
    );
    let (mut model, ids) = fixture();
    let root_drop = model
        .suffix
        .iter()
        .position(|event| matches!(event, Event::Drop(_, root, _) if *root == ids.root))
        .unwrap();
    let drop = model.suffix.remove(root_drop);
    model.suffix.insert(2, drop);
    assert!(
        model.is_consistent(),
        "adjacent cleanup still conserves both selected paths"
    );
    let Event::Drop(point, _, _) = &mut model.suffix[2] else {
        unreachable!()
    };
    *point = UnitDropPoint::CallReturn(ids.call);
    assert!(
        model.is_consistent(),
        "a matching adjacent Replace proves installed Choice CallReturn cleanup"
    );
    for mutation in 0..6 {
        let mut invalid = model.clone();
        match mutation {
            0 => {
                let Event::Drop(point, _, _) = &mut invalid.suffix[2] else {
                    unreachable!()
                };
                *point = UnitDropPoint::CallReturn(control);
            }
            1 => {
                let Event::Drop(_, root, _) = &mut invalid.suffix[2] else {
                    unreachable!()
                };
                *root = ids.previous;
            }
            2 => {
                let Event::Drop(_, _, value) = &mut invalid.suffix[2] else {
                    unreachable!()
                };
                *value = Value::Definition(ids.old.clone());
            }
            3 => {
                invalid.suffix.swap(2, 3);
            } // Binding breaks the adjacent commit witness.
            4 => {
                invalid.suffix.remove(1);
            } // No real Replace.
            5 => {
                invalid.suffix.insert(3, invalid.suffix[2].clone());
            }
            _ => unreachable!(),
        }
        assert!(!invalid.is_consistent(), "CallReturn mutation {mutation}");
    }
}
