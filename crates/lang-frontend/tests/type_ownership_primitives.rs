//! SPEC-0232：replace / swap 的 Phase 2 intrinsic 身份与事务事实。

use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, FinalTypeAdmissionError,
        OwnershipPrimitiveKind, ParameterMode, TypeKind, TypedFile, UnitCallTarget, UnitTypeKind,
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        standard_environments,
    },
};

fn parse(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("source");
    let file = parse_file(sources, &lex(sources, source).expect("lex")).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    (source, file)
}

fn single(text: &str) -> (SourceMap, ParsedFile, TypedFile) {
    let mut sources = SourceMap::new();
    let (_, file) = parse(&mut sources, "primitives.ko", text);
    let (names, environment) = standard_environments();
    let names = resolve_names(&sources, &file, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &environment).expect("types");
    (sources, file, typed)
}

fn unit(texts: &[(&str, &str)], reverse: bool) -> CompilationUnitTypes {
    let mut sources = SourceMap::new();
    let files = texts
        .iter()
        .map(|(path, text)| parse(&mut sources, path, text))
        .collect::<Vec<_>>();
    let mut inputs = texts
        .iter()
        .zip(&files)
        .map(|((path, _), (source, file))| SourceUnitInput::new("root", path, *source, file))
        .collect::<Vec<_>>();
    if reverse {
        inputs.reverse();
    }
    let (names, environment) = standard_environments();
    let observe = |inputs: &[SourceUnitInput<'_>]| {
        let index = index_compilation_unit(&sources, inputs).expect("index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let names = resolve_compilation_unit_names(&sources, inputs, &index, &names)
            .expect("names")
            .validate()
            .expect("valid names");
        let typed =
            check_compilation_unit_types(&sources, inputs, &names, &environment).expect("types");
        // Do not equate basic validation with final admission, especially for deferred inputs.
        // Matching rooted/MemoryDocument rejection is not a successful-admission witness.
        let complete =
            match complete_compilation_unit_types(&sources, inputs, &names, &environment, &typed) {
                Ok(complete) => {
                    assert!(complete.is_same_analysis(&typed));
                    Ok(())
                }
                Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => Err(diagnostics),
                Err(other) => panic!("internal final-admission failure: {other:?}"),
            };
        for fact in typed.ownership_primitives() {
            let indexed = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|source| source.id() == fact.expression().source_unit())
                .expect("fact's actual source unit");
            let (_, parsed) = files
                .iter()
                .find(|(source, _)| *source == indexed.source_id())
                .expect("fact's original parsed source");
            assert_eq!(typed.ownership_primitive(fact.expression()), Some(*fact));
            assert!(
                fact.operands()
                    .iter()
                    .all(|operand| operand.source_unit() == fact.expression().source_unit())
            );
            assert_eq!(
                fact.operands().map(|operand| operand.expression()),
                operands(parsed, fact.expression().expression()),
            );
            let call = typed
                .call(fact.expression())
                .expect("primitive call identity");
            assert!(matches!(
                call.instance().target(),
                UnitCallTarget::External(_)
            ));
            assert_eq!(call.instance().type_arguments(), &[fact.value_type()]);
            let expected_modes = match fact.kind() {
                OwnershipPrimitiveKind::Replace => [ParameterMode::Inout, ParameterMode::Value],
                OwnershipPrimitiveKind::Swap => [ParameterMode::Inout, ParameterMode::Inout],
            };
            assert_eq!(
                call.arguments()
                    .iter()
                    .map(|argument| argument.mode())
                    .collect::<Vec<_>>(),
                expected_modes
            );
        }
        (typed, complete)
    };
    let (rooted, complete) = observe(&inputs);
    if let [(source, file)] = files.as_slice() {
        // Only single-source cases acquire MemoryDocument. Genuine multifile witnesses
        // keep their rooted identities and input-order contracts unchanged.
        let memory_inputs = [SourceUnitInput::memory_document(*source, file)];
        let (memory, memory_complete) = observe(&memory_inputs);
        assert_eq!(rooted.diagnostics(), memory.diagnostics());
        assert_eq!(complete, memory_complete);
        assert_eq!(rooted.ownership_primitives(), memory.ownership_primitives());
        assert_eq!(rooted.calls(), memory.calls());
        assert_eq!(rooted.assignments(), memory.assignments());
    }
    rooted
}

