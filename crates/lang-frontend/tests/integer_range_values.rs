//! SPEC-0328 ordinary source probes: no fabricated facts or language execution.
//! Stage A: retain Single; fixed diagnostics and differential checks are not retirement evidence.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{BinaryOperator, Expression, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Copyability, FinalTypeAdmissionError, IntegerRangeOperationKind, TypeKind,
        UnitExpressionId, UnitTypeKind, UnitTypeRefId, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    root: Span,
    operator_span: Option<Span>,
    range_fact_valid: Option<bool>,
    range_fact_count: usize,
    names: Vec<Diagnostic>,
    raw: Option<Vec<Diagnostic>>,
    shape: Option<String>,
    resolved: bool,
    boolean: bool,
    builtin: Option<BuiltinType>,
    copyable: bool,
    matches_annotation: Option<bool>,
    complete: Option<Result<(), FinalTypeAdmissionError>>,
}

fn observe(text: &str, root_text: &str, annotation: Option<&str>) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex API");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parse API");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let roots = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok(root_text))
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1, "unique root {root_text}");
    let (root_id, root_node) = roots[0];
    let binary = match root_node.payload() {
        Expression::Binary {
            left,
            right,
            operator,
            operator_span,
        } => Some((*left, *right, *operator, *operator_span)),
        _ => None,
    };
    let operator_span = binary.map(|(_, _, _, span)| span);
    let annotation_id = annotation.map(|text| {
        let found = file
            .ast()
            .type_refs()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text))
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "unique annotation {text}");
        found[0]
    });
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names API");
    let typed = check_types(&sources, &file, &names, &types).expect("single raw API");
    let root_type = typed.expression_type(root_id);
    let kind = root_type.and_then(|id| typed.types().get(id));
    let single = Observation {
        mode: "single",
        root: root_node.span(),
        operator_span,
        range_fact_valid: typed.integer_range_operation(root_id).map(|fact| {
            binary.is_some_and(|(left, right, operator, _)| {
                fact.expression() == root_id
                    && fact.left() == left
                    && fact.right() == right
                    && Some(fact.kind()) == range_kind(operator)
                    && typed.expression_type(left) == Some(fact.left_type())
                    && typed.expression_type(right) == Some(fact.right_type())
                    && root_type == Some(fact.result_type())
            })
        }),
        range_fact_count: typed.integer_range_operations().len(),
        names: names.diagnostics().to_vec(),
        raw: Some(typed.diagnostics().to_vec()),
        shape: Some(format!("{kind:?}")),
        resolved: matches!(kind, Some(kind) if !matches!(kind, TypeKind::Error | TypeKind::Deferred(_))),
        boolean: matches!(kind, Some(TypeKind::Builtin(BuiltinType::Boolean))),
        builtin: match kind {
            Some(TypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        },
        copyable: root_type.and_then(|id| typed.copyability(id)) == Some(Copyability::Copyable),
        matches_annotation: annotation_id
            .map(|id| root_type.is_some() && root_type == typed.type_ref_type(id)),
        complete: Some(
            complete_file_types(&sources, &file, &names, &types, &typed).map(|complete| {
                assert!(complete.is_same_analysis(&typed));
            }),
        ),
    };
    let mut result = vec![single];
    // Both Unit descriptors reuse the exact SourceMap, SourceId and parsed AST above.
    for (mode, input) in [
        (
            "unit rooted",
            SourceUnitInput::new("root", "range.ko", source, &file),
        ),
        (
            "unit memory",
            SourceUnitInput::memory_document(source, &file),
        ),
    ] {
        assert_eq!(input.source_id(), source);
        assert!(std::ptr::eq(input.parsed(), &file));
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index API");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names API");
        let name_diagnostics = names.diagnostics().to_vec();
        let unit = match names.validate() {
            Err(_) => Observation {
                mode,
                root: root_node.span(),
                operator_span,
                range_fact_valid: None,
                range_fact_count: 0,
                names: name_diagnostics,
                raw: None,
                shape: None,
                resolved: false,
                boolean: false,
                builtin: None,
                copyable: false,
                matches_annotation: None,
                complete: None,
            },
            Ok(names) => {
                let unit = names.names().index().source_units()[0].id();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .expect("unit raw API");
                let root_type = typed.expression_type(UnitExpressionId::new(unit, root_id));
                let kind = root_type.and_then(|id| typed.types().get(id));
                Observation {
                    mode,
                    root: root_node.span(),
                    operator_span,
                    range_fact_valid: typed
                        .integer_range_operation(UnitExpressionId::new(unit, root_id))
                        .map(|fact| {
                            binary.is_some_and(|(left, right, operator, _)| {
                                fact.expression() == UnitExpressionId::new(unit, root_id)
                                    && fact.left() == UnitExpressionId::new(unit, left)
                                    && fact.right() == UnitExpressionId::new(unit, right)
                                    && Some(fact.kind()) == range_kind(operator)
                                    && typed.expression_type(fact.left()) == Some(fact.left_type())
                                    && typed.expression_type(fact.right())
                                        == Some(fact.right_type())
                                    && root_type == Some(fact.result_type())
                            })
                        }),
                    range_fact_count: typed.integer_range_operations().len(),
                    names: name_diagnostics,
                    raw: Some(typed.diagnostics().to_vec()),
                    shape: Some(format!("{kind:?}")),
                    resolved: matches!(kind, Some(kind) if !matches!(kind, UnitTypeKind::Error | UnitTypeKind::Deferred(_))),
                    boolean: matches!(kind, Some(UnitTypeKind::Builtin(BuiltinType::Boolean))),
                    builtin: match kind {
                        Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
                        _ => None,
                    },
                    copyable: root_type.map(|id| typed.copyability(id))
                        == Some(Copyability::Copyable),
                    matches_annotation: annotation_id.map(|id| {
                        root_type.is_some()
                            && root_type == typed.type_ref_type(UnitTypeRefId::new(unit, id))
                    }),
                    complete: Some(
                        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                            .map(|complete| {
                                assert!(complete.is_same_analysis(&typed));
                            }),
                    ),
                }
            }
        };
        assert_same_observation(&result[0], &unit);
        if result.len() == 2 {
            assert_same_observation(&result[1], &unit);
        }
        result.push(unit);
    }
    assert_eq!(result.len(), 3, "Single, rooted Unit and MemoryDocument");
    for observation in &result {
        println!(
            "SOURCE {text:?} ROOT {root_text:?} MODE {} SPAN {:?} SHAPE {:?} OBS {observation:?}",
            observation.mode, observation.root, observation.shape
        );
    }
    result
}

