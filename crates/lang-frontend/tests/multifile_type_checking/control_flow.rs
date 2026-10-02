use super::*;

fn if_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::If { .. }).then_some(id))
        .collect()
}

#[test]
fn if_control_facts_feed_cross_file_calls_and_survive_input_permutation() {
    let mut sources = SourceMap::new();
    let (library_source, library) =
        parsed(&mut sources, "library.ko", "package p\nfun one(): Int = 1");
    let (caller_source, caller) = parsed(
        &mut sources,
        "caller.ko",
        "package p\n\
         fun choose(flag: Boolean): Int = if (flag) {\n\
         one() + 1\n\
         } else {\n\
         return one()\n\
         }\n\
         fun observe(flag: Boolean): Unit {\n\
         if (flag) one()\n\
         }\n\
         fun complete(flag: Boolean): Int {\n\
         if (flag) { return one() } else { return one() }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/library.ko", library_source, &library),
        SourceUnitInput::new("root", "p/caller.ko", caller_source, &caller),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward if unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse if unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.calls(), reverse.calls());
    let caller_unit = source_unit(&forward_names, caller_source);
    let ifs = if_expressions(&caller);
    assert_eq!(ifs.len(), 3);
    let int = forward.types().builtin(BuiltinType::Int).expect("Int seed");
    let unit = forward
        .types()
        .builtin(BuiltinType::Unit)
        .expect("Unit seed");
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[0])),
        Some(int)
    );
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[1])),
        Some(unit)
    );
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[2])),
        forward.types().builtin(BuiltinType::Nothing)
    );
    assert_eq!(forward.calls().len(), 5);
    assert!(forward.calls().iter().all(|call| {
        call.target() == UnitCallTarget::Declaration(declaration(&forward_names, "one"))
    }));
}

#[test]
fn if_expected_condition_and_join_errors_recover_without_cascades() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "if-errors.ko",
        "package p\n\
         fun contextual(flag: Boolean): Byte = if (flag) 127 else 128\n\
         fun mixed(flag: Boolean): Unit { val mixedResult = if (flag) { 1 } else { false } }\n\
         fun invalidCondition(): Unit { if (1) {} }\n\
         fun sound(flag: Boolean): Int = if (flag) { 1 } else { 2 }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/if-errors.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("if errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0090", "L0089", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span belongs to the source"))
            .collect::<Vec<_>>(),
        ["128", "else", "1"]
    );
    let sound = UnitExpressionId::new(
        source_unit(&names, source),
        expression_with_text(&sources, &file, "if (flag) { 1 } else { 2 }"),
    );
    assert_eq!(
        typed
            .expression_type(sound)
            .and_then(|ty| typed.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert!(typed.validate().is_err());
}

#[test]
fn nested_if_return_uses_the_callable_return_annotation() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nested-return.ko",
        "package p\n\
         fun nestedReturn(flag: Boolean): Int {\n\
         val text: String = if (flag) { return false } else { \"ok\" }\n\
         return 1\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/nested-return.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("nested return mismatch stays in the recovery product");

    assert_eq!(typed.body_diagnostics().len(), 1);
    let diagnostic = &typed.body_diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0084");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok("false"));
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("return mismatch labels the callable annotation");
    assert_eq!(sources.slice(label.span()), Ok("Int"));
    assert!(typed.validate().is_err());
}

