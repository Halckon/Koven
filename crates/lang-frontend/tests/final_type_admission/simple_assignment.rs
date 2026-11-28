//! SPEC-0325: ordinary source, raw typing, final admission, then ownership.
//! No fabricated facts, native execution, or bypass of the final gate.
use super::*;
use lang_frontend::{
    diagnostic::Diagnostic,
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::Expression,
    source::Span,
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};

#[path = "simple_assignment/nullable_refinement.rs"]
mod nullable_refinement;

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Unit,
    Int,
    Nullable,
    AssignmentDeferred,
    Error,
    Other,
}

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    raw: Vec<Diagnostic>,
    assignments: Vec<(Span, String, Shape)>,
    current_uses: Vec<(Span, Shape)>,
    complete: Result<(), FinalTypeAdmissionError>,
    ownership: Option<Vec<Diagnostic>>,
}

fn single_shape(kind: Option<&TypeKind>) -> Shape {
    match kind {
        Some(TypeKind::Builtin(BuiltinType::Unit)) => Shape::Unit,
        Some(TypeKind::Builtin(BuiltinType::Int)) => Shape::Int,
        Some(TypeKind::Nullable(_)) => Shape::Nullable,
        Some(TypeKind::Deferred(DeferredReason::Assignment)) => Shape::AssignmentDeferred,
        Some(TypeKind::Error) => Shape::Error,
        _ => Shape::Other,
    }
}

fn unit_shape(kind: Option<&UnitTypeKind>) -> Shape {
    match kind {
        Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => Shape::Unit,
        Some(UnitTypeKind::Builtin(BuiltinType::Int)) => Shape::Int,
        Some(UnitTypeKind::Nullable(_)) => Shape::Nullable,
        Some(UnitTypeKind::Deferred(DeferredReason::Assignment)) => Shape::AssignmentDeferred,
        Some(UnitTypeKind::Error) => Shape::Error,
        _ => Shape::Other,
    }
}

fn observe(text: &str) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("single types");
    let raw = typed.diagnostics().to_vec();
    let mut assignments = Vec::new();
    let mut current_uses = Vec::new();
    for (id, node) in file.ast().expressions().iter() {
        let shape = single_shape(
            typed
                .expression_type(id)
                .and_then(|ty| typed.types().get(ty)),
        );
        if matches!(node.payload(), Expression::Assignment { .. }) {
            assignments.push((
                node.span(),
                sources.slice(node.span()).unwrap().to_owned(),
                shape,
            ));
        } else if matches!(node.payload(), Expression::Name)
            && sources.slice(node.span()) == Ok("current")
        {
            current_uses.push((node.span(), shape));
        }
    }
    let complete = complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), raw);
    eprintln!(
        "SPEC-0325 before ownership: raw={raw:?} assignments={assignments:?} current_uses={current_uses:?} complete={complete:?}"
    );
    let ownership = if complete.is_ok() {
        Some(
            check_ownership(&sources, &file, &names, &typed)
                .expect("single ownership")
                .diagnostics()
                .to_vec(),
        )
    } else {
        None
    };
    let single = Observation {
        mode: "single",
        raw,
        assignments,
        current_uses,
        complete,
        ownership,
    };

    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit = names.names().index().source_units()[0].id();
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
    let raw = typed.diagnostics().to_vec();
    let mut assignments = Vec::new();
    let mut current_uses = Vec::new();
    for (id, node) in file.ast().expressions().iter() {
        let shape = unit_shape(
            typed
                .expression_type(UnitExpressionId::new(unit, id))
                .and_then(|ty| typed.types().get(ty)),
        );
        if matches!(node.payload(), Expression::Assignment { .. }) {
            assignments.push((
                node.span(),
                sources.slice(node.span()).unwrap().to_owned(),
                shape,
            ));
        } else if matches!(node.payload(), Expression::Name)
            && sources.slice(node.span()) == Ok("current")
        {
            current_uses.push((node.span(), shape));
        }
    }
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), raw);
    eprintln!(
        "SPEC-0325 before ownership: raw={raw:?} assignments={assignments:?} current_uses={current_uses:?} complete={complete:?}"
    );
    let ownership = if complete.is_ok() {
        let typed = typed
            .validate()
            .expect("complete ordinary unit has basic capability");
        Some(
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .expect("unit ownership")
                .diagnostics()
                .to_vec(),
        )
    } else {
        None
    };
    let observations = vec![
        single,
        Observation {
            mode: "unit-basic",
            raw,
            assignments,
            current_uses,
            complete,
            ownership,
        },
    ];
    for entry in &observations {
        eprintln!("SPEC-0325 {entry:?}");
    }
    observations
}