// Compare full Diagnostic values (including message, severity, labels and notes),
// not a code-only projection. Internal admission errors are never accepted as parity.
fn assert_same_observation(single: &Observation, unit: &Observation) {
    assert_eq!(unit.root, single.root, "{unit:?}");
    assert_eq!(unit.operator_span, single.operator_span, "{unit:?}");
    assert_eq!(unit.names, single.names, "full name diagnostics: {unit:?}");
    // L0082 intentionally differs: Single adds a builtin-declaration label, Unit
    // does not. rejected_api independently freezes both complete diagnostics.
    let builtin_arguments = single.mode == "single"
        && single.raw.as_ref().is_some_and(|diagnostics| {
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0082")
        });
    if !builtin_arguments {
        assert_eq!(unit.raw, single.raw, "full raw diagnostics: {unit:?}");
    }
    match (&single.complete, &unit.complete) {
        (Some(Ok(())), Some(Ok(()))) => {}
        (
            Some(Err(FinalTypeAdmissionError::Diagnostics(expected))),
            Some(Err(FinalTypeAdmissionError::Diagnostics(actual))),
        ) => {
            if !builtin_arguments {
                assert_eq!(actual, expected, "full final diagnostics: {unit:?}");
            }
        }
        _ => panic!("final admission mismatch or internal failure: {single:?} / {unit:?}"),
    }
    assert_eq!(unit.range_fact_valid, single.range_fact_valid, "{unit:?}");
    assert_eq!(unit.range_fact_count, single.range_fact_count, "{unit:?}");
    assert_eq!(unit.resolved, single.resolved, "{unit:?}");
    assert_eq!(unit.boolean, single.boolean, "{unit:?}");
    assert_eq!(unit.builtin, single.builtin, "{unit:?}");
    assert_eq!(unit.copyable, single.copyable, "{unit:?}");
    assert_eq!(
        unit.matches_annotation, single.matches_annotation,
        "{unit:?}"
    );
}

