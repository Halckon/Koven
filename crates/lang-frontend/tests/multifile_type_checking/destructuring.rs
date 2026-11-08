use super::*;

fn destructuring_statements(file: &ParsedFile) -> Vec<StatementId> {
    file.ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::LocalDestructuring { .. }).then_some(id)
        })
        .collect()
}

#[test]
fn local_destructuring_reachability_stays_with_its_callable() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "main.ko",
        r#"
        value class Pair(val first: Int, val second: Int)
        fun f(flag: Boolean): Unit {
            while (false) {
                { val (first, second) = Pair(1, 2) }
                val nested: () -> Unit = { val (first, second) = Pair(3, 4) }
                val (second, first) = Pair(5, 6)
            }
            loop { if (flag) { break } }
            { val (first, second) = Pair(7, 8) }
            loop {}
            val (second, first) = Pair(9, 10)
        }
        fun sibling(): Unit { val (first, second) = Pair(11, 12) }
        "#,
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = source_unit(&names, source);
    let statements = destructuring_statements(&file);
    let expected = [false, true, false, true, false, true];
    assert_eq!(statements.len(), expected.len());
    for (statement, reachable) in statements.into_iter().zip(expected) {
        let descriptor = typed
            .destructuring(UnitStatementId::new(unit, statement))
            .expect("dead paths still produce type facts");
        assert_eq!(descriptor.is_reachable(), reachable, "{statement:?}");
    }
}

#[test]
fn local_destructuring_reachability_respects_abrupt_initializers_and_sources() {
    for (header, expected) in [
        (
            "val (first, second) = if (true) { return } else { Pair(1, 2) }",
            [false, false],
        ),
        (
            "for (item in if (true) { return } else { listOf(1) }) { val (first, second) = Pair(1, 2) }",
            [false, false],
        ),
        (
            "for ((key, value) in if (true) { return } else { entries }) { val (first, second) = Pair(1, 2) }",
            [false, false],
        ),
        (
            "for (item in if (flag) { return } else { listOf(1) }) { val (first, second) = Pair(1, 2) }",
            [true, true],
        ),
        (
            "val ignored = (if (true) { return } else { 0 }) + (if (flag) { val (first, second) = Pair(1, 2); first } else { 0 })",
            [false, false],
        ),
    ] {
        let text = format!(
            "value class Pair(val first: Int, val second: Int)\n\
             fun f(flag: Boolean, entries: Map<Int, Int>): Unit {{\n\
                {{ {header} }}\n\
                val (first, second) = Pair(3, 4)\n\
             }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("unit types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let unit = source_unit(&names, source);
        let statements = destructuring_statements(&file);
        assert_eq!(statements.len(), expected.len());
        for (statement, reachable) in statements.into_iter().zip(expected) {
            let descriptor = typed
                .destructuring(UnitStatementId::new(unit, statement))
                .expect("typed split");
            assert_eq!(
                descriptor.is_reachable(),
                reachable,
                "{header}: {statement:?}"
            );
        }
    }
}

#[test]
fn cross_file_value_class_destructuring_publishes_copy_and_consume_facts() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         value class Pair<A, B>(val first: A, val second: B)\n\
         class Ref\n\
         fun echo(pair: Pair<Int, Int>): Pair<Int, Int> = pair",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun split(copy: Pair<Int, Int>, moved: Pair<Int, String>, ref: Ref): Unit {\n\
             { val (first, second) = echo(copy) }\n\
             val (second, first) = moved\n\
             val (e, f) = ref\n\
         }\n\
         fun <T: Copyable> splitBound(pair: Pair<T, T>): Unit {\n\
             val (first, second) = pair\n\
         }\n\
         fun <T> splitUnbound(pair: Pair<T, T>): Unit {\n\
             val (second, first) = pair\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward destructuring unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse destructuring unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.destructurings(), reverse.destructurings());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward.calls().len(),
        1,
        "destructuring initializer is checked exactly once"
    );

    let uses_unit = source_unit(&forward_names, uses_source);
    let statements = destructuring_statements(&uses);
    assert_eq!(
        statements.len(),
        5,
        "three concrete and two generic destructurings"
    );
    assert_eq!(forward.destructurings().len(), 4);
    assert_eq!(
        forward
            .destructurings()
            .iter()
            .map(|descriptor| descriptor.mode())
            .collect::<Vec<_>>(),
        [
            DestructuringMode::Copy,
            DestructuringMode::Consume,
            DestructuringMode::Copy,
            DestructuringMode::Consume,
        ]
    );
    assert_eq!(
        forward.destructurings()[0].statement(),
        UnitStatementId::new(uses_unit, statements[0])
    );
    assert_eq!(
        forward.destructurings()[1].statement(),
        UnitStatementId::new(uses_unit, statements[1])
    );
    assert!(
        forward
            .destructuring(UnitStatementId::new(uses_unit, statements[2]))
            .is_none()
    );
    assert_eq!(
        forward.destructurings()[2].statement(),
        UnitStatementId::new(uses_unit, statements[3])
    );
    assert_eq!(
        forward.destructurings()[3].statement(),
        UnitStatementId::new(uses_unit, statements[4])
    );

    let first_component_types = forward.destructurings()[0]
        .components()
        .iter()
        .map(|component| forward.types().get(component.ty()))
        .collect::<Vec<_>>();
    assert_eq!(
        first_component_types,
        [
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        ]
    );
    let moved_component_types = forward.destructurings()[1]
        .components()
        .iter()
        .map(|component| forward.types().get(component.ty()))
        .collect::<Vec<_>>();
    assert_eq!(
        moved_component_types,
        [
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
            Some(&UnitTypeKind::Builtin(BuiltinType::String)),
        ]
    );
    assert_eq!(
        forward.destructurings()[0]
            .components()
            .iter()
            .map(|component| component.symbol())
            .collect::<Vec<_>>(),
        [
            symbol_named(&forward, &forward_names, uses_unit, "first"),
            symbol_named(&forward, &forward_names, uses_unit, "second"),
        ]
    );
    for name in ["e", "f"] {
        assert!(matches!(
            forward
                .symbol_type(symbol_named(&forward, &forward_names, uses_unit, name))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::Destructuring))
        ));
    }
}

