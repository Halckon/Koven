use super::*;

fn assignment_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Assignment { .. }).then_some(id)
        })
        .collect()
}

#[test]
fn assignment_publishes_plain_storage_facts_and_keeps_compounds_deferred() {
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
         fun assigned(initial: Shape): Shape {\n\
             var current = initial\n\
             if (current is Shape.Circle) {\n\
                 current = current\n\
                 return current\n\
             }\n\
             return current\n\
         }\n\
         fun addAssign(inout number: Int): Unit { number += 2 }\n\
         fun subtractAssign(inout number: Int): Unit { number -= 2 }\n\
         fun multiplyAssign(inout number: Int): Unit { number *= 2 }\n\
         fun divideAssign(inout number: Int): Unit { number /= 2 }\n\
         fun remainderAssign(inout number: Int): Unit { number %= 2 }",
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
            .expect("forward assignment unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse assignment unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.assignments(), reverse.assignments());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    let current_uses = expressions_with_text(&sources, &uses, "current");
    assert_eq!(current_uses.len(), 5);
    let current_types = current_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .expect("every current use has a type")
        })
        .collect::<Vec<_>>();
    assert_eq!(current_types[0], root);
    for index in [1, 2] {
        assert!(matches!(
            forward.types().get(current_types[index]),
            Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
        ));
    }
    assert_eq!(current_types[3], root);
    assert_eq!(current_types[4], root);

    let assignments = assignment_expressions(&uses);
    assert_eq!(assignments.len(), 6);
    let replacement = expression_with_text(&sources, &uses, "current = current");
    let replacement_id = UnitExpressionId::new(uses_unit, replacement);
    let descriptor = forward
        .assignment(replacement_id)
        .expect("plain replacement publishes a descriptor");
    assert_eq!(descriptor.expression(), replacement_id);
    assert_eq!(descriptor.operator(), AssignmentOperator::Assign);
    assert_eq!(descriptor.target_type(), root);
    assert!(descriptor.falls_through());
    assert_eq!(
        sources.slice(
            uses.ast()
                .expressions()
                .get(descriptor.target().expression())
                .expect("assignment target")
                .span(),
        ),
        Ok("current")
    );
    assert_eq!(
        sources.slice(
            uses.ast()
                .expressions()
                .get(descriptor.value().expression())
                .expect("assignment value")
                .span(),
        ),
        Ok("current")
    );
    assert_eq!(forward.assignments(), [descriptor]);
    assert!(matches!(
        forward
            .expression_type(replacement_id)
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Unit))
    ));
    assert!(
        assignments
            .into_iter()
            .filter(|&expression| expression != replacement)
            .all(|expression| matches!(
                forward
                    .expression_type(UnitExpressionId::new(uses_unit, expression))
                    .and_then(|ty| forward.types().get(ty)),
                Some(UnitTypeKind::Deferred(DeferredReason::Assignment))
            ))
    );
}

#[test]
fn assignment_rejects_mismatched_rhs_without_publishing_a_descriptor() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assignment-error.ko",
        "package p\nfun bad(inout number: Int): Unit { number = false }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/assignment-error.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("assignment mismatch stays in the recovery product");

    assert_eq!(typed.body_diagnostics().len(), 1);
    let diagnostic = &typed.body_diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0084");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok("false"));
    let expected_label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("expected type origin label");
    assert_eq!(sources.slice(expected_label.span()), Ok("number"));
    let target = expression_with_text(&sources, &file, "number");
    let target_span = file
        .ast()
        .expressions()
        .get(target)
        .expect("assignment target")
        .span();
    assert_ne!(expected_label.span(), target_span);
    assert!(expected_label.span().start() < target_span.start());
    assert!(typed.assignments().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn assignment_contextual_mismatch_rolls_back_its_descriptor() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assignment-context-error.ko",
        "package p\nfun bad(inout number: Int): Int = number = 1",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/assignment-context-error.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("contextual mismatch stays in the recovery product");

    assert_eq!(typed.body_diagnostics().len(), 1);
    assert_eq!(typed.body_diagnostics()[0].code().to_string(), "L0084");
    assert_eq!(
        sources.slice(typed.body_diagnostics()[0].primary_span()),
        Ok("number = 1")
    );
    let unit = source_unit(&names, source);
    let assignment = assignment_expressions(&file)
        .into_iter()
        .next()
        .expect("assignment expression");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(unit, assignment))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    assert!(typed.assignments().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn assignment_publishes_group_field_and_nothing_control_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assignment-places.ko",
        "package p\n\
         class Cell(var payload: Int) {\n\
             fun updateBare(inout other: Int): Unit { payload = other }\n\
             fun updateGroup(inout other: Int): Unit { (other) = payload }\n\
             fun stop(): Unit { this.payload = error(\"stop\") }\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/assignment-places.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("assignment place facts succeed");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = source_unit(&names, source);
    let int = typed.types().builtin(BuiltinType::Int).expect("Int seed");
    let assignments = typed.assignments();
    assert_eq!(assignments.len(), 3);
    assert_eq!(
        assignments
            .iter()
            .map(|descriptor| {
                sources.slice(
                    file.ast()
                        .expressions()
                        .get(descriptor.target().expression())
                        .expect("assignment target")
                        .span(),
                )
            })
            .collect::<Vec<_>>(),
        [Ok("payload"), Ok("(other)"), Ok("this.payload")]
    );
    assert!(assignments.iter().all(|descriptor| {
        descriptor.operator() == AssignmentOperator::Assign && descriptor.target_type() == int
    }));
    assert!(assignments[0].falls_through());
    assert!(assignments[1].falls_through());
    assert!(!assignments[2].falls_through());
    for expression in assignment_expressions(&file) {
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        ));
    }
    assert!(typed.validate().is_ok());
}

