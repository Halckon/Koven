//! Ordinary finite sources and real SSA edge tracing; no mutation or foreign products.
use super::{OwnedProducts, UnitSource};
use crate::ssa::model::{Definition, Edge, EntityId, Function, Operation, TerminatorKind};
use std::collections::BTreeSet;
pub(super) const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag && flag) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
fn edges(term: &TerminatorKind) -> Vec<&Edge> {
    match term {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => vec![],
        _ => panic!("only normal finite Boolean CFG expected"),
    }
}
// Follow actual phi arguments backwards. No expected AST edge numbering is involved.
fn origins(
    f: &Function,
    value: EntityId,
    p: EntityId,
    q: EntityId,
    visiting: &mut BTreeSet<EntityId>,
) -> u8 {
    if value == p {
        return 1;
    }
    if value == q {
        return 2;
    }
    assert!(
        visiting.insert(value),
        "no loop backedge in the bare-Break source"
    );
    let Definition::BlockParameter { block, index } = f.entity(value).unwrap().definition else {
        panic!("owner transport must be the actual producer or an actual block parameter")
    };
    let mut incoming = 0;
    let mut count = 0;
    for predecessor in &f.blocks {
        for edge in edges(&predecessor.terminator.as_ref().unwrap().kind) {
            if edge.target == block {
                count += 1;
                incoming |= origins(f, edge.arguments[index], p, q, visiting);
            }
        }
    }
    assert!(count > 0);
    visiting.remove(&value);
    incoming
}
#[test]
fn short_circuit_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("short-circuit-owner.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, events) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [event] = events.as_slice() else {
        panic!("one actual Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual short-circuit Return needs P/Q authorization"
    );
}
#[test]
fn short_circuit_real_cfg_preserves_p_before_two_owner_exits() {
    check_cfg(&[
        "flag && flag",
        "flag || flag",
        "!((flag) && (flag))",
        "!((flag) || (flag))",
    ]);
}
pub(super) fn check_cfg(conditions: &[&str]) {
    for condition in conditions {
        for constant in [false, true] {
            let text = SOURCE.replace("flag && flag", condition);
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                )
            } else {
                text
            };
            let source = UnitSource::analyze("short-circuit-owner-cfg.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            let ((lowered, events), actions) = crate::ssa::observe_owner_actions(|| {
                crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
            });
            let (program, entry) = lowered.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let [event] = events.as_slice() else {
                panic!("one actual Return")
            };
            let slot = event.2.unwrap();
            assert_eq!(event.1, Some(slot));
            assert_eq!(actions, [("root", slot), ("take", slot)]);
            let functions: Vec<_> = program.modules.iter().flat_map(|m| &m.functions).collect();
            let named = |prefix: &str| {
                *functions
                    .iter()
                    .find(|f| {
                        source
                            .sources
                            .slice(f.origin.span())
                            .is_ok_and(|s| s.starts_with(prefix))
                    })
                    .unwrap()
            };
            let run = named("fun run(");
            let caller = named("fun entry(");
            assert_eq!(
                run.blocks
                    .iter()
                    .filter(|b| matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Conditional { .. }
                    ))
                    .count(),
                2,
                "both real Boolean reads remain in the short-circuit CFG"
            );
            let literals: Vec<_> = run
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                .collect();
            assert_eq!(literals.len(), 2);
            let p = literals.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"outer")).unwrap().results[0];
            let literal_q = literals.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"next")).unwrap().results[0];
            let replacements: Vec<_> = run
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
                .collect();
            let [replacement] = replacements.as_slice() else {
                panic!("one actual Replace")
            };
            let Operation::RootReplace {
                owner,
                replacement: rhs,
                ..
            } = replacement.operation
            else {
                unreachable!()
            };
            let q = replacement.results[0];
            let trace = |v| origins(run, v, p, q, &mut BTreeSet::new());
            assert_eq!(trace(EntityId::Value(owner)), 1);
            assert_eq!(EntityId::Value(rhs), literal_q);
            let returns: Vec<_> = run
                .blocks
                .iter()
                .filter_map(|b| match &b.terminator.as_ref()?.kind {
                    TerminatorKind::Return { values } => Some((b, values)),
                    _ => None,
                })
                .collect();
            let [(exit, values)] = returns.as_slice() else {
                panic!("one owner exit")
            };
            assert_eq!(values.len(), 1);
            assert_eq!(trace(EntityId::Value(values[0])), 3);
            let incoming: Vec<_> = run
                .blocks
                .iter()
                .flat_map(|b| {
                    edges(&b.terminator.as_ref().unwrap().kind)
                        .into_iter()
                        .filter(move |e| e.target == exit.id)
                        .map(move |e| (b, e))
                })
                .collect();
            assert_eq!(
                incoming.len(),
                2,
                "internal condition edges must converge before the owner exit"
            );
            let mut roles = Vec::new();
            for (block, edge) in incoming {
                assert!(matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Branch(_)
                ));
                assert_eq!(edge.arguments.len(), 1);
                roles.push(trace(edge.arguments[0]));
            }
            roles.sort_unstable();
            assert_eq!(roles, [1, 2]);
            let p_type = run.entity(p).unwrap().ty;
            for block in &run.blocks {
                for parameter in &block.parameters {
                    if run.entity(*parameter).unwrap().ty == p_type && block.id != exit.id {
                        assert_eq!(
                            trace(*parameter),
                            1,
                            "all pre-exit owner phi inputs remain actual P"
                        );
                    }
                }
            }
            let drops: Vec<_> = run
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some((i.block, EntityId::Value(owner))),
                    _ => None,
                })
                .collect();
            assert_eq!(drops, [(replacement.block, replacement.results[1])]);
            let calls: Vec<_> = caller.instructions.iter().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id())).collect();
            let drops: Vec<_> = caller
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some(EntityId::Value(owner)),
                    _ => None,
                })
                .collect();
            assert_eq!(calls.len(), 2);
            assert_eq!(drops.len(), 2);
            for call in calls {
                assert_eq!(call.results.len(), 1);
                assert_eq!(drops.iter().filter(|v| **v == call.results[0]).count(), 1);
            }
        }
    }
}
#[test]
fn short_circuit_native_preserves_skip_and_evaluated_results() {
    check_native(&[
        ("flag && flag", b"outer\nnext\n".as_slice()),
        ("flag || flag", b"outer\nnext\n".as_slice()),
        ("!((flag) && (flag))", b"next\nouter\n".as_slice()),
        ("!((flag) || (flag))", b"next\nouter\n".as_slice()),
    ]);
}
pub(super) fn check_native(cases: &[(&str, &[u8])]) {
    for (condition, expected) in cases {
        for constant in [false, true] {
            let text = SOURCE.replace("flag && flag", condition).replace(
                "val first = run(false); val second = run(true)",
                "val first = run(false); println(first); val second = run(true); println(second)",
            );
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                )
            } else {
                text
            };
            let source = UnitSource::analyze("short-circuit-owner-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            super::assert_native_output(&source, expected);
        }
    }
}