#[test]
fn cross_file_destructuring_arity_reports_l0118_and_keeps_recovery_types() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nvalue class Pair<A, B>(val first: A, val second: B)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun split(pair: Pair<Int, String>): Unit {\n\
             { val (second) = pair }\n\
             val (first, second, extra) = pair\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("arity errors stay in recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse arity errors stay in recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert!(forward.destructurings().is_empty());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0118", "L0118", "L0080"]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("pattern span"))
            .collect::<Vec<_>>(),
        ["(second)", "(first, second, extra)", "extra"]
    );
    let labels = forward
        .body_diagnostics()
        .iter()
        .map(|diagnostic| {
            diagnostic
                .details()
                .iter()
                .find_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                    DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
                })
                .expect("L0118 labels the value-class declaration")
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Pair", "Pair", "Pair"]);

    let uses_unit = source_unit(&forward_names, uses_source);
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "second",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::String))
    ));
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "extra",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    assert!(forward.validate().is_err());
}

#[test]
fn pending_local_destructuring_preserves_component_types_without_executable_facts() {
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-destructuring.ko",
        "value class Position(val index: Int, val spare: Int)\n\
         fun from_list(own positions: MutableList<Position>, own items: MutableList<Int>): Unit {\n\
             val (index, spare) = (if (outside()) positions else positions).removeFirst()\n\
             items.removeAt(index)\n\
             items.add(spare)\n\
         }\n\
         fun from_branch(pos: Position, own values: MutableList<Int>): Unit {\n\
             val pending = if (outside()) pos else pos\n\
             val (index, spare) = pending\n\
             values.insertAt(index, spare)\n\
         }\n\
         fun stable(pos: Position, own ready: MutableList<Int>): Unit {\n\
             val (index, spare) = pos\n\
             ready.insertAt(index, spare)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-destructuring.ko",
        source,
        &file,
    )];
    let (mut names_env, types_env) = standard_environments();
    names_env.declare_function("outside").expect("name binding");
    let names = validated_names(&sources, &inputs, &names_env);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env)
        .expect("recoverable type analysis");
    assert!(
        typed.body_diagnostics().is_empty(),
        "{:?}",
        typed.body_diagnostics()
    );
    let unit = source_unit(&names, source);
    let key = |text| UnitExpressionId::new(unit, expression_with_text(&sources, &file, text));
    let statements = destructuring_statements(&file);
    assert_eq!(statements.len(), 3);
    assert!(
        typed
            .destructuring(UnitStatementId::new(unit, statements[0]))
            .is_none()
    );
    assert!(
        typed
            .destructuring(UnitStatementId::new(unit, statements[1]))
            .is_none()
    );
    assert_eq!(
        typed
            .destructuring(UnitStatementId::new(unit, statements[2]))
            .map(|fact| fact.mode()),
        Some(DestructuringMode::Copy)
    );
    assert!(typed.container_remove_ats().is_empty());
    assert!(typed.container_appends().is_empty());
    assert_eq!(typed.container_insert_ats().len(), 1);
    assert!(
        typed
            .container_insert_at(key("ready.insertAt(index, spare)"))
            .is_some()
    );
    for name in ["index", "spare"] {
        let mut reads = 0;
        for (id, node) in file.ast().expressions().iter() {
            if matches!(node.payload(), Expression::Name) && sources.slice(node.span()) == Ok(name)
            {
                reads += 1;
                assert_eq!(
                    typed
                        .expression_type(UnitExpressionId::new(unit, id))
                        .and_then(|ty| typed.types().get(ty)),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
                );
            }
        }
        assert_eq!(reads, 3, "{name}");
    }
    let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
        complete_compilation_unit_types(&sources, &inputs, &names, &types_env, &typed)
    else {
        panic!("pending initializers must produce source diagnostics");
    };
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(
        diagnostics.iter().all(|diagnostic| {
            diagnostic.code().to_string() == "L0165"
                && matches!(
                    sources.slice(diagnostic.primary_span()),
                    Ok("outside" | "outside()")
                )
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn pending_local_destructuring_still_reports_known_pattern_arity() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-destructuring-arity.ko",
        "value class Position(val index: Int, val spare: Int)\n\
         fun recover(pos: Position, own items: MutableList<Int>): Unit {\n\
             val (index) = (if (outside()) pos else pos)\n\
             items.add(index)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-destructuring-arity.ko",
        source,
        &file,
    )];
    let (mut names_env, types_env) = standard_environments();
    names_env.declare_function("outside").expect("name binding");
    let names = validated_names(&sources, &inputs, &names_env);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env)
        .expect("recoverable type analysis");
    assert!(typed.destructurings().is_empty());
    assert!(typed.container_appends().is_empty());
    let [diagnostic] = typed.body_diagnostics() else {
        panic!(
            "expected exactly one arity diagnostic: {:?}",
            typed.body_diagnostics()
        );
    };
    assert_eq!(diagnostic.code().to_string(), "L0118");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok("(index)"));
}