fn assert_unit_assignment(entry: &Observation, expected: &str) {
    assert_eq!(entry.assignments.len(), 1, "{entry:?}");
    assert_eq!(entry.assignments[0].1, expected, "{}", entry.mode);
    assert_eq!(entry.assignments[0].2, Shape::Unit, "{entry:?}");
    assert!(entry.raw.is_empty(), "{entry:?}");
    assert!(entry.complete.is_ok(), "{entry:?}");
}

fn assert_raw_rejection(entry: &Observation, code: &str, text: &str, token: &str) {
    assert_eq!(entry.raw.len(), 1, "{entry:?}");
    assert_eq!(entry.raw[0].code().to_string(), code);
    let start = text.find(token).expect("unique expected token");
    assert_eq!(text.matches(token).count(), 1);
    let span = entry.raw[0].primary_span();
    assert_eq!((span.start(), span.end()), (start, start + token.len()));
    let Err(FinalTypeAdmissionError::Diagnostics(actual)) = &entry.complete else {
        panic!("{entry:?}");
    };
    assert_eq!(
        actual, &entry.raw,
        "original diagnostic including details is preserved"
    );
    assert!(entry.ownership.is_none());
}

#[test]
fn original_p42_capture_reaches_ownership() {
    let text = include_str!("../fixtures/simple-assignment-0325/p42_lambda_mutate.ko");
    for entry in observe(text) {
        assert_unit_assignment(&entry, "sum = sum + it");
        let ownership = entry.ownership.as_ref().unwrap();
        let codes: Vec<_> = ownership
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect();
        assert_eq!(
            codes,
            if entry.mode == "unit-basic" {
                vec![]
            } else {
                vec!["L0135"]
            }
        );
    }
}

#[test]
fn captured_and_local_tail_assignments_keep_ownership_boundary() {
    for (text, captured) in [
        (
            "fun test(): Unit { var number = 1; val f: () -> Unit = { number = 2 } }",
            true,
        ),
        (
            "fun test(): Unit { val f: () -> Unit = { var number = 1; number = 2 } }",
            false,
        ),
    ] {
        for entry in observe(text) {
            assert_unit_assignment(&entry, "number = 2");
            let codes = entry
                .ownership
                .as_ref()
                .unwrap()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            let expected = match (captured, entry.mode) {
                (true, "unit-basic") => vec![],
                (true, _) => vec!["L0135"],
                (false, _) => vec![],
            };
            assert_eq!(codes, expected, "{}", entry.mode);
        }
    }
}

#[test]
fn exclusive_capture_boundary_distinguishes_write_demands_from_immutable_captures() {
    for (text, unit_code) in [
        (
            "fun test(): Unit { var number = 1; val f: () -> Unit = { (number) = 2 } }",
            None,
        ),
        (
            "fun test(): Unit { var number = 1; val f: () -> Unit = move { number = 2 } }",
            Some("L0135"),
        ),
        (
            "fun test(inout number: Int): Unit { val f: () -> Unit = { number = 2 } }",
            Some("L0135"),
        ),
        (
            "fun test(): Unit { var number = 1; val f: () -> Unit = { val g: () -> Unit = { number = 2 } } }",
            Some("L0135"),
        ),
        (
            "fun test(): Unit { val f: () -> Unit = { var number = 1; val g: () -> Unit = { number = 2 } } }",
            None,
        ),
    ] {
        for entry in observe(text)
            .into_iter()
            .filter(|entry| entry.mode == "unit-basic")
        {
            let diagnostics = entry.ownership.as_ref().expect("ownership ran");
            let codes: Vec<_> = diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            assert_eq!(codes, unit_code.as_slice(), "{text}");
        }
    }
}