fn range_kind(operator: BinaryOperator) -> Option<IntegerRangeOperationKind> {
    use IntegerRangeOperationKind as Kind;
    Some(match operator {
        BinaryOperator::InclusiveRange => Kind::Inclusive,
        BinaryOperator::ExclusiveRange => Kind::Exclusive,
        BinaryOperator::In => Kind::Contains,
        BinaryOperator::NotIn => Kind::NotContains,
        BinaryOperator::Equal => Kind::Equal,
        BinaryOperator::NotEqual => Kind::NotEqual,
        _ => return None,
    })
}

fn assert_complete(observation: &Observation) {
    assert_eq!(observation.range_fact_valid, Some(true), "{observation:?}");
    assert!(observation.names.is_empty(), "{observation:?}");
    assert!(
        observation.raw.as_ref().is_some_and(Vec::is_empty),
        "{observation:?}"
    );
    assert!(observation.resolved, "{observation:?}");
    assert!(
        matches!(observation.complete, Some(Ok(()))),
        "{observation:?}"
    );
}

#[test]
fn implicit_range_construction_has_resolved_copyable_value() {
    let mut all = Vec::new();
    for (ty, range) in [
        ("Int", BuiltinType::IntRange),
        ("Long", BuiltinType::LongRange),
    ] {
        for operator in ["..", "..<"] {
            let root = format!("left {operator} right");
            let text = format!("fun probe(left: {ty}, right: {ty}): Unit {{ val range = {root} }}");
            all.extend(
                observe(&text, &root, None)
                    .into_iter()
                    .map(|observation| (observation, range)),
            );
        }
    }
    // Operand/result identities and the exact operation kind are checked with the AST.
    for (observation, range) in all {
        assert_eq!(observation.builtin, Some(range));
        assert_eq!(observation.range_fact_count, 1);
        assert_complete(&observation);
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn explicit_range_identity_and_capabilities_are_available() {
    let mut all = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        let text = format!("fun probe(left: {ty}, right: {ty}): {range} = left..right");
        all.extend(observe(&text, "left..right", Some(range)));
        // Ordinary bound checking exercises Transferable without starting a thread.
        let text = format!(
            "fun <T: Transferable> accept(value: T): Unit {{}}\nfun probe(left: {ty}, right: {ty}): Unit {{\n val range: {range} = left..right\n val copy = range\n accept(copy)\n accept(range)\n}}"
        );
        all.extend(observe(&text, "left..right", Some(range)));
        let text = format!(
            "fun accept(value: IntRange): Unit {{}}\nfun accept(value: LongRange): Unit {{}}\nfun probe(left: {ty}, right: {ty}): Unit {{ accept(left..right) }}"
        );
        all.extend(observe(&text, "left..right", Some(range)));
    }
    for observation in all {
        assert_eq!(
            observation.range_fact_count, 1,
            "trial must not duplicate facts"
        );
        assert_complete(&observation);
        assert_eq!(
            observation.matches_annotation,
            Some(true),
            "{observation:?}"
        );
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn range_membership_and_equality_publish_boolean_results() {
    let mut all = Vec::new();
    for ty in ["Int", "Long"] {
        for root in [
            "value in (left..right)",
            "value !in (left..<right)",
            "(left..right) == (left..<right)",
            "(left..right) != (left..<right)",
        ] {
            let text = format!("fun probe(left: {ty}, right: {ty}, value: {ty}): Boolean = {root}");
            all.extend(observe(&text, root, None));
        }
    }
    for observation in all {
        assert_complete(&observation);
        assert!(observation.boolean, "{observation:?}");
    }
}

#[test]
fn range_values_cross_existing_storage_boundaries() {
    let mut all = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        for text in [
            format!(
                "fun accept(value: {range}): Unit {{}}\nfun probe(left: {ty}, right: {ty}): Unit {{\n val stored = left..right\n val copied = stored\n accept(copied)\n accept(stored)\n}}"
            ),
            format!(
                "fun pass(own value: {range}): {range} = value\nfun probe(left: {ty}, right: {ty}): {range} = pass(left..right)"
            ),
            format!(
                "class Holder(val value: {range})\nfun probe(left: {ty}, right: {ty}): Holder = Holder(left..right)"
            ),
            format!("fun probe(left: {ty}, right: {ty}): List<{range}> = listOf(left..right)"),
        ] {
            all.extend(observe(&text, "left..right", None));
        }
    }
    for observation in all {
        assert_complete(&observation);
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn invalid_range_endpoints_have_operand_diagnostics() {
    let mut all = Vec::new();
    let pairs = [
        ("Int", "Long"),
        ("Long", "Int"),
        ("Byte", "Byte"),
        ("Short", "Short"),
        ("UByte", "UByte"),
        ("UShort", "UShort"),
        ("UInt", "UInt"),
        ("ULong", "ULong"),
        ("Float", "Float"),
        ("Double", "Double"),
        ("Char", "Char"),
        ("Boolean", "Boolean"),
        ("String", "String"),
        ("Int?", "Int?"),
        ("Long?", "Long?"),
    ];
    for (left, right) in pairs {
        for operator in ["..", "..<"] {
            let root = format!("left {operator} right");
            let text =
                format!("fun probe(left: {left}, right: {right}): Unit {{ val bad = {root} }}");
            let observations = observe(&text, &root, None);
            let left_start = text.rfind("left").expect("left operand");
            let right_start = text.rfind("right").expect("right operand");
            let operator_start = left_start + "left ".len();
            let labels = [
                (
                    left_start..left_start + 4,
                    format!("left operand has type {left}"),
                ),
                (
                    right_start..right_start + 5,
                    format!("right operand has type {right}"),
                ),
            ];
            for observation in &observations {
                let raw = observation.raw.as_ref().expect("raw endpoint analysis");
                assert_eq!(raw.len(), 1);
                assert_frozen_diagnostic(
                    &raw[0],
                    "L0085",
                    "operator does not accept these operand types",
                    operator_start..operator_start + operator.len(),
                    &labels,
                );
            }
            all.extend(observations);
        }
    }
    for observation in all {
        assert!(observation.names.is_empty(), "{observation:?}");
        let raw = observation
            .raw
            .as_ref()
            .expect("types reached for known endpoint types");
        assert_eq!(raw.len(), 1, "{observation:?}");
        assert_eq!(raw[0].code().to_string(), "L0085", "{observation:?}");
        assert_eq!(Some(raw[0].primary_span()), observation.operator_span);
        assert_eq!(observation.range_fact_count, 0, "{observation:?}");
        assert_eq!(observation.range_fact_valid, None);
        let Some(Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics))) =
            &observation.complete
        else {
            panic!("source diagnostics required: {observation:?}");
        };
        assert_eq!(
            raw, final_diagnostics,
            "complete ordered diagnostics: {observation:?}"
        );
    }
}

// Fixed source-derived expectations, not regenerated from either checker.
fn assert_frozen_diagnostic(
    diagnostic: &Diagnostic,
    code: &str,
    message: &str,
    span: std::ops::Range<usize>,
    labels: &[(std::ops::Range<usize>, String)],
) {
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert_eq!(diagnostic.code().to_string(), code);
    assert_eq!(diagnostic.message(), message);
    assert_eq!(
        diagnostic.primary_span().start()..diagnostic.primary_span().end(),
        span
    );
    assert_eq!(
        diagnostic.details().len(),
        labels.len(),
        "no extra labels, notes or help"
    );
    for (detail, (span, message)) in diagnostic.details().iter().zip(labels) {
        let DiagnosticDetail::Label(label) = detail else {
            panic!("expected ordered label, got {detail:?}");
        };
        assert_eq!(
            label.span().source_id(),
            diagnostic.primary_span().source_id()
        );
        assert_eq!(label.span().start()..label.span().end(), *span);
        assert_eq!(label.message(), message);
    }
}

fn rejected_api(
    text: &str,
    root: &str,
    code: &str,
    span_text: &str,
) -> Vec<(Observation, String, std::ops::Range<usize>)> {
    let start = text.rfind(span_text).expect("diagnostic source token");
    observe(text, root, None)
        .into_iter()
        .map(|observation| {
            let span = start..start + span_text.len();
            let message = match code {
                "L0143" => "this type identity cannot be constructed",
                "L0165" => "type checking could not complete: the member's type is unresolved",
                "L0082" => "builtin type does not accept type arguments",
                "L0161" => "Map key type must be Hashable",
                _ => panic!("missing fixed diagnostic expectation for {code}"),
            };
            let labels = if code == "L0082" && observation.mode == "single" {
                let name_start = text[..start].rfind(": ").expect("return annotation") + 2;
                vec![(
                    name_start..start - 1,
                    "builtin type declared here".to_owned(),
                )]
            } else {
                Vec::new()
            };
            let Some(Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics))) =
                &observation.complete
            else {
                panic!("expected structured final rejection: {observation:?}");
            };
            assert_eq!(final_diagnostics.len(), 1);
            assert_frozen_diagnostic(&final_diagnostics[0], code, message, span.clone(), &labels);
            let raw = observation.raw.as_ref().expect("raw stage reached");
            if code == "L0165" {
                assert!(raw.is_empty());
            } else {
                assert_eq!(raw.len(), 1);
                assert_frozen_diagnostic(&raw[0], code, message, span.clone(), &labels);
            }
            (observation, code.to_owned(), span)
        })
        .collect()
}