fn operands(file: &ParsedFile, expression: ExpressionId) -> [ExpressionId; 2] {
    let Expression::Call { arguments, .. } =
        file.ast().expressions().get(expression).unwrap().payload()
    else {
        panic!("primitive is a call");
    };
    [arguments[0].value, arguments[1].value]
}

const BASIC: &str =
    "fun run(): Unit {\nvar a = 1\nvar b = 2\nval old = replace<Int>(&a, 3)\nswap(&a, &b)\n}";

#[test]
fn single_primitives_publish_exact_intrinsic_operands_and_call_contracts() {
    let (_, file, typed) = single(BASIC);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = unit(&[("primitives.ko", BASIC)], false);
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(unit.ownership_primitives().len(), 2);
    assert_eq!(
        unit.ownership_primitives()[0].kind(),
        OwnershipPrimitiveKind::Replace
    );
    assert_eq!(
        unit.ownership_primitives()[1].kind(),
        OwnershipPrimitiveKind::Swap
    );
    let facts = typed.ownership_primitives();
    assert_eq!(facts.len(), 2);
    for (fact, kind, modes) in [
        (
            facts[0],
            OwnershipPrimitiveKind::Replace,
            [ParameterMode::Inout, ParameterMode::Value],
        ),
        (
            facts[1],
            OwnershipPrimitiveKind::Swap,
            [ParameterMode::Inout, ParameterMode::Inout],
        ),
    ] {
        assert_eq!(fact.kind(), kind);
        assert_eq!(fact.operands(), operands(&file, fact.expression()));
        assert_eq!(typed.ownership_primitive(fact.expression()), Some(fact));
        assert_eq!(
            typed.types().get(fact.value_type()),
            Some(&TypeKind::Builtin(BuiltinType::Int))
        );
        let call = typed
            .call(fact.expression())
            .expect("ordinary call contract");
        assert!(matches!(
            call.instance().target(),
            CallableTarget::External(_)
        ));
        assert_eq!(call.instance().type_arguments(), &[fact.value_type()]);
        assert_eq!(
            call.arguments()
                .iter()
                .map(|arg| arg.mode())
                .collect::<Vec<_>>(),
            modes
        );
    }
}