#[test]
fn pending_local_destructuring_checks_direct_and_nested_inline_fields() {
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

    let (mut names_env, types_env) = standard_environments();
    names_env.declare_type("Unbound").expect("name binding");
    for (declaration, field_type) in [
        ("", "Unbound"),
        ("value class Inner(val value: Unbound)\n", "Inner"),
        ("value class Inner<T>(val value: T)\n", "Inner<Unbound>"),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "{declaration}value class Position(val index: Int, val extra: {field_type}, val label: String)\n\
             fun recover(pos: Position, own items: MutableList<Int>): Unit {{\n\
                 val (index, extra, label) = pos\n\
                 items.add(index)\n\
             }}"
        );
        let (source, file) = parsed(&mut sources, "pending-destructuring-field.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "pending-destructuring-field.ko",
            source,
            &file,
        )];
        let names = validated_names(&sources, &inputs, &names_env);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env)
            .expect("recoverable type analysis");
        assert!(
            typed.body_diagnostics().is_empty(),
            "{:?}",
            typed.body_diagnostics()
        );
        assert!(
            typed.destructurings().is_empty(),
            "a MoveOnly field cannot admit a pending sibling: {field_type}",
        );
        assert!(
            typed.container_appends().is_empty(),
            "the split is atomic even for a known component: {field_type}",
        );
        let index = UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &file, "index"),
        );
        assert_eq!(
            typed
                .expression_type(index)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        );
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types_env, &typed)
        else {
            panic!("unbound field types must produce source diagnostics");
        };
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code().to_string() == "L0165"
                    && sources.slice(diagnostic.primary_span()) == Ok("Unbound")
            }),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn private_destructuring_and_field_access_require_the_declaring_classifier() {
    let mut sources = SourceMap::new();
    let declared = "package p\n\
         value class Vault(private val hidden: Int, val open: Int) {\n\
             fun unpack(other: Vault): Int { val (open, hidden) = other; return hidden + open }\n\
             fun read(other: Vault): Int = other.hidden\n\
         }\n\
         value class Visible(internal val item: Int)\n\
         value class Shadow(val first: Int, private val component1: Int)\n\
         fun outside(v: Vault): Int { val (hidden, open) = v; return open }";
    let (decl_source, declarations) = parsed(&mut sources, "p/models.ko", declared);
    let (use_source, uses) = parsed(
        &mut sources,
        "q/uses.ko",
        "package q\n\
         import p.Vault\n\
         import p.Visible\n\
         import p.Shadow\n\
         fun unpack(v: Vault): Int { val (open, hidden) = v; return hidden }\n\
         fun field(v: Vault): Int = v.hidden\n\
         fun shadowed(other: Shadow): Int = other.component1()\n\
         fun allowed(v: Visible): Int { val (item) = v; return item }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/models.ko", decl_source, &declarations),
        SourceUnitInput::new("root", "q/uses.ko", use_source, &uses),
    ];
    let (names_env, types_env) = standard_environments();
    let names = validated_names(&sources, &inputs, &names_env);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env).unwrap();
    let diagnostics = typed.body_diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| (
                d.code().to_string(),
                sources.slice(d.primary_span()).unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            ("L0080".into(), "hidden"),
            ("L0080".into(), "hidden"),
            ("L0080".into(), "hidden"),
            ("L0080".into(), "component1")
        ],
    );
    for (diagnostic, label_name) in
        diagnostics
            .iter()
            .zip(["hidden", "hidden", "hidden", "component1"])
    {
        let label_start =
            declared.find(&format!("private val {label_name}")).unwrap() + "private val ".len();
        assert!(
            diagnostic.details().iter().any(|detail| matches!(
                detail,
                DiagnosticDetail::Label(label) if label.span().source_id() == decl_source
                    && label.span().start() == label_start
                    && sources.slice(label.span()) == Ok(label_name)
            )),
            "{diagnostic:?}"
        );
    }
    let [inside, outside] = destructuring_statements(&declarations)[..] else {
        panic!("two patterns in the declaring file");
    };
    let [imported, internal] = destructuring_statements(&uses)[..] else {
        panic!("two patterns in the consumer file");
    };
    for (source, ok, denied) in [
        (decl_source, inside, outside),
        (use_source, internal, imported),
    ] {
        let unit = source_unit(&names, source);
        assert!(
            typed
                .destructuring(UnitStatementId::new(unit, ok))
                .is_some()
        );
        assert!(
            typed
                .destructuring(UnitStatementId::new(unit, denied))
                .is_none()
        );
    }
    for text in ["v.hidden", "other.component1()"] {
        let call = UnitExpressionId::new(
            source_unit(&names, use_source),
            expression_with_text(&sources, &uses, text),
        );
        assert!(typed.call(call).is_none());
        assert!(typed.aggregate_projection(call).is_none());
    }
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};
    let admission = complete_compilation_unit_types(&sources, &inputs, &names, &types_env, &typed);
    let Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics)) = admission else {
        panic!("private patterns must reject with source diagnostics: {admission:?}");
    };
    assert_eq!(final_diagnostics, diagnostics);
}