#[test]
fn expected_non_unit_lambda_rejects_assignment_result() {
    use lang_frontend::{
        diagnostic::{DiagnosticCodeCatalog, Severity},
        name_resolution::SourceUnitKey,
        parser::{AssignmentOperator, Item, NameMarker, Statement, TypeRef},
        type_checking::{ExpressionCategory, UnitTypeRefId},
    };

    let text = "fun test(): Unit { val f: () -> Int = { var number = 1; number = 2 } }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = standard_environments();
    let ast = file.ast();
    let start = sources.span(source, 26, 35).unwrap();
    let primary = sources.span(source, 56, 66).unwrap();
    assert_eq!(sources.slice(start), Ok("() -> Int"));
    let (annotation, lambda) = ast
        .items()
        .iter()
        .find_map(|(_, item)| match item.payload() {
            Item::Variable {
                name: NameMarker::Present(name),
                type_ref: Some(annotation),
                initializer,
                ..
            } if sources.slice(*name) == Ok("f") => Some((*annotation, *initializer)),
            _ => None,
        })
        .expect("the original context-bearing variable");
    let annotation_node = ast.type_refs().get(annotation).unwrap();
    assert_eq!(annotation_node.span(), start);
    let TypeRef::Function {
        parameters,
        return_type,
        ..
    } = annotation_node.payload()
    else {
        panic!("callable context is a real function annotation")
    };
    assert!(parameters.is_empty());
    assert_eq!(
        sources.slice(ast.type_refs().get(*return_type).unwrap().span()),
        Ok("Int")
    );

    let Expression::Lambda { body, .. } = *ast.expressions().get(lambda).unwrap().payload() else {
        panic!("the original lambda")
    };
    let Statement::LambdaBody { elements } = ast.statements().get(body).unwrap().payload() else {
        panic!("lambda tail scope")
    };
    let Statement::Expression { expression: tail } = *ast
        .statements()
        .get(*elements.last().unwrap())
        .unwrap()
        .payload()
    else {
        panic!("the contextual tail value")
    };
    let tail_node = ast.expressions().get(tail).unwrap();
    assert_eq!(tail_node.span(), primary);
    assert_eq!(sources.slice(primary), Ok("number = 2"));
    let Expression::Assignment {
        operator: AssignmentOperator::Assign,
        target,
        value,
        ..
    } = *tail_node.payload()
    else {
        panic!("original plain assignment")
    };
    let catalog = DiagnosticCodeCatalog::try_new(&["L0084"]).unwrap();
    let mut expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0084").unwrap(),
        "expression type does not match the expected type",
        primary,
    )
    .unwrap();
    expected
        .add_label(&sources, start, "expected Int, found Unit")
        .unwrap();

    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &file)),
        (true, SourceUnitInput::new("root", "main.ko", source, &file)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        assert!(match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "main.ko",
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(typed.diagnostics(), std::slice::from_ref(&expected));
        let unit = entry.id();
        let int = typed.types().builtin(BuiltinType::Int).unwrap();
        let annotation = typed
            .type_ref_type(UnitTypeRefId::new(unit, annotation))
            .expect("resolved callable type");
        assert!(
            matches!(typed.types().get(annotation), Some(UnitTypeKind::Function {
            move_only: false, parameters, return_type
        }) if parameters.is_empty() && *return_type == int)
        );
        let expr = |id| UnitExpressionId::new(unit, id);
        assert_eq!(typed.expression_type(expr(lambda)), Some(annotation));
        assert_eq!(typed.expression_type(expr(target)), Some(int));
        assert_eq!(typed.expression_type(expr(value)), Some(int));
        assert_eq!(
            typed.expression_category(expr(target)),
            Some(ExpressionCategory::Place)
        );
        assert_eq!(
            typed
                .expression_type(expr(tail))
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Error)
        );
        assert!(typed.assignments().is_empty());
        let Err(FinalTypeAdmissionError::Diagnostics(complete)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
        else {
            panic!("the contextual mismatch rejects final type admission before ownership")
        };
        assert_eq!(complete.as_slice(), typed.diagnostics());
        assert!(typed.validate().is_err());
    }
}