#[test]
fn range_identity_does_not_enable_extra_public_apis() {
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        let text = format!("fun probe(left: {ty}, right: {ty}): {range} = left..right");
        positive.extend(observe(&text, "left..right", Some(range)));
        let call = format!("{range}()");
        negative.extend(rejected_api(
            &format!("fun probe(): Unit {{ val bad = {call} }}"),
            &call,
            "L0143",
            range,
        ));
        for member in ["first", "last", "step", "size", "hash"] {
            let root = format!("value.{member}");
            negative.extend(rejected_api(
                &format!("fun probe(value: {range}): Unit {{ val bad = {root} }}"),
                &root,
                "L0165",
                &root,
            ));
        }
        negative.extend(rejected_api(
            &format!("fun probe(left: {ty}, right: {ty}): {range}<Int> = left..right"),
            "left..right",
            "L0082",
            "Int",
        ));
        for container in ["Map", "MutableMap"] {
            negative.extend(rejected_api(
                &format!("fun probe(value: {container}<{range}, Int>): Unit {{ val marker = 0 }}"),
                "0",
                "L0161",
                range,
            ));
        }
    }
    for observation in positive {
        assert_complete(&observation);
        assert_eq!(
            observation.matches_annotation,
            Some(true),
            "{observation:?}"
        );
    }
    for (observation, code, span) in negative {
        assert!(observation.names.is_empty(), "{observation:?}");
        let Some(Err(FinalTypeAdmissionError::Diagnostics(ref diagnostics))) = observation.complete
        else {
            panic!("structured source rejection required: {observation:?}");
        };
        assert_eq!(diagnostics.len(), 1, "{observation:?}");
        assert_eq!(diagnostics[0].code().to_string(), code, "{observation:?}");
        assert_eq!(
            diagnostics[0].primary_span().source_id(),
            observation.root.source_id()
        );
        assert_eq!(
            diagnostics[0].primary_span().start()..diagnostics[0].primary_span().end(),
            span
        );
        let raw = observation.raw.as_ref().expect("actual types reached");
        if code == "L0165" {
            assert!(raw.is_empty(), "{observation:?}");
        } else {
            assert_eq!(raw, diagnostics, "original diagnostic preserved in full");
        }
        assert_eq!(
            observation.range_fact_count, 0,
            "no successful fact survives rejected source"
        );
        assert_eq!(observation.range_fact_valid, None, "{observation:?}");
    }
}
