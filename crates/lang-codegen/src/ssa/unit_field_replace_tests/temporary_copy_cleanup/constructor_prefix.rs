//! Construction delivery prefixes stop at the exact source operand, not field order.
use super::*;

#[test]
fn constructor_field_prefix_stops_before_parent_and_suffix() {
    for exit in ["return 9", "stop()"] {
        for (fields, setup, arguments, prefix) in [
            ("val value: Int", "", exit.to_string(), false),
            (
                "val prefix: String, val value: Int, val tail: Int",
                "",
                format!("\"prefix\", {exit}, later()"),
                true,
            ),
            (
                "val prefix: String, val value: Int, val tail: Int",
                "val tag = 7; val input = \"prefix\";",
                format!("tail = tag, prefix = input, value = {exit}"),
                true,
            ),
            (
                "val prefix: String, val value: Int, val tail: Int",
                "",
                format!("value = {exit}, prefix = laterText(), tail = later()"),
                false,
            ),
        ] {
            let constructor = format!("Holder({arguments})");
            for receiver in [
                constructor.clone(),
                format!("({constructor})"),
                format!("(({constructor}))"),
            ] {
                let source = format!(
                    "class Holder({fields})\nfun stop(): Nothing = stop()\nfun later(): Int = 77\nfun laterText(): String = \"suffix\"\nfun entry(): Int {{ {setup} return {receiver}.value }}"
                );
                for constants in [false, true] {
                    let (result, routed) = refine::observe_routing(|| lower(&source, constants));
                    let (program, _) = result.expect("proven constructor prefix lowers");
                    assert!(
                        routed
                            .iter()
                            .any(|name| name.starts_with("koven.test.entry.d")),
                        "{routed:?}"
                    );
                    verify_program(&program).unwrap();
                    let function = entry(&program);
                    assert!(function.instructions.iter().all(|i| !matches!(
                        i.operation,
                        Operation::HeapAllocate { .. } | Operation::HeapFieldRead { .. }
                    )));
                    for instruction in &function.instructions {
                        if let Operation::DirectCall { callee, .. } = instruction.operation {
                            let called = program
                                .modules
                                .iter()
                                .flat_map(|m| &m.functions)
                                .find(|f| f.id == callee)
                                .unwrap();
                            assert!(
                                called.name.contains(".stop."),
                                "unreachable suffix called: {}",
                                called.name
                            );
                        }
                    }
                    let strings = function
                        .instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                        .collect::<Vec<_>>();
                    assert_eq!(strings.len(), usize::from(prefix));
                    let drops = function
                        .instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                        .collect::<Vec<_>>();
                    assert_eq!(drops.len(), usize::from(prefix && exit == "return 9"));
                    if prefix && exit == "return 9" {
                        let [EntityId::Value(owner)] = strings[0].results.as_slice() else {
                            panic!("prefix string owner");
                        };
                        assert_eq!(drops[0].operation, Operation::Drop { owner: *owner });
                    }
                    let returns = function
                        .blocks
                        .iter()
                        .filter_map(|b| b.terminator.as_ref())
                        .filter(|t| {
                            matches!(t.kind, crate::ssa::model::TerminatorKind::Return { .. })
                        })
                        .count();
                    assert_eq!(returns, usize::from(exit == "return 9"));
                    crate::llvm::render_verified_program(&program).unwrap();
                }
            }
        }
    }
}

#[test]
fn field_prefix_admission_does_not_authorize_standalone_construction() {
    for exit in ["return 9", "stop()"] {
        let source = format!(
            "class Holder(val prefix: String, val value: Int)\nfun stop(): Nothing = stop()\nfun entry(): Int {{ Holder(\"prefix\", {exit}); return 0 }}"
        );
        for constants in [false, true] {
            let (result, routed) = refine::observe_routing(|| lower(&source, constants));
            assert!(
                !routed
                    .iter()
                    .any(|name| name.starts_with("koven.test.entry.d")),
                "{routed:?}"
            );
            // Preserve the independently measured compatibility boundary, not a
            // language rule: unit_lower rejects a plan with a terminating operand.
            let error = result.err().expect("standalone compatibility boundary");
            assert_eq!(error.kind, LoweringErrorKind::MissingFact);
            let span = error.span.expect("construction rejection span");
            assert_eq!(
                &format!("package test\n{source}")[span.start()..span.end()],
                format!("Holder(\"prefix\", {exit})")
            );
        }
    }
}