#[test]
fn invalid_rhs_preserves_storage_type_diagnostic() {
    let captured =
        "fun test(): Unit { var number: Int = 1; val f: () -> Unit = { number = true } }";
    for entry in observe(captured) {
        assert_raw_rejection(&entry, "L0084", captured, "true");
    }
    invalid_local_rhs_unit();
}

fn invalid_local_rhs_unit() {
    use lang_frontend::{
        diagnostic::{DiagnosticCodeCatalog, Severity},
        name_resolution::{ReferenceTarget, SourceUnitKey, SymbolKind},
        parser::{AssignmentOperator, Item, LiteralKind, NameMarker, VariableKind},
        type_checking::ExpressionCategory,
    };

    let text = "fun test(): Unit { val f: () -> Unit = { var number: Int = 1; number = true } }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = standard_environments();
    let ast = file.ast();
    let assignment = expression(&sources, &file, "number = true");
    let Expression::Assignment {
        operator: AssignmentOperator::Assign,
        target,
        value,
        ..
    } = *ast.expressions().get(assignment).unwrap().payload()
    else {
        panic!("original plain assignment")
    };
    let target_span = ast.expressions().get(target).unwrap().span();
    let rhs = ast.expressions().get(value).unwrap();
    let primary = sources.span(source, 71, 75).unwrap();
    let declaration = sources.span(source, 45, 51).unwrap();
    assert_eq!(rhs.span(), primary);
    assert!(matches!(
        rhs.payload(),
        Expression::Literal(LiteralKind::Boolean(true))
    ));
    assert_eq!(sources.slice(declaration), Ok("number"));

    let catalog = DiagnosticCodeCatalog::try_new(&["L0084"]).unwrap();
    let mut expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0084").unwrap(),
        "expression type does not match the expected type",
        primary,
    )
    .unwrap();
    expected
        .add_label(&sources, declaration, "expected Int, found Boolean")
        .unwrap();

    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &file)),
        (true, SourceUnitInput::new("root", "main.ko", source, &file)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        assert!(match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "main.ko",
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(typed.diagnostics(), std::slice::from_ref(&expected));
        let unit = entry.id();
        let target = UnitExpressionId::new(unit, target);
        let int = typed.types().builtin(BuiltinType::Int).unwrap();
        assert_eq!(typed.expression_type(target), Some(int));
        assert_eq!(
            typed.expression_category(target),
            Some(ExpressionCategory::Place)
        );
        let rhs_kind = typed
            .expression_type(UnitExpressionId::new(unit, value))
            .and_then(|ty| typed.types().get(ty));
        assert_eq!(rhs_kind, Some(&UnitTypeKind::Error));

        let local = names.names().source_units()[0].resolution();
        let binding = local
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == declaration)
            .unwrap();
        assert_eq!(binding.kind(), SymbolKind::Variable);
        assert!(ast.items().iter().any(|(_, item)| matches!(
            item.payload(), Item::Variable { kind: VariableKind::Var, name: NameMarker::Present(name), .. }
                if *name == declaration
        )));
        assert!(
            local
                .references()
                .iter()
                .any(|reference| reference.span() == target_span
                    && reference.target() == &ReferenceTarget::Symbol(binding.id()))
        );
        assert!(
            typed
                .body_symbol_types()
                .iter()
                .any(|(symbol, ty)| symbol.source_unit() == unit
                    && symbol.symbol() == binding.id()
                    && *ty == int)
        );

        assert!(typed.assignments().is_empty());
        let Err(FinalTypeAdmissionError::Diagnostics(complete)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
        else {
            panic!("the raw mismatch must fail final type admission")
        };
        assert_eq!(complete.as_slice(), typed.diagnostics());
        assert!(
            typed.validate().is_err(),
            "raw type failure must not grant Basic capability"
        );
    }
}

