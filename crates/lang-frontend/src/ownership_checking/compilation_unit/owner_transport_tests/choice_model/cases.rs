//! Synthetic transport graphs use actual source-qualified identities and producer tokens.
//! The catalog's real source has a Unit If: these graphs do not assert typed String-If
//! admission. Production remains Unsupported for the real String-result consumers.
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
    let catalog = owned.owner_observations()[0].clone();
    assert_eq!(catalog.status(), UnitOwnerObservationStatus::Complete);
    assert!(catalog.is_consistent());
    let expr = |text| UnitExpressionId::new(fixture.source(0), fixture.expression(0, text));
    let old = literal_owner(&fixture, 0, &catalog, "\"old\"");
    let left = literal_owner(&fixture, 0, &catalog, "\"left\"");
    let right = literal_owner(&fixture, 0, &catalog, "\"right\"");
    let next = literal_owner(&fixture, 0, &catalog, "\"next\"");
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
        catalog,
        prefix: vec![
            Event::Create(expr("\"old\""), old.clone()),
            Event::Bind(root, expr("\"old\""), Value::Definition(old.clone())),
        ],
        choice,
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
            Event::Drop(forwarded, Value::Definition(old.clone())),
            Event::Drop(root, selected),
            Event::Create(expr("\"next\""), next.clone()),
            Event::Bind(incoming, expr("\"next\""), Value::Definition(next.clone())),
            Event::Drop(incoming, Value::Definition(next.clone())),
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
    let selected = Value::Choice(model.choice.id.clone());
    model
        .prefix
        .push(Event::Drop(ids.root, Value::Definition(ids.old)));
    model.suffix = vec![
        Event::Bind(ids.root, model.choice.id.control, selected.clone()),
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
        Event::Drop(ids.forwarded, selected),
        Event::Drop(ids.root, Value::Definition(ids.next)),
    ];
    assert!(model.validate().is_some());
}

#[test]
fn malformed_edges_inputs_and_identity_fail_closed() {
    let (model, ids) = fixture();
    let (foreign, foreign_ids) = fixture();
    let mut cases = Vec::new();
    let mut graph = model.clone();
    graph.choice.inputs.swap(0, 1);
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.inputs[0].owner = ids.right.clone();
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.inputs[0].tail = ids.old_literal;
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.inputs[0].owner = foreign_ids.left;
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.id.identity = foreign.choice.id.identity;
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.events[0].clear();
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.events[0].push(Event::Bind(
        ids.incoming,
        graph.choice.inputs[0].tail,
        Value::Definition(ids.left.clone()),
    ));
    graph.choice.events[0].push(Event::Drop(ids.incoming, Value::Definition(ids.left)));
    cases.push(graph);
    for (index, graph) in cases.iter().enumerate() {
        assert!(graph.validate().is_none(), "malformed input case {index}");
    }
}

#[test]
fn forward_choice_double_consumption_and_wrong_slots_fail_closed() {
    let (model, ids) = fixture();
    let selected = Value::Choice(model.choice.id.clone());
    let mut cases = Vec::new();
    let mut graph = model.clone();
    graph.prefix.push(Event::Bind(
        ids.incoming,
        model.choice.id.control,
        selected.clone(),
    ));
    cases.push(graph);
    let mut graph = model.clone();
    graph.choice.events[0].push(Event::Forward(
        graph.choice.inputs[0].tail,
        model.choice.id.control,
        selected.clone(),
    ));
    cases.push(graph); // Cyclic/self choice input is unavailable before the choice finishes.
    let mut graph = model.clone();
    graph.suffix.push(Event::Drop(ids.root, selected.clone()));
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
    graph
        .suffix
        .push(Event::Create(model.choice.inputs[0].tail, ids.left));
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
        model.choice.id.control,
        Value::Definition(ids.old.clone()),
    ));
    model.suffix.insert(
        1,
        Event::Bind(
            ids.root,
            model.choice.id.control,
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
    let (model, ids) = fixture();
    let sources = Fixture::new(&[("root.ko", SOURCE), ("other.ko", "fun other(): Unit {}")]);
    let owned = sources.shared();
    let other = owned
        .owner_observations()
        .iter()
        .find(|observation| observation.callable() != model.catalog.callable())
        .expect("second source callable")
        .callable();
    let mut graph = model.clone();
    graph.choice.id.callable = other;
    assert!(graph.validate().is_none());
    let mut graph = model.clone();
    graph.choice.inputs[0].tail =
        UnitExpressionId::new(sources.source(1), graph.choice.inputs[0].tail.expression());
    assert!(graph.validate().is_none());
    let mut graph = model.clone();
    let mut unknown = graph.choice.id.clone();
    unknown.control = ids.next_literal;
    graph.suffix[0] = Event::Pending(ids.call, graph.choice.id.control, Value::Choice(unknown));
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