#[test]
fn unit_primitives_keep_source_qualified_identity_and_input_order_independence() {
    let texts = [
        ("a.ko", BASIC),
        (
            "b.ko",
            "fun other(): Unit {\nvar a = 1\nvar b = 2\nval old = replace<Int>(&a, 3)\nswap(&a, &b)\n}",
        ),
    ];
    let forward = unit(&texts, false);
    let reverse = unit(&texts, true);
    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(
        forward.ownership_primitives(),
        reverse.ownership_primitives()
    );
    assert_eq!(forward.ownership_primitives().len(), 4);
    for fact in forward.ownership_primitives() {
        assert_eq!(forward.ownership_primitive(fact.expression()), Some(*fact));
        assert!(
            fact.operands()
                .iter()
                .all(|operand| operand.source_unit() == fact.expression().source_unit())
        );
        assert_eq!(
            forward.types().get(fact.value_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        let call = forward
            .call(fact.expression())
            .expect("ordinary call contract");
        assert!(matches!(
            call.instance().target(),
            UnitCallTarget::External(_)
        ));
        assert_eq!(call.instance().type_arguments(), &[fact.value_type()]);
    }
    let facts = forward.ownership_primitives();
    assert_ne!(facts[0].expression(), facts[2].expression());
    assert_eq!(
        facts[0].expression().expression(),
        facts[2].expression().expression()
    );
    assert!(forward.validate().is_ok());
    assert!(reverse.validate().is_ok());
}

#[test]
fn source_functions_named_replace_and_swap_have_no_intrinsic_facts() {
    let text = "fun replace(value: Int): Int = value\nfun swap(value: Int): Int = value\nfun run(): Int = replace(swap(1))";
    let (_, _, typed) = single(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.ownership_primitives().is_empty());
    assert_eq!(typed.calls().len(), 2);
    assert!(
        typed
            .calls()
            .iter()
            .all(|call| matches!(call.instance().target(), CallableTarget::Source(_)))
    );
    let typed = unit(&[("source.ko", text)], false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.calls().len(), 2);
    assert!(typed.ownership_primitives().is_empty());
    assert!(
        typed
            .calls()
            .iter()
            .all(|call| matches!(call.instance().target(), UnitCallTarget::Declaration(_)))
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn imported_source_aliases_never_gain_intrinsic_identity() {
    let typed = unit(
        &[
            (
                "lib/api.ko",
                "package lib\nfun replace(value: Int): Int = value\nfun swap(value: Int): Int = value",
            ),
            (
                "app/main.ko",
                "package app\nimport lib.replace as exchange\nimport lib.swap\nfun run(): Int = exchange(swap(1))",
            ),
        ],
        false,
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.ownership_primitives().is_empty());
    assert_eq!(typed.calls().len(), 2);
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_primitives_do_not_publish_partial_facts() {
    for call in [
        "replace(&a)",
        "replace<Int, Int>(&a, 2)",
        "replace(place = &a, new = 2)",
        "replace(a, 2)",
        "replace(&a, &b)",
        "replace(&a, true)",
        "swap(&a)",
        "swap<Int, Int>(&a, &b)",
        "swap(a = &a, b = &b)",
        "swap(a, &b)",
        "swap(&a, b)",
        "swap(&a, &flag)",
    ] {
        let text = format!("fun run(): Unit {{\nvar a = 1\nvar b = 2\nvar flag = true\n{call}\n}}");
        let (_, _, typed) = single(&text);
        assert!(!typed.diagnostics().is_empty(), "{call}");
        assert!(typed.ownership_primitives().is_empty(), "{call}");
        let typed = unit(&[("invalid.ko", &text)], false);
        assert!(!typed.diagnostics().is_empty(), "{call}");
        assert!(typed.ownership_primitives().is_empty(), "{call}");
        assert!(typed.validate().is_err(), "{call}");
    }
}

#[test]
fn invalid_expected_result_does_not_publish_ownership_primitives() {
    let text = "fun run(): Unit {\nvar a = 1\nval wrong: Boolean = replace(&a, 2)\n}";
    let (_, _, typed) = single(text);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    let typed = unit(&[("invalid.ko", text)], false);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn overload_trials_commit_each_nested_primitive_once() {
    let text = "fun choose(callback: () -> Int): Int = 1\nfun choose(callback: () -> String): String = \"text\"\nfun run(): Int {\nvar a = 1\nreturn choose({ replace(&a, 2) })\n}";
    let (_, _, typed) = single(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    let typed = unit(&[("trial.ko", text)], false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    assert!(typed.validate().is_ok());
}

#[test]
fn failed_overload_trials_do_not_leak_nested_primitives() {
    let text = "fun choose(callback: () -> Boolean): Int = 1\nfun choose(callback: () -> String): String = \"text\"\nfun run(): Unit {\nvar a = 1\nchoose({ replace(&a, 2) })\n}";
    let (_, _, typed) = single(text);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    let typed = unit(&[("trial.ko", text)], false);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn divergent_replace_preserves_static_primitive_and_call_identity() {
    let text = "fun run(): Unit {\nvar a = 1\nvar b = 2\nb = replace(&a, error(\"stop\"))\n}";
    let (_, _, typed) = single(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    assert!(
        typed
            .call(typed.ownership_primitives()[0].expression())
            .is_some()
    );
    let typed = unit(&[("divergent.ko", text)], false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    assert!(
        typed
            .call(typed.ownership_primitives()[0].expression())
            .is_some()
    );
    assert_eq!(typed.assignments().len(), 1);
    assert!(typed.validate().is_ok());
}

#[test]
fn deferred_primitive_operands_do_not_publish_successful_contracts() {
    let text = "fun run(): Unit {\nvar a = 1 as Int\nreplace(&a, 2)\n}";
    let (_, _, typed) = single(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.calls().is_empty());
    let typed = unit(&[("deferred.ko", text)], false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.calls().is_empty());
}

#[test]
fn divergent_place_prefixes_preserve_replace_and_swap_structure() {
    for call in [
        "replace(&a[error(\"stop\")], 2)",
        "swap(&a[error(\"stop\")], &b)",
        "swap(&b, &a[error(\"stop\")])",
    ] {
        let text = format!("fun run(): Unit {{\nval a = arrayOf<Int>(1)\nvar b = 2\n{call}\n}}");
        let (_, _, typed) = single(&text);
        assert!(
            typed.diagnostics().is_empty(),
            "{call}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 1, "{call}");
        let typed = unit(&[("divergent.ko", &text)], false);
        assert!(
            typed.diagnostics().is_empty(),
            "{call}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 1, "{call}");
        assert!(typed.validate().is_ok(), "{call}");
    }
}

#[test]
fn nominal_nullable_and_generic_values_keep_their_exact_exchange_type() {
    for text in [
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nvar b = Item(2)\nval old = replace(&a, Item(3))\nswap(&a, &b)\n}",
        "fun run(): Unit {\nvar a: Int? = 1\nvar b: Int? = 2\nval old = replace(&a, 3)\nswap(&a, &b)\n}",
        "fun <T> change(inout a: T, own b: T): T = replace<T>(&a, b)\nfun <T> exchange(inout a: T, inout b: T): Unit = swap<T>(&a, &b)",
    ] {
        let (_, _, typed) = single(text);
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 2);
        for fact in typed.ownership_primitives() {
            assert_eq!(
                typed.expression_type(fact.operands()[0]),
                Some(fact.value_type())
            );
            assert_eq!(
                typed
                    .call(fact.expression())
                    .unwrap()
                    .instance()
                    .type_arguments(),
                &[fact.value_type()]
            );
        }
        let typed = unit(&[("values.ko", text)], false);
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 2);
        for fact in typed.ownership_primitives() {
            assert_eq!(
                typed.expression_type(fact.operands()[0]),
                Some(fact.value_type())
            );
            assert_eq!(
                typed
                    .call(fact.expression())
                    .unwrap()
                    .instance()
                    .type_arguments(),
                &[fact.value_type()]
            );
        }
        assert!(typed.validate().is_ok(), "{text}");
    }
}

#[test]
fn explicit_environment_binding_identifies_intrinsics_independent_of_spelling() {
    use lang_frontend::{
        name_resolution::NameEnvironment,
        type_checking::{IntrinsicCallable, TypeEnvironment},
    };

    let mut sources = SourceMap::new();
    let (source, file) = parse(
        &mut sources,
        "bound.ko",
        "fun run(): Int {\nvar a = 1\nreturn exchange(&a, 2)\n}",
    );
    let mut names = NameEnvironment::new();
    let int = names.declare_type("Int").unwrap();
    let external = names.declare_function("exchange").unwrap();
    let mut environment = TypeEnvironment::new(&names);
    environment.bind_builtin(int, BuiltinType::Int).unwrap();
    environment
        .bind_intrinsic_callable(external, IntrinsicCallable::Replace)
        .unwrap();
    let resolution = resolve_names(&sources, &file, &names).unwrap();
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &file, &resolution, &environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    let fact = typed.ownership_primitives()[0];
    assert_eq!(fact.kind(), OwnershipPrimitiveKind::Replace);
    assert_eq!(
        typed.call(fact.expression()).unwrap().instance().target(),
        CallableTarget::External(external)
    );

    for input in [
        SourceUnitInput::new("root", "bound.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.ownership_primitives().len(), 1);
        let fact = typed.ownership_primitives()[0];
        assert_eq!(fact.kind(), OwnershipPrimitiveKind::Replace);
        assert_eq!(
            typed.call(fact.expression()).unwrap().instance().target(),
            UnitCallTarget::External(external)
        );
        let [indexed] = names.names().index().source_units() else {
            panic!("one actual bound source");
        };
        assert_eq!(indexed.source_id(), source);
        assert_eq!(fact.expression().source_unit(), indexed.id());
        assert_eq!(
            fact.operands().map(|operand| operand.expression()),
            operands(&file, fact.expression().expression())
        );
        assert!(
            fact.operands()
                .iter()
                .all(|operand| operand.source_unit() == fact.expression().source_unit())
        );
        assert!(typed.validate().is_ok());
    }
}

#[test]
fn errors_anywhere_prevent_partial_primitive_table_publication() {
    let text = "fun run(): Unit {\nvar a = 1\nval old = replace(&a, 2)\nval wrong: Boolean = 3\n}";
    let (_, _, typed) = single(text);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert_eq!(
        typed.calls().len(),
        1,
        "recovery call structure remains available"
    );
    let typed = unit(&[("invalid.ko", text)], false);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert_eq!(
        typed.calls().len(),
        1,
        "recovery call structure remains available"
    );
    assert!(typed.validate().is_err());
}

#[test]
fn closures_with_divergent_bodies_keep_static_primitive_identity() {
    for replacement in ["{ error(\"stop\") }", "({ error(\"stop\") })"] {
        let text = format!(
            "fun run(): Unit {{\nvar f: () -> Int = {{ 1 }}\nval old = replace(&f, {replacement})\n}}"
        );
        let (_, file, typed) = single(&text);
        assert!(
            typed.diagnostics().is_empty(),
            "{replacement}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 1);
        for fact in typed.ownership_primitives() {
            assert_eq!(fact.operands(), operands(&file, fact.expression()));
            assert!(typed.call(fact.expression()).is_some());
        }
        let typed = unit(&[("closure.ko", &text)], false);
        assert!(
            typed.diagnostics().is_empty(),
            "{replacement}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 1);
        assert!(typed.validate().is_ok());
    }
}

#[test]
fn short_circuit_operands_keep_nested_static_primitive_identity() {
    for replacement in [
        "true || replace(&other, error(\"stop\"))",
        "false && replace(&other, error(\"stop\"))",
        "condition || replace(&other, error(\"stop\"))",
    ] {
        let text = format!(
            "fun run(condition: Boolean): Unit {{\nvar flag = true\nvar other = true\nreplace(&flag, {replacement})\n}}"
        );
        let (_, file, typed) = single(&text);
        assert!(
            typed.diagnostics().is_empty(),
            "{replacement}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 2);
        for fact in typed.ownership_primitives() {
            assert_eq!(fact.operands(), operands(&file, fact.expression()));
            assert!(typed.call(fact.expression()).is_some());
        }
        let typed = unit(&[("logical.ko", &text)], false);
        assert!(
            typed.diagnostics().is_empty(),
            "{replacement}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.ownership_primitives().len(), 2);
        assert!(typed.validate().is_ok());
    }
}

#[test]
fn divergent_if_condition_preserves_static_primitive_identity() {
    let text = "fun run(): Unit {\nvar a = 1\nreplace(&a, if (error(\"stop\")) 2 else 3)\n}";
    let (_, file, typed) = single(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    for fact in typed.ownership_primitives() {
        assert_eq!(fact.operands(), operands(&file, fact.expression()));
        assert!(typed.call(fact.expression()).is_some());
    }
    let typed = unit(&[("condition.ko", text)], false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.ownership_primitives().len(), 1);
    assert!(typed.validate().is_ok());
}