#[test]
fn grouped_variable_matches_plain_assignment() {
    for (target, assignment) in [("number", "number = 2"), ("(number)", "(number) = 2")] {
        let text = format!("fun test(): Unit {{ var number = 1; {target} = 2 }}");
        for entry in observe(&text) {
            assert_unit_assignment(&entry, assignment);
            assert!(entry.ownership.as_ref().unwrap().is_empty(), "{entry:?}");
        }
    }
}

#[test]
fn compound_assignment_keeps_deferred_boundary() {
    use lang_frontend::{
        diagnostic::{DiagnosticCodeCatalog, Severity},
        name_resolution::{ReferenceTarget, SourceUnitKey, SymbolKind},
        parser::{AssignmentOperator, Item, NameMarker, VariableKind},
    };

    let text = "fun test(): Unit { val f: () -> Unit = { var number = 1; number += 2 } }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = standard_environments();
    let assignment = expression(&sources, &file, "number += 2");
    let node = file.ast().expressions().get(assignment).unwrap();
    let Expression::Assignment {
        target,
        operator,
        operator_span,
        value,
    } = *node.payload()
    else {
        panic!("the original compound assignment")
    };
    assert_eq!(operator, AssignmentOperator::AddAssign);
    assert_eq!(sources.slice(operator_span), Ok("+="));
    let target_span = file.ast().expressions().get(target).unwrap().span();
    assert_eq!(sources.slice(target_span), Ok("number"));
    assert_eq!(value, expression(&sources, &file, "2"));
    let catalog = DiagnosticCodeCatalog::try_new(&["L0165"]).unwrap();
    assert_eq!(node.span(), sources.span(source, 57, 68).unwrap());
    let expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0165").unwrap(),
        "type checking could not complete: assignment type checking is unresolved",
        node.span(),
    )
    .unwrap();

    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &file)),
        (true, SourceUnitInput::new("root", "main.ko", source, &file)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        assert!(match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "main.ko",
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let local = names.names().source_units()[0].resolution();
        let binding = local
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "number")
            .unwrap();
        assert_eq!(binding.kind(), SymbolKind::Variable);
        assert!(file.ast().items().iter().any(|(_, item)| matches!(
            item.payload(), Item::Variable {
                kind: VariableKind::Var, name: NameMarker::Present(name), ..
            } if *name == binding.span()
        )));
        assert!(
            local
                .references()
                .iter()
                .any(|reference| reference.span() == target_span
                    && reference.target() == &ReferenceTarget::Symbol(binding.id()))
        );

        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let unit = entry.id();
        let typed_assignment = UnitExpressionId::new(unit, assignment);
        assert_eq!(
            unit_shape(
                typed
                    .expression_type(typed_assignment)
                    .and_then(|ty| typed.types().get(ty))
            ),
            Shape::AssignmentDeferred
        );
        assert!(
            typed.assignments().is_empty(),
            "no successful typed write descriptor"
        );
        let int = typed.types().builtin(BuiltinType::Int).unwrap();
        assert!(
            typed
                .body_symbol_types()
                .iter()
                .any(|(symbol, ty)| symbol.source_unit() == unit
                    && symbol.symbol() == binding.id()
                    && *ty == int)
        );

        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
        else {
            panic!("final type admission, not ownership, must reject the source")
        };
        assert_eq!(diagnostics, std::slice::from_ref(&expected));
        assert!(
            typed.validate().is_ok(),
            "basic shape alone is not final type admission"
        );
    }
}

