//! Statement-context Unit must not erase the producer's control-flow result.
use super::*;

#[test]
fn unit_statement_when_completion_is_source_qualified_and_survives_publication() {
    let mut sources = SourceMap::new();
    let (total_id, total) = parsed(
        &mut sources,
        "total.ko",
        "package a\nenum class Flag { One, Two }\nfun choose(flag: Flag): Int { when(flag) { is Flag.One -> return 7; is Flag.Two -> return 8 } }",
    );
    let (partial_id, partial) = parsed(
        &mut sources,
        "partial.ko",
        "package b\nenum class Flag { One, Two }\nfun choose(flag: Flag): Int { when(flag) { is Flag.One -> return 7; is Flag.Two -> (8) }; return 9 }",
    );
    let total_when = when_expressions(&total)[0];
    let partial_when = when_expressions(&partial)[0];
    assert_eq!(
        total_when, partial_when,
        "colliding local IDs must not share a flow fact"
    );
    for reverse in [false, true] {
        let mut inputs = vec![
            SourceUnitInput::new("root", "a/total.ko", total_id, &total),
            SourceUnitInput::new("root", "b/partial.ko", partial_id, &partial),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let names = validated_names(&sources, &inputs, &environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        for (source, expression, falls_through) in [
            (total_id, total_when, false),
            (partial_id, partial_when, true),
        ] {
            let id = UnitExpressionId::new(source_unit(&names, source), expression);
            let ty = typed.expression_type(id).unwrap();
            assert_eq!(
                typed.types().get(ty),
                Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            );
            assert_eq!(typed.expression_falls_through(id), Some(falls_through));
            assert_eq!(
                typed.clone().expression_falls_through(id),
                Some(falls_through)
            );
        }
    }
}

#[test]
fn unit_when_selector_exit_preserves_earlier_match_and_stops_the_miss_path() {
    for (control, falls_through) in [
        ("when (flag) { true, pred(return 7) -> return 2 }", false),
        ("when { flag, pred(return 7) -> return 2 }", false),
        ("when (flag) { true, pred(return 7) -> 2 }", true),
        ("when { flag, pred(return 7) -> 2 }", true),
        ("when (flag) { true, pred(7) -> return 2 }", true),
        ("when { flag, pred(7) -> return 2 }", true),
        ("when { pred(return 7) -> 2; else -> 3 }", false),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "when-selector-completion.ko",
            &format!(
                "fun pred(value: Int): Boolean = value == 1\nfun choose(flag: Boolean): Int {{ {control} }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "when-selector-completion.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let id = UnitExpressionId::new(source_unit(&names, source), when_expressions(&file)[0]);
            assert_eq!(
                typed.expression_falls_through(id),
                Some(falls_through),
                "{control}"
            );
            let diagnostics: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            let expected = if falls_through {
                ["L0088"].as_slice()
            } else {
                [].as_slice()
            };
            assert_eq!(diagnostics, expected, "{control}");
        }
    }
}

#[test]
fn unit_when_selector_exit_publishes_nothing_after_branch_type_checking() {
    for (arms, diagnostics) in [
        ("pred(return 7) -> 2; else -> 3", [].as_slice()),
        (
            "pred(return 7) -> 2; else -> \"mixed\"",
            ["L0112"].as_slice(),
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "when-selector-result.ko",
            &format!(
                "fun pred(value: Int): Boolean = value == 1
                 fun choose(): Int {{ val result = when {{ {arms} }}; return 0 }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "when-selector-result.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let actual: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            assert_eq!(actual, diagnostics, "{arms}");
            if diagnostics.is_empty() {
                let id =
                    UnitExpressionId::new(source_unit(&names, source), when_expressions(&file)[0]);
                assert_eq!(typed.expression_falls_through(id), Some(false));
                let ty = typed.expression_type(id).expect("when result");
                assert_eq!(
                    typed.types().get(ty),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
                );
            }
        }
    }
}

#[test]
fn unit_loop_completion_requires_a_structurally_reachable_local_break() {
    for (body, has_exit, expected) in [
        ("loop {}", false, [].as_slice()),
        ("loop { continue; break }", false, [].as_slice()),
        ("loop { return 7; break }", false, [].as_slice()),
        ("loop { loop { break }; continue }", false, [].as_slice()),
        ("loop { loop {}; break }", false, [].as_slice()),
        ("loop { if (return 7) { break } }", false, [].as_slice()),
        (
            "loop { if (flag && ((return 7) == 0)) { break } }",
            false,
            [].as_slice(),
        ),
        (
            "loop { when { pred(return 7) -> break; else -> break } }",
            false,
            [].as_slice(),
        ),
        ("loop { while (return 7) {}; break }", false, [].as_slice()),
        (
            "loop { while (flag) {}; break }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { while (flag || ((return 7) == 0)) {}; break }",
            false,
            [].as_slice(),
        ),
        (
            "loop { while (flag || ((return 7) == 0)) { break }; break }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { if (flag && ((return 7) == 0)) { 0 } else { continue }; break }",
            false,
            [].as_slice(),
        ),
        ("loop { sink(return 7, break) }", false, [].as_slice()),
        ("loop { sink(break, 7) }", true, ["L0088"].as_slice()),
        ("loop { break }", true, ["L0088"].as_slice()),
        ("loop { break }; return 9", true, [].as_slice()),
        (
            "loop { if (flag) { break }; continue }",
            true,
            ["L0088"].as_slice(),
        ),
        // Ordinary completion keeps both Boolean edges. Const can use a narrower plan.
        ("loop { if (false) { break } }", true, ["L0088"].as_slice()),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "loop-completion.ko",
            &format!(
                "fun pred(value: Int): Boolean = value == 1
                 fun sink(first: Int, second: Int): Unit {{}}
                 fun choose(flag: Boolean): Int {{ {body} }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "loop-completion.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let actual: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            assert_eq!(actual, expected, "{body}");
            let (outer, _) = file
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Statement::Loop { .. }
                    )
                })
                .last()
                .expect("outer loop");
            let id = lang_frontend::type_checking::UnitStatementId::new(
                source_unit(&names, source),
                outer,
            );
            assert_eq!(typed.loop_falls_through(id), Some(has_exit), "{body}");
        }
    }
}

#[test]
fn construction_operands_preserve_reachable_loop_exits() {
    for (body, has_exit, expected) in [
        ("loop { sink(return 7, break) }", false, [].as_slice()),
        ("loop { Pair(return 7, break) }", false, [].as_slice()),
        ("loop { Pair(7, break) }", true, ["L0088"].as_slice()),
        ("loop { Pair(continue, break) }", false, [].as_slice()),
        ("loop { Pair(break, return 7) }", true, ["L0088"].as_slice()),
        (
            "loop { Pair(second = return 7, first = break) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Pair(second = 7, first = break) }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { Generic(\"tag\", return 7, break) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Generic<String>(\"tag\", return 7, break) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Generic(\"tag\", 7, break) }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { Generic<String>(\"tag\", 7, break) }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { Pair(if (flag) { return 7 } else { 8 }, break) }",
            true,
            ["L0088"].as_slice(),
        ),
        (
            "loop { Pair(if (flag) { return 7 } else { return 8 }, break) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Pair(return 7, if (flag) { loop { break }; 8 } else { 9 }) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Pair(return 7, \"bad\") }",
            false,
            ["L0084"].as_slice(),
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "loop-completion.ko",
            &format!(
                "class Pair(val first: Int, val second: Int)
                 class Generic<T>(val tag: T, val first: Int, val second: Int)
                 fun pred(value: Int): Boolean = value == 1
                 fun sink(first: Int, second: Int): Unit {{}}
                 fun choose(flag: Boolean): Int {{ {body} }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "loop-completion.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let actual: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            assert_eq!(actual, expected, "{body}");
            if expected == ["L0084"] {
                assert_eq!(
                    sources.slice(typed.diagnostics()[0].primary_span()),
                    Ok("\"bad\"")
                );
            }
            let (outer, _) = file
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Statement::Loop { .. }
                    )
                })
                .last()
                .expect("outer loop");
            let id = lang_frontend::type_checking::UnitStatementId::new(
                source_unit(&names, source),
                outer,
            );
            assert_eq!(typed.loop_falls_through(id), Some(has_exit), "{body}");
        }
    }
}

#[test]
fn rejected_construction_storage_keeps_loop_completion() {
    for (body, has_exit, expected) in [
        (
            "loop { Marker<Int>(return 7, break) }",
            false,
            [].as_slice(),
        ),
        (
            "loop { Marker<Nothing>(return 7, break) }",
            false,
            ["L0125"].as_slice(),
        ),
        (
            "loop { Marker<Nothing>(7, break) }",
            true,
            ["L0125", "L0088"].as_slice(),
        ),
        ("loop { Marker<Int>(7, break) }", true, ["L0088"].as_slice()),
        (
            "loop { Marker<Nothing>(second = return 7, first = break) }",
            false,
            ["L0125"].as_slice(),
        ),
        (
            "loop { Marker<Nothing>(7, 8) }",
            false,
            ["L0125"].as_slice(),
        ),
        (
            "loop { Marker<Nothing>(return 7, \"bad\") }",
            false,
            ["L0125"].as_slice(),
        ),
        (
            "loop { Marker<Nothing>(return 7, outside()) }",
            false,
            ["L0125", "L0165"].as_slice(),
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "loop-completion.ko",
            &format!(
                "interface Pattern<T>
                 class Marker<T>(val first: Int, val second: Int) : Pattern<Rc<T>>
                 fun choose(flag: Boolean): Int {{ {body} }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "loop-completion.ko", source, &file),
        ] {
            let inputs = [input];
            let (mut name_environment, type_environment) = standard_environments();
            name_environment.declare_function("outside").unwrap();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let actual: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            // Unbound external signatures are diagnosed by final admission, not raw checking.
            let raw_expected: Vec<_> = expected
                .iter()
                .copied()
                .filter(|code| *code != "L0165")
                .collect();
            assert_eq!(actual, raw_expected, "{body}");
            if body.contains("outside()") {
                use lang_frontend::type_checking::{
                    FinalTypeAdmissionError, complete_compilation_unit_types,
                };
                let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
                    complete_compilation_unit_types(
                        &sources,
                        &inputs,
                        &names,
                        &type_environment,
                        &typed,
                    )
                else {
                    panic!("unbound suffix must fail final admission")
                };
                let final_codes: Vec<_> =
                    diagnostics.iter().map(|d| d.code().to_string()).collect();
                assert_eq!(final_codes, expected);
                assert_eq!(sources.slice(diagnostics[1].primary_span()), Ok("outside"));
            }
            for diagnostic in typed.diagnostics() {
                match diagnostic.code().to_string().as_str() {
                    "L0125" => assert_eq!(sources.slice(diagnostic.primary_span()), Ok("Nothing")),
                    "L0165" => assert_eq!(sources.slice(diagnostic.primary_span()), Ok("outside")),
                    "L0088" => {}
                    code => panic!("unexpected diagnostic {code}"),
                }
            }
            if body.contains("Marker<Nothing>") {
                assert!(
                    typed.constructions().is_empty(),
                    "rejected storage publishes no construction"
                );
            }
            let (outer, _) = file
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Statement::Loop { .. }
                    )
                })
                .last()
                .expect("outer loop");
            let id = lang_frontend::type_checking::UnitStatementId::new(
                source_unit(&names, source),
                outer,
            );
            assert_eq!(typed.loop_falls_through(id), Some(has_exit), "{body}");
        }
    }
}