#[test]
fn cross_file_enum_tests_publish_stable_flow_facts_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         enum class Shape { Circle(radius: Int), Point }\n\
         class Token {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun choose(shape: Shape): Shape =\n\
             if (shape is Shape.Circle) shape else shape\n\
         fun afterExit(shape: Shape): Shape {\n\
             if (shape !is Shape.Circle) { return shape }\n\
             return shape\n\
         }\n\
         fun conjunction(shape: Shape): Boolean =\n\
             shape is Shape.Circle && shape is Shape.Circle\n\
         fun disjunction(shape: Shape): Boolean =\n\
             shape !is Shape.Circle || shape is Shape.Circle\n\
         fun local(initial: Shape): Shape {\n\
             var current = initial\n\
             if (current is Shape.Circle) return current\n\
             return initial\n\
         }\n\
         fun grouped(shape: Shape): Shape =\n\
             if (!(shape !is Shape.Circle)) shape else shape\n\
         fun nullable(input: Token?): Token? =\n\
             if (input is Token) input else input",
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
            .expect("forward enum-flow unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse enum-flow unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.type_ref_types(), reverse.type_ref_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    let shape_uses = expressions_with_text(&sources, &uses, "shape");
    assert_eq!(shape_uses.len(), 13);
    let actual_kinds = shape_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .and_then(|ty| forward.types().get(ty))
                .expect("every shape use has a type")
        })
        .collect::<Vec<_>>();
    for index in [0, 2, 3, 4, 6, 8, 10, 12] {
        assert_eq!(
            actual_kinds[index],
            &UnitTypeKind::Nominal {
                declaration: declaration(&forward_names, "Shape"),
                arguments: Vec::new(),
            }
        );
    }
    for index in [1, 5, 7, 9, 11] {
        assert!(matches!(
            actual_kinds[index],
            UnitTypeKind::EnumCase { root: case_root, .. } if *case_root == root
        ));
    }
    let case_refs = type_refs_with_text(&sources, &uses, "Shape.Circle");
    assert_eq!(case_refs.len(), 8);
    assert!(case_refs.iter().all(|&type_ref| matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(uses_unit, type_ref))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    )));
    let current_uses = expressions_with_text(&sources, &uses, "current");
    assert_eq!(current_uses.len(), 2);
    assert_eq!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, current_uses[0]))
            .and_then(|ty| forward.types().get(ty)),
        Some(&UnitTypeKind::Nominal {
            declaration: declaration(&forward_names, "Shape"),
            arguments: Vec::new(),
        })
    );
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, current_uses[1]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    ));
    let token = forward
        .signatures()
        .declaration(declaration(&forward_names, "Token"))
        .expect("Token signature")
        .ty();
    let input_uses = expressions_with_text(&sources, &uses, "input");
    assert_eq!(input_uses.len(), 3);
    let input_types = input_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .expect("every nullable input use has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        forward.types().get(input_types[0]),
        Some(UnitTypeKind::Nullable(inner)) if *inner == token
    ));
    assert_eq!(input_types[1], token);
    assert_eq!(input_types[2], input_types[0]);
}

#[test]
fn invalid_cross_file_type_tests_and_case_annotations_keep_precise_diagnostics() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         enum class Shape { Circle, Point }\n\
         class Other {}\n\
         interface Marker",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun unrelated(shape: Shape): Boolean = shape is Other\n\
         fun interfaceTest(shape: Shape): Boolean = shape !is Marker\n\
         fun illegalAnnotation(shape: Shape): Unit {\n\
             val case: Shape.Circle? = shape\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid tests stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0106", "L0106", "L0114"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span belongs to source"))
            .collect::<Vec<_>>(),
        ["is", "!is", "Shape.Circle?"]
    );
    let labels = typed
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
                .expect("diagnostic has a source label")
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Other", "Marker", "Shape"]);
    assert!(typed.validate().is_err());
}

#[test]
fn cross_file_when_covers_enum_boolean_nullable_and_statement_contexts() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nenum class Shape { Circle, Point }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun classify(shape: Shape): Shape = when (shape) {\n\
             is Shape.Circle -> shape\n\
             is Shape.Point -> shape\n\
         }\n\
         fun negative(shape: Shape): Int = when (shape) {\n\
             !is Shape.Circle -> 0\n\
             is Shape.Circle -> 1\n\
         }\n\
         fun alternatives(choice: Shape): Shape = when (choice) {\n\
             is Shape.Circle, is Shape.Point -> choice\n\
         }\n\
         fun noRemaining(remaining: Shape): Shape = when (remaining) {\n\
             is Shape.Circle -> remaining\n\
             else -> remaining\n\
         }\n\
         fun boolean(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             false -> 0\n\
         }\n\
         fun nullable(shape: Shape?): Int = when (shape) {\n\
             null -> 0\n\
             is Shape.Circle -> 1\n\
             is Shape.Point -> 2\n\
         }\n\
         fun subjectless(flag: Boolean): Int = when {\n\
             flag -> 1\n\
             else -> 0\n\
         }\n\
         fun inferred(flag: Boolean): Unit {\n\
             val mixed = when (flag) { true -> 1; false -> \"text\" }\n\
         }\n\
         fun statement(shape: Shape): Int {\n\
             when (shape) { is Shape.Circle -> shape }\n\
             return 0\n\
         }\n\
         fun nested(flag: Boolean, shape: Shape): Int {\n\
             if (flag) {\n\
                 when (shape) { is Shape.Circle -> shape }\n\
             }\n\
             return 0\n\
         }\n\
         fun terminal(flag: Boolean): Int {\n\
             when (flag) {\n\
                 true -> return 1\n\
                 false -> return 0\n\
             }\n\
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
            .expect("forward when unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse when unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let classify_uses = expressions_with_text(&sources, &uses, "shape");
    assert!(classify_uses.len() >= 3);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, classify_uses[0])),
        Some(root)
    );
    for expression in &classify_uses[1..3] {
        assert!(matches!(
            forward
                .expression_type(UnitExpressionId::new(uses_unit, *expression))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
        ));
    }
    let alternative_uses = expressions_with_text(&sources, &uses, "choice");
    assert_eq!(alternative_uses.len(), 2);
    assert!(alternative_uses.iter().all(|expression| {
        forward.expression_type(UnitExpressionId::new(uses_unit, *expression)) == Some(root)
    }));
    let remaining_uses = expressions_with_text(&sources, &uses, "remaining");
    assert_eq!(remaining_uses.len(), 3);
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, remaining_uses[0])),
        Some(root)
    );
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, remaining_uses[1]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    ));
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, remaining_uses[2])),
        Some(root),
        "v0.35 remaining-domain facts are not active under v0.32"
    );
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "mixed",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Any))
    ));
    let when_nodes = when_expressions(&uses);
    assert_eq!(when_nodes.len(), 11);
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, when_nodes[8]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Unit))
    ));
}