#[test]
fn borrowed_destructuring_cannot_bind_private_fields_but_can_discard_them() {
    for (external_binding, external_result, accepted) in
        [("secret", "secret", false), ("_", "visible", true)]
    {
        let mut sources = SourceMap::new();
        let text = format!(
            "value class Vault(private val hidden: Int, val open: Int) {{\n\
                 fun inside(items: List<Vault>): Int {{\n\
                     for ((secret, visible) in items) {{ return secret + visible }}\n\
                     return 0\n\
                 }}\n\
             }}\n\
             fun outside(items: List<Vault>): Int {{\n\
                 for (({external_binding}, visible) in items) {{ return {external_result} }}\n\
                 return 0\n\
             }}"
        );
        let (source, file) = parsed(&mut sources, "borrowed-private-field.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "borrowed-private-field.ko",
            source,
            &file,
        )];
        let (names_env, types_env) = standard_environments();
        let names = validated_names(&sources, &inputs, &names_env);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env).unwrap();
        if accepted {
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            assert_eq!(typed.sequential_iterations().len(), 2);
        } else {
            let [diagnostic] = typed.body_diagnostics() else {
                panic!(
                    "only the external named binding is invalid: {:?}",
                    typed.body_diagnostics()
                );
            };
            assert_eq!(diagnostic.code().to_string(), "L0160");
            assert_eq!(
                sources.slice(diagnostic.primary_span()),
                Ok("(secret, visible)")
            );
            assert!(diagnostic.primary_span().start() > text.find("fun outside").unwrap());
            assert!(typed.sequential_iterations().is_empty());
        }
    }
}