#[test]
fn overload_lambda_assignment_trial_commits_only_the_unique_cross_file_candidate() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "declarations.ko",
        "package p\n\
         fun resolve(callback: (Int) -> Int): Int = 1\n\
         fun resolve(callback: (String) -> String): String = \"text\"\n\
         fun intResult(input: Int): Int = input\n\
         fun pick(input: Int, callback: (Int) -> Int): Int = input\n\
         fun pick(input: String, callback: (String) -> String): String = input\n\
         fun makeInt(): Int = 1\n\
         fun mutate(callback: (Int) -> Unit): Unit {}\n\
         fun mutate(callback: (String) -> Unit): String = \"text\"",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(inout target: Int): Unit {\n\
             val selected = resolve({ item -> intResult(item) })\n\
             val filtered = pick(makeInt(), { filteredItem -> filteredItem })\n\
             val mutated = mutate({ candidate -> target = candidate })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("unique overload-lambda trial succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed overload-lambda trial succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.assignments(), reverse.assignments());
    assert_eq!(forward.calls().len(), 5);
    assert!(forward.calls()[..4].iter().all(|call| {
        forward.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    }));
    assert_eq!(forward.assignments().len(), 1);
    assert_eq!(
        forward.types().get(forward.assignments()[0].target_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    let uses_unit = source_unit(&forward_names, uses_source);
    let parameter = symbol_named(&forward, &forward_names, uses_unit, "item");
    assert_eq!(
        forward.body_parameter_mode(parameter),
        Some(ParameterMode::Borrow)
    );
    assert_eq!(
        forward
            .symbol_type(parameter)
            .and_then(|ty| forward.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
}

#[test]
fn failed_and_ambiguous_assignment_trials_leak_no_candidate_facts() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "declarations.ko",
        "package p\n\
         fun resolve(callback: (Int) -> Unit): Int = 1\n\
         fun resolve(callback: (String) -> Unit): String = \"text\"",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(inout target: Int): Unit {\n\
             val ambiguous = resolve({ first -> target = 1 })\n\
             val noMatch = resolve({ second -> target = true })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("overload-lambda failures stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed overload-lambda failures stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.assignments(), reverse.assignments());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0124", "L0123"]
    );
    assert_eq!(
        forward.body_diagnostics()[0].message(),
        "call remains ambiguous after argument type checking"
    );
    assert_eq!(forward.body_diagnostics()[0].details().len(), 2);
    assert!(
        forward.body_diagnostics()[0]
            .details()
            .iter()
            .all(|detail| {
                matches!(
                    detail,
                    DiagnosticDetail::Label(label)
                        if sources.slice(label.span()) == Ok("resolve")
                            && label.message() == "matching callable declared here"
                )
            })
    );
    assert!(forward.calls().is_empty());
    assert!(forward.assignments().is_empty());
    let unit = source_unit(&forward_names, uses_source);
    assert!(forward.body_parameter_modes().is_empty());
    let lambda_parameters = forward_names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
        .collect::<Vec<_>>();
    assert_eq!(lambda_parameters.len(), 2);
    for symbol in lambda_parameters {
        assert!(!forward.body_symbol_types().keys().any(|candidate| {
            candidate.source_unit() == unit && candidate.symbol() == symbol.id()
        }));
    }
    assert!(forward.validate().is_err());
}
