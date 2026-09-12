use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{
    LoweringErrorKind,
    model::{EntityType, Operation, ScalarConstant, SsaTypeKind},
    render::render_program,
    unit_lower::constant::lower_constant_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
};

#[test]
fn scalar_materializations_use_exact_values_and_reject_foreign_analysis() {
    for (ty, value, expected) in [
        ("Boolean", "true", ScalarConstant::Boolean(true)),
        ("Byte", "127", ScalarConstant::Integer(127)),
        ("Short", "32767", ScalarConstant::Integer(32767)),
        ("Int", "2147483647", ScalarConstant::Integer(2147483647)),
        (
            "Long",
            "9223372036854775807",
            ScalarConstant::Integer(9223372036854775807),
        ),
        ("UByte", "255u", ScalarConstant::Integer(255)),
        ("UShort", "65535u", ScalarConstant::Integer(65535)),
        ("UInt", "4294967295u", ScalarConstant::Integer(4294967295)),
        (
            "ULong",
            "18446744073709551615u",
            ScalarConstant::Integer(18446744073709551615),
        ),
        ("Char", "'界'", ScalarConstant::Char(u32::from('界'))),
    ] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            &format!(
                "package p\nconst val VALUE: {ty} = {value}\nfun unused(own flag: Boolean): Boolean = flag && true"
            ),
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            &format!("package q\nimport p.VALUE\nfun entry(): {ty} = VALUE"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .unwrap();
        let entry = declaration(&names, "q", "entry");
        let (forward, _) = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        let (reverse, _) = lower_constant_unit_with_entry(
            &sources,
            &[inputs[1], inputs[0]],
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap();
        assert_eq!(render_program(&forward), render_program(&reverse));
        // No declaration initializer function or storage root may appear at runtime.
        assert_eq!(forward.modules[0].functions.len(), 1, "{ty}");
        let expected_type = match ty {
            "Boolean" => SsaTypeKind::Boolean,
            "Char" => SsaTypeKind::Char,
            name => SsaTypeKind::Integer {
                bits: match name {
                    "Byte" | "UByte" => 8,
                    "Short" | "UShort" => 16,
                    "Int" | "UInt" => 32,
                    _ => 64,
                },
                signed: !name.starts_with('U'),
            },
        };
        let module = &forward.modules[0];
        let function = &module.functions[0];
        assert_eq!(
            module.types[function.return_types[0].index()],
            expected_type,
            "{ty}"
        );
        for instruction in &function.instructions {
            if matches!(instruction.operation, Operation::Constant(_)) {
                let result_type = function.entity(instruction.results[0]).unwrap().ty;
                assert_eq!(
                    result_type,
                    EntityType::Value(function.return_types[0]),
                    "{ty}"
                );
            }
        }
        let constants = forward.modules[0].functions[0]
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::Constant(value) = &instruction.operation {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(constants, [expected], "{ty}");
        let other_typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .unwrap()
            .validate_constants()
            .unwrap();
        let error = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &other_typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
        let (_, other_environment) = standard_environments();
        let error = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &other_environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
        let renamed = [
            inputs[0],
            SourceUnitInput::new("root", "q/renamed.ko", consumer_source, &consumer),
        ];
        let error = lower_constant_unit_with_entry(
            &sources,
            &renamed,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
    }
}

#[test]
fn string_uses_materialize_independent_owners_with_literal_cleanup() {
    for (signature, body, literals, drops) in [
        ("String", "= TEXT", 1, 0),
        ("String", "= ((TEXT))", 1, 0),
        ("Unit", "{ val done = println(TEXT) }", 1, 1),
        ("Unit", "{ val done = view(TEXT, (TEXT)) }", 2, 2),
        ("Unit", "{ val done = consume(TEXT) }", 1, 0),
        ("Unit", "{ val done = consume((TEXT)) }", 1, 0),
        ("String", "= TEXT + TEXT", 2, 2),
        ("Boolean", "= (TEXT) == TEXT", 2, 2),
        ("Boolean", "= TEXT != (TEXT)", 2, 2),
    ] {
        for spelling in ["TEXT", "p.TEXT", "\"界\\n\""] {
            let body = body.replace("TEXT", spelling);
            let mut sources = SourceMap::new();
            let (provider_source, provider) = parsed(
                &mut sources,
                "p/provider.ko",
                "package p\nconst val TEXT = \"界\\n\"\nfun view(first: String, second: String): Unit {}\nfun consume(own text: String): Unit {}",
            );
            let (consumer_source, consumer) = parsed(
                &mut sources,
                "q/consumer.ko",
                &format!(
                    "package q\nimport p.TEXT\nimport p.view\nimport p.consume\nfun entry(): {signature} {body}"
                ),
            );
            let inputs = [
                SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
                SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
            ];
            let (name_environment, environment) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                    .unwrap()
                    .validate()
                    .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
                .unwrap()
                .validate_constants()
                .unwrap();
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            let entry = declaration(&names, "q", "entry");
            let (program, entry) = lower_constant_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
            )
            .unwrap_or_else(|error| panic!("{body}: {error:?}"));
            let function = program.modules[0].function(entry).unwrap();
            let owners = function
                .instructions
                .iter()
                .filter_map(|instruction| match &instruction.operation {
                    Operation::StringLiteral { bytes, .. } => {
                        assert_eq!(bytes, "界\n".as_bytes());
                        Some(instruction.results[0])
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(owners.len(), literals, "{body}");
            assert_eq!(
                owners
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                literals,
                "each use owns a distinct value"
            );
            let dropped = function
                .instructions
                .iter()
                .filter_map(|instruction| match instruction.operation {
                    Operation::Drop { owner } => Some(super::model::EntityId::Value(owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(dropped.len(), drops, "{body}");
            if drops == literals {
                assert_eq!(
                    dropped,
                    owners.iter().copied().rev().collect::<Vec<_>>(),
                    "{body}"
                );
            }
            crate::llvm::render_verified_program(&program)
                .expect("String constants produce verified LLVM");
        }
    }
}

#[test]
fn short_circuit_plans_preserve_static_and_conditional_execution() {
    for (expression, expected_strings) in [
        ("(p.FALSE) && p.view(p.TEXT)", 0),
        ("(p.TRUE) || p.view(p.TEXT)", 0),
        ("p.TRUE && p.view(p.TEXT)", 1),
        ("p.FALSE || p.view(p.TEXT)", 1),
        ("flag && p.view(p.TEXT)", 1),
        ("flag || p.view(p.TEXT)", 1),
        (
            "(if (flag) { error(p.TEXT) } else { p.TRUE }) && p.view(p.TEXT)",
            2,
        ),
        (
            "(if (flag) { error(p.TEXT) } else { p.FALSE }) || p.view(p.TEXT)",
            2,
        ),
        ("flag && if (flag) { error(p.TEXT) } else { true }", 1),
        ("flag || if (flag) { error(p.TEXT) } else { false }", 1),
        (
            "flag && if (flag) { return true } else { p.view(p.TEXT) }",
            1,
        ),
        (
            "flag || if (flag) { return false } else { p.view(p.TEXT) }",
            1,
        ),
    ] {
        let program = lower_short_circuit_fixture(&format!(
            "fun entry(own flag: Boolean): Boolean = {expression}"
        ));
        let count = program.modules[0]
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .count();
        assert_eq!(count, expected_strings, "{expression}");
        crate::llvm::render_verified_program(&program)
            .expect("short circuit facts produce verified LLVM");
    }
}

#[test]
fn conditional_move_cleans_only_the_skip_edge() {
    for operator in ["&&", "||"] {
        let program = lower_short_circuit_fixture(&format!(
            "fun entry(own flag: Boolean, own text: String): Boolean = flag {operator} p.consume(text)"
        ));
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("q.entry"))
            .unwrap();
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            1,
            "only the unconsumed skip owner is dropped"
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

fn lower_short_circuit_fixture(consumer: &str) -> super::model::Program {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nconst val FALSE = false\nconst val TRUE = true\nconst val TEXT = \"value\"\nfun view(text: String): Boolean = true\nfun consume(own text: String): Boolean = true\nfun view_pair(text: String, own flag: Boolean): Unit {}\nfun consume_pair(own text: String, own flag: Boolean): Unit {}",
    );
    let (consumer_source, consumer_file) = parsed(
        &mut sources,
        "q/consumer.ko",
        &format!("package q\n{consumer}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer_file),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .unwrap_or_else(|error| panic!("{consumer}: {error:?}"))
    .0
}

#[test]
fn pending_string_prefix_survives_short_circuit_exit_edges() {
    for callee in ["view_pair", "consume_pair"] {
        for operator in ["&&", "||"] {
            for exit in ["return", "error(p.TEXT)"] {
                let program = lower_short_circuit_fixture(&format!(
                    "fun entry(own flag: Boolean): Unit {{ val done = p.{callee}(p.TEXT, flag {operator} if (flag) {{ {exit} }} else {{ true }}) }}"
                ));
                let function = program.modules[0]
                    .functions
                    .iter()
                    .find(|function| function.name.contains("q.entry"))
                    .unwrap();
                let borrowed = callee == "view_pair";
                let returning = exit == "return";
                let drops = function
                    .instructions
                    .iter()
                    .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                    .count();
                let endings = function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        matches!(instruction.operation, Operation::BorrowEnd { .. })
                    })
                    .count();
                assert_eq!(
                    drops,
                    usize::from(borrowed) + usize::from(returning),
                    "{callee}/{operator}/{exit}"
                );
                assert_eq!(
                    endings,
                    if borrowed {
                        1 + usize::from(returning)
                    } else {
                        0
                    },
                    "{callee}/{operator}/{exit}"
                );
                crate::llvm::render_verified_program(&program)
                    .expect("prefix cleanup is valid on both exit and continuation edges");
            }
        }
    }
}

#[test]
fn pending_prefix_preserves_borrowed_inputs_and_cleans_named_values_and_loop_exits() {
    for source in [
        "fun entry(text: String, own flag: Boolean): Unit { val done = p.view_pair(text, flag && if (flag) { return } else { true }) }",
        "fun entry(own flag: Boolean): Unit { val text = p.TEXT
val done = p.consume_pair(text, flag && if (flag) { return } else { true }) }",
        "fun entry(own flag: Boolean): Unit { loop { val done = p.view_pair(p.TEXT, if (flag) { break } else { true }) } }",
        "fun entry(own flag: Boolean): Unit { loop { val done = p.consume_pair(p.TEXT, if (flag) { continue } else { true }) } }",
    ] {
        let program = lower_short_circuit_fixture(source);
        if source.starts_with("fun entry(text:") {
            let function = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains("q.entry"))
                .unwrap();
            assert!(
                !function.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::BorrowEnd { .. } | Operation::Drop { .. }
                )),
                "the caller retains its borrowed input"
            );
        }
        crate::llvm::render_verified_program(&program)
            .expect("pending scope cleanup preserves valid owners and loans");
    }
}

#[test]
fn string_binary_prefix_uses_rebound_owner_and_cleans_exit_edges() {
    for operator in ["+", "==", "!="] {
        for left in ["p.TEXT", "\"value\"", "text"] {
            for exit in ["p.TEXT", "return", "error(p.TEXT)"] {
                let source = format!(
                    "fun entry(own flag: Boolean): Unit {{ val text = p.TEXT\nval result = {left} {operator} (if (flag) {{ {exit} }} else {{ p.TEXT }}) }}"
                );
                let program = lower_short_circuit_fixture(&source);
                let function = program.modules[0]
                    .functions
                    .iter()
                    .find(|function| function.name.contains("q.entry"))
                    .unwrap();
                let drops = function
                    .instructions
                    .iter()
                    .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                    .count();
                assert_eq!(
                    drops,
                    2 + usize::from(left != "text")
                        + usize::from(operator == "+")
                        + usize::from(exit == "return"),
                    "{source}: return cleans its prefix; Abort does not unwind"
                );
                crate::llvm::render_verified_program(&program)
                    .expect("String operand owners survive RHS control flow and exit cleanup");
            }
        }
    }
}

#[test]
fn nested_string_prefixes_preserve_outer_pending_slots() {
    for source in [
        "fun entry(own flag: Boolean): Unit { val result = p.TEXT + (p.TEXT + (if (flag) { return } else { p.TEXT })) }",
        "fun entry(own flag: Boolean): Unit { val result = (if (flag) { return } else { p.TEXT }) + p.TEXT }",
        "fun entry(own flag: Boolean): Unit { val result = p.view_pair(p.TEXT, p.TEXT == (if (flag) { return } else { p.TEXT })) }",
        "fun entry(own flag: Boolean): Unit { loop { val result = p.TEXT + (if (flag) { break } else { p.TEXT }) } }",
        "fun entry(own flag: Boolean): Unit { loop { val result = p.TEXT + (if (flag) { continue } else { p.TEXT }) } }",
    ] {
        let program = lower_short_circuit_fixture(source);
        crate::llvm::render_verified_program(&program)
            .expect("nested pending operands keep their own slots and cleanup scope");
    }
}