#[test]
fn unit_interpolation_completion_follows_the_evaluated_prefix() {
    let cases = [
        (r#"sink("before ${1} ${return 7} ${2}")"#, false, &[][..]),
        (
            r#"sink("before ${if (flag) { return 7 } else { 1 }} ${2}")"#,
            true,
            &["L0088"][..],
        ),
        (r#""${return 7}${true + 1}""#, false, &["L0085"][..]),
        (r#"loop { sink("${return 7}${break}") }"#, false, &[][..]),
        (
            r#"loop { sink("${break}${return 7}") }"#,
            false,
            &["L0088"][..],
        ),
    ];
    for (body, completes, expected_diagnostics) in cases {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "interpolation.ko",
            &format!(
                "fun sink(value: String): Unit {{}}\nfun probe(flag: Boolean): Int {{ {body} }}"
            ),
        );
        let expression = file
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), Expression::String { .. }).then_some(id)
            })
            .expect("the source contains an interpolated string");
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "interpolation.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let id = UnitExpressionId::new(source_unit(&names, source), expression);
            let codes = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(codes, expected_diagnostics, "{body}");
            assert_eq!(
                typed.expression_falls_through(id),
                Some(completes),
                "{body}"
            );
            assert_eq!(
                typed
                    .types()
                    .get(typed.expression_type(id).expect("static string type")),
                Some(&UnitTypeKind::Builtin(BuiltinType::String)),
                "{body}"
            );
        }
    }
}

