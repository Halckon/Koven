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
fn standalone_constructor_prefix_has_no_parent_value() {
    for exit in ["return 9", "stop()"] {
        for (setup, arguments, prefix) in [
            (
                "",
                format!("value = {exit}, prefix = laterText(), tail = later()"),
                false,
            ),
            ("", format!("\"prefix\", {exit}, later()"), true),
            (
                "val tag = 7; val input = \"prefix\";",
                format!("tail = tag, prefix = input, value = {exit}"),
                true,
            ),
        ] {
            let constructor = format!("Holder({arguments})");
            for expression in [constructor.clone(), format!("(({constructor}))")] {
                for body in [
                    format!("{expression}; return 0"),
                    format!("val unused = {expression}; return 0"),
                    format!("return {expression}.value"),
                ] {
                    let source = format!(
                        "class Holder(val prefix: String, val value: Int, val tail: Int)\nfun stop(): Nothing = stop()\nfun later(): Int = 77\nfun laterText(): String = \"suffix\"\nfun entry(): Int {{ {setup} {body} }}"
                    );
                    for constants in [false, true] {
                        let (result, routed) =
                            refine::observe_routing(|| lower(&source, constants));
                        let (program, _) = result.expect("proven standalone construction prefix");
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
                                panic!("prefix owner")
                            };
                            assert_eq!(drops[0].operation, Operation::Drop { owner: *owner });
                        }
                        for instruction in &function.instructions {
                            if let Operation::DirectCall { callee, .. } = instruction.operation {
                                assert!(
                                    program
                                        .modules
                                        .iter()
                                        .flat_map(|m| &m.functions)
                                        .find(|f| f.id == callee)
                                        .unwrap()
                                        .name
                                        .contains(".stop.")
                                );
                            }
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
}

#[test]
fn constructor_return_prefix_returns_only_an_existing_owner() {
    for (exit, returns) in [("return fallback", true), ("stop()", false)] {
        let source = format!(
            "class Holder(val prefix: String, val value: Int)\nfun stop(): Nothing = stop()\nfun entry(own fallback: Holder): Holder {{ return Holder(\"prefix\", {exit}) }}"
        );
        for constants in [false, true] {
            let (result, routed) = refine::observe_routing(|| lower(&source, constants));
            let (program, _) = result.expect("constructor return prefix");
            assert!(
                routed
                    .iter()
                    .any(|name| name.starts_with("koven.test.entry.d"))
            );
            verify_program(&program).unwrap();
            let function = entry(&program);
            assert!(function.instructions.iter().all(|i| !matches!(
                i.operation,
                Operation::HeapAllocate { .. } | Operation::HeapFieldRead { .. }
            )));
            let string = function
                .instructions
                .iter()
                .find(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                .unwrap();
            let [EntityId::Value(prefix)] = string.results.as_slice() else {
                panic!("prefix owner")
            };
            let drops = function
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                .collect::<Vec<_>>();
            let [EntityId::Value(fallback)] = function.blocks[0].parameters.as_slice() else {
                panic!("existing parameter owner")
            };
            assert_eq!(drops.len(), 1);
            if returns {
                assert_eq!(drops[0].operation, Operation::Drop { owner: *prefix });
            } else {
                // The unused parameter is released at function entry, before the
                // constructor prefix exists; Abort does not release that prefix.
                assert_eq!(drops[0].operation, Operation::Drop { owner: *fallback });
                assert_eq!(function.instructions[0].id, drops[0].id);
                assert_ne!(fallback, prefix);
            }
            let actual_returns = function
                .blocks
                .iter()
                .filter_map(|b| b.terminator.as_ref())
                .filter(|t| matches!(t.kind, crate::ssa::model::TerminatorKind::Return { .. }))
                .count();
            assert_eq!(actual_returns, usize::from(returns));
            for term in function
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
            {
                if let crate::ssa::model::TerminatorKind::Return { values } = &term.kind {
                    assert_eq!(values.as_slice(), &[*fallback]);
                }
            }
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

#[test]
fn terminated_non_class_construction_does_not_gain_class_routing() {
    for expression in [
        "Point(return 9)",
        "Box<Point>(return 9)",
        "Rc<Int>(return 9)",
    ] {
        let source = format!(
            "value class Point(val value: Int)\nfun entry(): Int {{ {expression}; return 0 }}"
        );
        for constants in [false, true] {
            let (_, routed) = refine::observe_routing(|| lower(&source, constants));
            assert!(
                !routed
                    .iter()
                    .any(|name| name.starts_with("koven.test.entry.d")),
                "{expression}: {routed:?}"
            );
        }
    }
}