#[test]
fn malformed_for_headers_publish_no_plan_and_keep_source_diagnostics() {
    use lang_frontend::{
        analysis::{UnitSourceDescriptor, analyze_parsed_unit_names, analyze_unit_recovery},
        name_resolution::resolve_names,
        type_checking::{FinalTypeAdmissionError, check_types, complete_file_types},
    };

    for (visibility, expected) in [("", &["L0010"][..]), ("private ", &["L0160", "L0010"][..])] {
        let mut sources = SourceMap::new();
        let text = format!(
            "value class Vault(val open: Int, {visibility}val hidden: Int)\n\
             fun leak(items: List<Vault>): Int {{\n\
                 for ((visible, secret in items) {{ return visible }}\n\
                 return 0\n\
             }}"
        );
        let source = sources.add_source("for-missing-closer.ko", &text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let [missing] = file.diagnostics() else {
            panic!("one Parser diagnostic: {:?}", file.diagnostics());
        };
        assert_eq!(missing.code().to_string(), "L0010");
        assert_eq!(
            missing.primary_span().start(),
            text.find("in items").unwrap()
        );
        assert!(missing.primary_span().is_empty());
        let (names_env, types_env) = standard_environments();
        let legacy_names = resolve_names(&sources, &file, &names_env).unwrap();
        let legacy = check_types(&sources, &file, &legacy_names, &types_env).unwrap();
        assert!(legacy.sequential_iterations().is_empty());
        let result = complete_file_types(&sources, &file, &legacy_names, &types_env, &legacy);
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = result else {
            panic!("expected source diagnostics: {result:?}");
        };
        let codes = diagnostics
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, expected, "{diagnostics:?}");
        if let [private, _] = diagnostics.as_slice() {
            assert_eq!(sources.slice(private.primary_span()), Ok("("));
        }

        let snapshot = analyze_parsed_unit_names(
            sources,
            vec![(UnitSourceDescriptor::memory_document(source), file)],
            names_env,
        )
        .unwrap();
        assert!(snapshot.validated_names().is_none());
        let (typed, owned) = analyze_unit_recovery(&snapshot, &types_env)
            .expect("malformed for headers must recover without executable iteration facts");
        assert!(typed.sequential_iterations().is_empty());
        assert!(owned.iterations().is_empty());
    }
}