#[test]
fn unit_ownership_primitive_completion_follows_its_operands() {
    for (body, falls_through, result_type, expected) in [
        (
            r#"var slot = "old"; replace(&slot, return source)"#,
            false,
            BuiltinType::String,
            &[][..],
        ),
        (
            r#"var slot = "old"; replace(&slot, if (flag) { return source } else { "new" })"#,
            true,
            BuiltinType::String,
            &["L0088"][..],
        ),
        (
            "val a = arrayOf<Int>(0); val b = arrayOf<Int>(1); swap(&a[return source], &b[0])",
            false,
            BuiltinType::Unit,
            &[][..],
        ),
        (
            "val a = arrayOf<Int>(0); val b = arrayOf<Int>(1); swap(&a[0], &b[0]); return source",
            true,
            BuiltinType::Unit,
            &[][..],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "ownership-primitive.ko",
            &format!(
                "fun probe(source: String, flag: Boolean): borrow String from source {{ {body} }}"
            ),
        );
        let (call, _) = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
            .last()
            .expect("outer ownership primitive call");
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "ownership-primitive.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let codes = typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(codes, expected, "{body}");
            let id = UnitExpressionId::new(source_unit(&names, source), call);
            assert_eq!(typed.expression_falls_through(id), Some(falls_through));
            assert_eq!(
                typed
                    .types()
                    .get(typed.expression_type(id).expect("static primitive result")),
                Some(&UnitTypeKind::Builtin(result_type)),
            );
            assert_eq!(typed.ownership_primitive(id).is_some(), expected.is_empty());
        }
    }
}
