//! Real admitted sources, distinct catalog identities and exact control/cleanup boundaries.
use super::*;
use crate::ownership_checking::{
    CompilationUnitOwnership, OwnershipDeferredReason, UnitDropPoint, UnitDropTarget,
    UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin, UnitOwnerTransportEvent as E,
};
use crate::parser::{Expression, Item, NameMarker, Statement};
use crate::type_checking::UnitStatementId;
const SOURCE: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }"#;

fn declaration(f: &Fixture, file: usize, name: &str) -> (UnitStatementId, UnitExpressionId) {
    let ast = f.parsed[file].ast();
    let found: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(marker),
                initializer,
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            (f.sources.slice(*marker).unwrap() == name).then_some((
                UnitStatementId::new(f.source(file), id),
                UnitExpressionId::new(f.source(file), *initializer),
            ))
        })
        .collect();
    assert_eq!(found.len(), 1);
    found[0]
}
fn check(f: &Fixture, file: usize, owned: &CompilationUnitOwnership) {
    let o = &owned.owner_observations()[file];
    let value = o
        .replace_break_observation_v1()
        .expect("actual Replace then Break must publish separate P/Q exits");
    assert!(value.is_consistent());
    let ast = f.parsed[file].ast();
    let source = f.source(file);
    let controls: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::While {
                condition, body, ..
            } = node.payload()
            else {
                return None;
            };
            Some((
                UnitStatementId::new(source, id),
                UnitExpressionId::new(source, *condition),
                UnitStatementId::new(source, *body),
            ))
        })
        .collect();
    assert_eq!(controls.len(), 1);
    let (statement, condition, body) = controls[0];
    let jump = UnitExpressionId::new(source, f.expression(file, "break"));
    let held = f.local(file, "held");
    let previous = f.local(file, "previous");
    let retired = f.local(file, "retired");
    let (_, initial) = declaration(f, file, "held");
    let (previous_statement, call) = declaration(f, file, "previous");
    let call_node = ast.expressions().get(call.expression()).unwrap();
    let Expression::Call { arguments, .. } = call_node.payload() else {
        panic!("real call")
    };
    assert_eq!(arguments.len(), 2);
    let argument = UnitExpressionId::new(source, arguments[1].value);
    assert_eq!(
        (
            value.statement(),
            value.condition(),
            value.body(),
            value.break_expression()
        ),
        (statement, condition, body, jump)
    );
    assert_eq!(value.callable(), o.callable());
    let p = value.entry().owner();
    let q = value.break_boundary().owner();
    assert_ne!(p, q);
    assert_eq!(
        (p.index, q.index),
        (0, 0),
        "indices alone cannot identify these owners"
    );
    assert_eq!(value.input_producers().definitions().len(), 1);
    assert_eq!(value.body_producers().definitions().len(), 1);
    assert_eq!(
        value
            .input_producers()
            .definition(p.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(initial)
    );
    assert_eq!(
        value
            .body_producers()
            .definition(q.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(argument)
    );
    assert!(value.input_producers().definition(q.clone()).is_none());
    assert!(value.body_producers().definition(p.clone()).is_none());
    assert_eq!(value.entry(), value.condition_false_exit());
    assert_eq!(value.break_boundary(), value.body_completion());
    for snapshot in [
        value.entry(),
        value.condition_false_exit(),
        value.break_boundary(),
        value.body_completion(),
    ] {
        assert_eq!(snapshot.symbol(), held);
        assert_eq!(snapshot.scope_depth(), 1);
        assert_eq!(snapshot.declaration(), value.entry().declaration());
        assert_eq!(f.sources.slice(snapshot.declaration()).unwrap(), "held");
    }
    assert_eq!(value.entry().origin(), value.entry().declaration());
    assert_eq!(value.break_boundary().origin(), call_node.span());
    assert_ne!(value.entry().origin(), value.break_boundary().origin());
    assert_eq!(
        value.body_events(),
        &[
            E::Result {
                expression: argument,
                owner: q.clone()
            },
            E::Pending {
                call,
                argument,
                owner: q.clone()
            },
            E::Replace {
                call,
                root: held,
                argument,
                previous: p.clone(),
                incoming: q
            },
            E::Result {
                expression: call,
                owner: p.clone()
            },
            E::Binding {
                symbol: previous,
                initializer: call,
                owner: p.clone()
            },
            E::Drop {
                point: UnitDropPoint::AfterStatement(previous_statement),
                symbol: previous,
                owner: p
            },
        ]
    );
    assert_eq!(
        value.edges(),
        &[
            Edge::EnterBody(condition, body),
            Edge::BreakExit(
                UnitDropPoint::ControlTransfer(jump),
                UnitDropPoint::LoopExit(statement)
            ),
            Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(statement)),
        ]
    );
    let (suffix, _) = declaration(f, file, "retired");
    let actual: Vec<_> = owned
        .drops()
        .iter()
        .filter(|d| matches!(d.target(), UnitDropTarget::Named(s) if s.source_unit() == source))
        .map(|d| (d.point(), d.target()))
        .collect();
    assert_eq!(
        actual,
        vec![
            (
                UnitDropPoint::AfterStatement(previous_statement),
                UnitDropTarget::Named(previous)
            ),
            (
                UnitDropPoint::AfterStatement(suffix),
                UnitDropTarget::Named(retired)
            )
        ]
    );
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.target() != UnitDropTarget::Named(held))
    );
    assert!(o.definitions().is_empty() && o.events().is_empty());
    assert!(o.value_observation_v1().is_none() && o.loop_observation_v1().is_none());
    assert!(
        o.invariant_loop_observation_v1().is_none() && o.invariant_break_observation_v1().is_none()
    );
    assert!(
        o.invariant_split_continue_observation_v1().is_none()
            && o.invariant_split_exit_observation_v1().is_none()
    );
    assert!(
        o.invariant_normal_join_observation_v1().is_none()
            && o.invariant_mixed_join_observation_v1().is_none()
    );
}
#[test]
fn actual_replace_break_preserves_distinct_exit_owners() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    check(&fixture, 0, &owned);
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn replace_break_keeps_actual_source_and_analysis_identities() {
    for explicit in [false, true] {
        for same_text in [false, true] {
            let mut first = SOURCE.to_owned();
            if explicit {
                first = first.replace("flag: Boolean)", "flag: Boolean): Unit");
            }
            if same_text {
                first = first.replace("\"next\"", "\"outer\"");
            }
            let second = first.replacen("fun run(", "fun other(", 1);
            let fixture = Fixture::new(&[("a.ko", &first), ("b.ko", &second)]);
            let first_analysis = fixture.shared();
            let repeated = fixture.shared();
            for owned in [&first_analysis, &repeated] {
                assert!(owned.clone().validate().is_ok());
                for file in 0..2 {
                    check(&fixture, file, owned);
                }
            }
            for file in 0..2 {
                let a = first_analysis.owner_observations()[file]
                    .replace_break_observation_v1()
                    .unwrap();
                for other in [
                    repeated.owner_observations()[file]
                        .replace_break_observation_v1()
                        .unwrap(),
                    first_analysis.owner_observations()[1 - file]
                        .replace_break_observation_v1()
                        .unwrap(),
                ] {
                    for token in [a.entry().owner(), a.break_boundary().owner()] {
                        assert!(other.input_producers().definition(token.clone()).is_none());
                        assert!(other.body_producers().definition(token).is_none());
                    }
                }
            }
            for (path, text) in [("a.ko", first.as_str()), ("b.ko", second.as_str())] {
                let f = Fixture::new(&[(path, text)]);
                let owned = f.shared();
                check(&f, 0, &owned);
                compare(&f, &f.parts(&owned));
            }
        }
    }
}
#[test]
fn replace_break_rejects_extra_paths_and_revokes_global_failures() {
    let cases = [
        SOURCE.replace("; break", ""),
        SOURCE.replace("break", "continue"),
        SOURCE.replace("break", "(break)"),
        SOURCE.replace("while(flag)", "while((flag))"),
        SOURCE.replace("while(flag)", "while(!flag)"),
        SOURCE.replace("while(flag)", "while(true)"),
        SOURCE.replace("flag: Boolean", "inout flag: Boolean"),
        SOURCE.replace("flag: Boolean", "flag: Boolean, unused: Boolean"),
        SOURCE.replace("\"next\"", "(\"next\")"),
        SOURCE.replace("&held", "&(held)"),
        SOURCE.replace("retired = held", "retired = (held)"),
        SOURCE.replace("val previous", "var previous"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace(
            "var held = \"outer\"",
            "val seed = \"outer\"; var held = seed",
        ),
        SOURCE.replace("; break", "; val extra = \"extra\"; break"),
        SOURCE.replace("break", "if(flag) { break } else { break }"),
        format!(
            "{}\nfun make(): String = \"opaque\"",
            SOURCE.replace("\"next\"", "make()")
        ),
    ];
    for text in cases {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.owner_observations()[0]
                .replace_break_observation_v1()
                .is_none(),
            "{text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
    for separate in [false, true] {
        let bad = "fun bad(input: String) { val moved = input }";
        let combined = format!("{SOURCE}\n{bad}");
        let files = if separate {
            vec![("a.ko", SOURCE), ("b.ko", bad)]
        } else {
            vec![("a.ko", combined.as_str())]
        };
        let fixture = Fixture::new(&files);
        let owned = fixture.shared();
        assert!(!owned.diagnostics().is_empty() && owned.clone().validate().is_err());
        assert!(owned.owner_observations().is_empty());
    }
}

// Deliberate damaged-model and deferred-publication guards: compile locally, run only in CI.
#[test]
fn replace_break_model_rejects_damaged_states_and_catalogs() {
    let f = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = f.shared();
    let repeated = f.shared();
    let original = owned.owner_observations()[0]
        .replace_break_observation_v1()
        .unwrap();
    for variant in 0..8 {
        let mut damaged = original.clone();
        match variant {
            0 => damaged.post_break.owner = damaged.entry.owner.clone(),
            1 => damaged.condition_false = damaged.post_break.clone(),
            2 => damaged.body_completion.origin = damaged.entry.origin,
            3 => {
                damaged.events.remove(1);
            }
            4 => {
                damaged.events.pop();
            }
            5 => {
                damaged.producers = repeated.owner_observations()[0]
                    .replace_break_observation_v1()
                    .unwrap()
                    .producers
                    .clone()
            }
            6 => {
                damaged.jump = damaged.argument;
                damaged.edges = crate::ownership_checking::compilation_unit::owner_transport::LoopBodyCompletion::Break(damaged.jump).edges(damaged.statement, damaged.condition, damaged.body);
            }
            _ => damaged.completed_identity = damaged.identity.clone(),
        }
        assert!(!damaged.is_consistent(), "variant {variant}");
    }
    let closed = Fixture::new(&[(
        "closed.ko",
        r#"fun run(flag: Boolean) { while(flag) { val local = "inner" } }"#,
    )])
    .shared();
    let mut polluted = closed.owner_observations()[0].clone();
    assert!(polluted.loop_observation_v1().is_some());
    polluted.replace_break_value = owned.owner_observations()[0].replace_break_value.clone();
    assert!(
        polluted.loop_observation_v1().is_none()
            && polluted.replace_break_observation_v1().is_none()
    );
}
#[test]
fn replace_break_deferred_publication_withholds_the_complete_product() {
    let f = Fixture::new(&[("root.ko", SOURCE)]);
    let baseline = f.shared();
    let parts = f.parts(&baseline);
    let mut raw = f.dataflow(&parts);
    assert!(raw.deferred.is_empty());
    assert!(
        raw.owner_observations[0]
            .replace_break_observation_v1()
            .is_some()
    );
    raw.deferred.push(UnitOwnershipDeferredFact::new(
        UnitExpressionId::new(f.source(0), f.expression(0, "flag")),
        OwnershipDeferredReason::MemberReceiver,
    ));
    let owned = CompilationUnitOwnership::new(
        f.typed.types(),
        parts.bindings.into_values().collect(),
        parts.arguments,
        parts.receivers,
        parts.capture,
        raw,
    );
    assert!(!owned.deferred().is_empty());
    assert_eq!(owned.owner_observations().len(), 1);
    assert!(
        owned.owner_observations()[0]
            .replace_break_observation_v1()
            .is_none()
    );
    assert!(owned.owner_observations()[0].replace_break_value.is_none());
}