#[test]
fn pending_iteration_sources_withhold_plans_and_propagate_to_named_bindings() {
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-iteration-source.ko",
        "value class Pair(val first: Int, val second: Int)\n\
         class Bundle(val items: List<Pair>, val pairs: Map<Int, Int>)\n\
         fun pending(p: Bundle, own output: MutableList<Int>): Unit {\n\
             val provider = (if (outside()) p else p).items\n\
             for ((_, element) in provider) { output.add(element) }\n\
             for ((key, value) in (if (outside()) p else p).pairs) { output.insertAt(key, value) }\n\
         }\n\
         fun stable(p: Bundle, own output: MutableList<Int>): Unit {\n\
             for ((_, ready) in p.items) { output.add(ready) }\n\
             for ((index, ready) in p.pairs) { output.insertAt(index, ready) }\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-iteration-source.ko",
        source,
        &file,
    )];
    let (mut names_env, types_env) = standard_environments();
    names_env.declare_function("outside").unwrap();
    let names = validated_names(&sources, &inputs, &names_env);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types_env).unwrap();
    assert!(
        typed.body_diagnostics().is_empty(),
        "{:?}",
        typed.body_diagnostics()
    );
    let unit = source_unit(&names, source);
    let key = |text| UnitExpressionId::new(unit, expression_with_text(&sources, &file, text));
    assert_eq!(typed.sequential_iterations().len(), 1);
    assert_eq!(typed.sequential_iterations()[0].source(), key("p.items"));
    assert_eq!(typed.map_iterations().len(), 1);
    assert_eq!(typed.map_iterations()[0].source(), key("p.pairs"));
    assert_eq!(typed.container_appends().len(), 1);
    assert!(typed.container_append(key("output.add(ready)")).is_some());
    assert_eq!(typed.container_insert_ats().len(), 1);
    assert!(
        typed
            .container_insert_at(key("output.insertAt(index, ready)"))
            .is_some()
    );
    let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
        complete_compilation_unit_types(&sources, &inputs, &names, &types_env, &typed)
    else {
        panic!("expected unresolved source diagnostics");
    };
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(
        diagnostics.iter().all(|d| d.code().to_string() == "L0165"
            && matches!(sources.slice(d.primary_span()), Ok("outside" | "outside()"))),
        "{diagnostics:?}"
    );
}

#[test]
fn pending_iteration_headers_remain_recoverable_without_ownership_provider_facts() {
    use lang_frontend::analysis::{
        UnitSourceDescriptor, analyze_parsed_unit_names, analyze_unit_recovery,
    };
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

    for (provider, binding) in [
        ("List<Int>", "item"),
        ("Map<Int, Int>", "(key, value)"),
        ("IntRange", "item"),
        ("View<Int>", "item"),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run(p: {provider}): Unit {{ for ({binding} in if (outside()) p else p) {{}} }}"
        );
        let (source, file) = parsed(&mut sources, "pending-provider.ko", &text);
        let (mut names_env, types_env) = standard_environments();
        names_env.declare_function("outside").unwrap();
        let snapshot = analyze_parsed_unit_names(
            sources,
            vec![(UnitSourceDescriptor::memory_document(source), file)],
            names_env,
        )
        .unwrap();
        let (typed, owned) = analyze_unit_recovery(&snapshot, &types_env)
            .unwrap_or_else(|error| panic!("{provider} must remain diagnosable: {error:?}"));
        assert!(typed.sequential_iterations().is_empty());
        assert!(typed.map_iterations().is_empty());
        assert!(owned.iterations().is_empty());
        assert!(owned.map_iterations().is_empty());
        assert!(typed.clone().validate().is_err());
        assert!(owned.validate().is_err());

        let result = complete_compilation_unit_types(
            snapshot.sources(),
            &snapshot.inputs(),
            snapshot.validated_names().unwrap(),
            &types_env,
            &typed,
        );
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = result else {
            panic!("{provider}: expected source diagnostics, found {result:?}");
        };
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("{provider}: {diagnostics:?}");
        };
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert!(matches!(
            snapshot.sources().slice(diagnostic.primary_span()),
            Ok("outside" | "outside()")
        ));
    }
}
