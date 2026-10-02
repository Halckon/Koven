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
             val (a, b) = echo(copy)\n\
             val (c, d) = moved\n\
             val (e, f) = ref\n\
         }\n\
         fun <T: Copyable> splitBound(pair: Pair<T, T>): Unit {\n\
             val (g, h) = pair\n\
         }\n\
         fun <T> splitUnbound(pair: Pair<T, T>): Unit {\n\
             val (i, j) = pair\n\
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
            symbol_named(&forward, &forward_names, uses_unit, "a"),
            symbol_named(&forward, &forward_names, uses_unit, "b"),
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
             val (one) = pair\n\
             val (x, y, extra) = pair\n\
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
        ["L0118", "L0118"]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("pattern span"))
            .collect::<Vec<_>>(),
        ["(one)", "(x, y, extra)"]
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
    assert_eq!(labels, ["Pair", "Pair"]);

    let uses_unit = source_unit(&forward_names, uses_source);
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "one",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "extra",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    assert!(forward.validate().is_err());
}