#[test]
fn cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nenum class Shape { Circle, Point }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun missing(shape: Shape): Int = when (shape) {\n\
             is Shape.Circle -> 1\n\
         }\n\
         fun invalid(): Int = when { 1 -> 1; else -> 0 }\n\
         fun repeated(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             true -> 2\n\
         }\n\
         fun misplaced(flag: Boolean): Int = when (flag) {\n\
             else -> 0\n\
             true -> 1\n\
             else -> 2\n\
         }\n\
         fun branchJoin(flag: Boolean, inout number: Int): Unit {\n\
             val result = when (flag) {\n\
                 true -> number = number\n\
                 false -> 0\n\
             }\n\
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
            .expect("when errors stay in recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse when errors stay in recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0111", "L0107", "L0111", "L0110", "L0109", "L0108",]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["when", "1", "when", "true", "else", "else"]
    );
    let missing_case = forward.body_diagnostics()[0]
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("enum non-exhaustiveness labels the missing case");
    assert_eq!(missing_case, "Point");
    // Guide06: other known branch types join as Any; Unit and Int are not a conflict.
    let uses_unit = source_unit(&forward_names, uses_source);
    let joined = symbol_named(&forward, &forward_names, uses_unit, "result");
    assert_eq!(
        forward.types().get(forward.symbol_type(joined).unwrap()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Any))
    );
    let joined_when = *when_expressions(&uses).last().unwrap();
    assert_eq!(
        forward.types().get(
            forward
                .expression_type(UnitExpressionId::new(uses_unit, joined_when))
                .unwrap()
        ),
        Some(&UnitTypeKind::Builtin(BuiltinType::Any))
    );
    assert!(forward.validate().is_err());
}

#[test]
fn cross_file_loops_publish_jump_and_deferred_binding_facts() {
    let mut sources = SourceMap::new();
    let (callee_source, callee) = parsed(
        &mut sources,
        "callee.ko",
        "package p\nfun truth(): Boolean = true",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun loops(): Unit {\n\
             while (truth()) { continue }\n\
             for (item in truth()) {\n\
                 val deferred = item\n\
                 break\n\
             }\n\
             loop { break }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/callee.ko", callee_source, &callee),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward loop unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse loop unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    let uses_unit = source_unit(&forward_names, uses_source);
    for jump in ["continue", "break"] {
        assert!(
            expressions_with_text(&sources, &uses, jump)
                .iter()
                .all(|id| {
                    forward
                        .expression_type(UnitExpressionId::new(uses_unit, *id))
                        .and_then(|ty| forward.types().get(ty))
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
                })
        );
    }
    for symbol in ["item", "deferred"] {
        assert!(matches!(
            forward
                .symbol_type(symbol_named(&forward, &forward_names, uses_unit, symbol,))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::LoopSource))
        ));
    }
}

#[test]
fn unit_loop_and_return_diagnostics_match_callable_boundaries() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-loops.ko",
        "package p\n\
         fun invalid(): Unit {\n\
             break\n\
             continue\n\
             while (1) { break }\n\
             loop { break }\n\
             break\n\
             return 1\n\
         }\n\
         fun bare(): Int { return }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-loops.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("loop and return failures stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0142", "L0142", "L0084", "L0142", "L0087", "L0087"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["break", "continue", "1", "break", "1", "return"]
    );
    assert!(typed.validate().is_err());
}