#[test]
fn nullable_storage_rhs_clears_target_refinement() {
    let text = "fun test(): Unit { var current: Int? = 1; if (current != null) { current = null; val after: Int? = current } }";
    for entry in observe(text) {
        assert_unit_assignment(&entry, "current = null");
        assert_eq!(entry.current_uses.len(), 3, "{entry:?}");
        assert_eq!(entry.current_uses[0].1, Shape::Nullable);
        assert_eq!(entry.current_uses[1].1, Shape::Int);
        assert_eq!(entry.current_uses[2].1, Shape::Nullable);
        assert!(entry.ownership.as_ref().unwrap().is_empty(), "{entry:?}");
    }
    // A later Int read must not reuse the pre-assignment non-null fact.
    nullable_refinement::check(
        "fun test(): Unit { var current: Int? = 1; if (current != null) { current = null; val after: Int = current } }",
    );
}

#[test]
fn field_assignment_retains_existing_entry_difference() {
    let text = "class Item(var number: Int) { fun set(): Unit { number = 2 } }";
    for entry in observe(text) {
        assert_eq!(entry.assignments.len(), 1);
        assert_eq!(
            entry.assignments[0].2,
            if entry.mode == "single" {
                Shape::AssignmentDeferred
            } else {
                Shape::Unit
            },
            "{entry:?}"
        );
        assert!(entry.raw.is_empty(), "{entry:?}");
        assert!(entry.complete.is_ok(), "{entry:?}");
        let diagnostics = entry.ownership.as_ref().unwrap();
        assert_eq!(diagnostics.len(), 1, "{entry:?}");
        assert_eq!(
            diagnostics[0].code().to_string(),
            if entry.mode == "single" {
                "L0135"
            } else {
                "L0134"
            }
        );
        let span = diagnostics[0].primary_span();
        assert_eq!((span.start(), span.end()), (48, 54));
    }
}

#[test]
fn readonly_variable_assignments_remain_ownership_errors() {
    let original_own_parameter = "fun test(own number: Int): Unit { number = 2 }";
    for (text, formal, decl, target) in [
        (
            "fun test(): Unit { val number = 1; number = 2 }",
            false,
            23,
            35,
        ),
        (original_own_parameter, true, 13, 34),
    ] {
        readonly_assignment_unit(text, formal, decl, target);
    }

    let text = "fun test(number: Int): Unit { number = 2 }";
    for entry in observe(text) {
        assert_unit_assignment(&entry, "number = 2");
        let diagnostics = entry.ownership.as_ref().unwrap();
        assert_eq!(diagnostics.len(), 1, "{entry:?}");
        // Preserve the pre-existing single shared-parameter loan priority.
        let code = if entry.mode == "single" {
            "L0135"
        } else {
            "L0134"
        };
        assert_eq!(diagnostics[0].code().to_string(), code);
        let start = text.rfind("number").unwrap();
        let span = diagnostics[0].primary_span();
        assert_eq!((span.start(), span.end()), (start, start + "number".len()));
    }
}

#[test]
fn unit_value_context_and_right_associative_chain_use_storage_types() {
    let passing = "fun done(): Unit {}\nfun test(): Unit { var number = 1; var result: Unit = done(); val value: Unit = result = number = 2 }";
    for entry in observe(passing) {
        assert_eq!(entry.assignments.len(), 2, "{entry:?}");
        assert!(
            entry
                .assignments
                .iter()
                .all(|(_, _, shape)| *shape == Shape::Unit),
            "{entry:?}"
        );
        assert!(entry.raw.is_empty(), "{entry:?}");
        assert!(entry.complete.is_ok(), "{entry:?}");
        assert!(entry.ownership.as_ref().unwrap().is_empty(), "{entry:?}");
    }
    let rejected = "fun test(): Unit { var left = 1; var right = 2; left = right = 3 }";
    for entry in observe(rejected) {
        assert_raw_rejection(&entry, "L0084", rejected, "right = 3");
    }
}

