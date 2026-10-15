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
        assert_eq!(ownership.len(), 1, "{entry:?}");
        assert_eq!(ownership[0].code().to_string(), "L0135");
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
            assert_eq!(codes, if captured { vec!["L0135"] } else { vec![] });
        }
    }
}

#[test]
fn expected_non_unit_lambda_rejects_assignment_result() {
    let text = "fun test(): Unit { val f: () -> Int = { var number = 1; number = 2 } }";
    for entry in observe(text) {
        assert_raw_rejection(&entry, "L0084", text, "number = 2");
    }
}

#[test]
fn invalid_rhs_preserves_storage_type_diagnostic() {
    for text in [
        "fun test(): Unit { var number: Int = 1; val f: () -> Unit = { number = true } }",
        "fun test(): Unit { val f: () -> Unit = { var number: Int = 1; number = true } }",
    ] {
        for entry in observe(text) {
            assert_raw_rejection(&entry, "L0084", text, "true");
        }
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
    let text = "fun test(): Unit { val f: () -> Unit = { var number = 1; number += 2 } }";
    for entry in observe(text) {
        assert_eq!(entry.assignments.len(), 1);
        assert_eq!(
            entry.assignments[0].2,
            Shape::AssignmentDeferred,
            "{entry:?}"
        );
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = &entry.complete else {
            panic!("{entry:?}");
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code().to_string(), "L0165");
        assert_eq!(diagnostics[0].primary_span(), entry.assignments[0].0);
        assert!(entry.ownership.is_none());
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
    let invalid = "fun test(): Unit { var current: Int? = 1; if (current != null) { current = null; val after: Int = current } }";
    for entry in observe(invalid) {
        assert_eq!(
            entry.current_uses.last().unwrap().1,
            Shape::Error,
            "{entry:?}"
        );
        assert_eq!(entry.raw.len(), 1, "{entry:?}");
        assert_eq!(entry.raw[0].code().to_string(), "L0084");
        let start = invalid.rfind("current").unwrap();
        let span = entry.raw[0].primary_span();
        assert_eq!((span.start(), span.end()), (start, start + "current".len()));
        let Err(FinalTypeAdmissionError::Diagnostics(actual)) = &entry.complete else {
            panic!("{entry:?}");
        };
        assert_eq!(actual, &entry.raw);
        assert!(entry.ownership.is_none());
    }
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
    for (text, shared_parameter) in [
        ("fun test(): Unit { val number = 1; number = 2 }", false),
        ("fun test(own number: Int): Unit { number = 2 }", false),
        ("fun test(number: Int): Unit { number = 2 }", true),
    ] {
        for entry in observe(text) {
            assert_unit_assignment(&entry, "number = 2");
            let diagnostics = entry.ownership.as_ref().unwrap();
            assert_eq!(diagnostics.len(), 1, "{entry:?}");
            // Preserve the pre-existing single shared-parameter loan priority.
            let code = if shared_parameter && entry.mode == "single" {
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