fn readonly_assignment_unit(text: &str, own_parameter: bool, declaration: usize, target: usize) {
    use lang_frontend::{
        diagnostic::{DiagnosticCodeCatalog, Severity},
        name_resolution::{ReferenceTarget, SourceUnitKey, SymbolKind},
        parser::{AssignmentOperator, Item, NameMarker, VariableKind},
        type_checking::{ExpressionCategory, ParameterMode},
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = standard_environments();
    let span = |start| sources.span(source, start, start + "number".len()).unwrap();
    let (declaration, use_site) = (span(declaration), span(target));
    assert_eq!(sources.slice(declaration), Ok("number"));
    assert_eq!(sources.slice(use_site), Ok("number"));
    let catalog = DiagnosticCodeCatalog::try_new(&["L0134"]).unwrap();
    let mut expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0134").unwrap(),
        "assignment target is not a mutable place",
        use_site,
    )
    .unwrap();
    expected
        .add_label(&sources, declaration, "immutable binding declared here")
        .unwrap();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "main.ko", source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let [indexed] = index.source_units() else {
            panic!("one original source")
        };
        assert!(index.diagnostics().is_empty());
        assert_eq!(indexed.source_id(), source);
        assert!(match (rooted, indexed.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "main.ko",
            _ => false,
        });
        let unit = indexed.id();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let [assignment] = typed.assignments() else {
            panic!("one actual typed assignment")
        };
        assert_eq!(assignment.operator(), AssignmentOperator::Assign);
        assert!(assignment.falls_through());
        let int = typed.types().builtin(BuiltinType::Int).unwrap();
        assert_eq!(assignment.target_type(), int);
        assert_eq!(typed.expression_type(assignment.value()), Some(int));
        assert_eq!(
            typed.expression_type(assignment.expression()),
            typed.types().builtin(BuiltinType::Unit)
        );
        assert_eq!(
            typed.expression_category(assignment.target()),
            Some(ExpressionCategory::Place)
        );
        let spell = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), unit);
            file.ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        assert_eq!(spell(assignment.target()), use_site);
        assert_eq!(
            sources.slice(spell(assignment.expression())),
            Ok("number = 2")
        );
        assert_eq!(sources.slice(spell(assignment.value())), Ok("2"));
        let local = names.names().source_units()[0].resolution();
        let binding = local
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == declaration)
            .unwrap();

        assert!(
            local
                .references()
                .iter()
                .any(|reference| reference.span() == use_site
                    && reference.target() == &ReferenceTarget::Symbol(binding.id()))
        );
        if own_parameter {
            assert_eq!(binding.kind(), SymbolKind::ValueParameter);
            let [function] = typed.signatures().declarations() else {
                panic!("one original top-level signature")
            };
            let function = function.callable().expect("original test function");
            assert_eq!(function.name(), "test");
            let [parameter] = function.parameters() else {
                panic!("one original formal")
            };
            assert_eq!(parameter.name(), Some("number"));
            assert_eq!(parameter.mode(), ParameterMode::Value);
            assert_eq!(parameter.ty(), int);
            assert_eq!(sources.slice(parameter.span()), Ok("own number: Int"));
            let symbol = parameter.symbol().expect("source-qualified formal");
            assert_eq!(symbol.source_unit(), unit);
            assert_eq!(symbol.symbol(), binding.id());
            assert_eq!(typed.symbol_type(symbol), Some(int));
        } else {
            assert_eq!(binding.kind(), SymbolKind::Variable);
            assert_eq!(
            file.ast()
                .items()
                .iter()
                .filter(|(_, node)| matches!(
                    node.payload(),
                    Item::Variable { kind: VariableKind::Val, name: NameMarker::Present(name), .. }
                        if *name == declaration
                ))
                .count(),
            1
        );
            assert!(
                typed
                    .body_symbol_types()
                    .iter()
                    .any(|(symbol, ty)| symbol.source_unit() == unit
                        && symbol.symbol() == binding.id()
                        && *ty == int)
            );
        }

        let complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .expect("assignment types are fully admissible before ownership");
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert_eq!(owned.diagnostics(), std::slice::from_ref(&expected));
        assert!(owned.deferred().is_empty());
        assert!(owned.field_replacements().is_empty());
        assert!(owned.ownership_primitives().is_empty());
        assert!(owned.value_deliveries().is_empty());
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}
