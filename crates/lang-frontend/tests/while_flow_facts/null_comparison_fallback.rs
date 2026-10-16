//! SPEC-0330：只读观察真实 null comparison facts，不注入或改写 typed 产物。
use super::*;
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SourceUnitId, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    parser::{BinaryOperator, LiteralKind},
};

#[derive(Debug)]
pub(super) struct ComparisonObservation {
    span: Span,
    read_type: Option<Shape>,
    null_type: Option<Shape>,
    result_type: Option<Shape>,
    non_null_use: bool,
    descriptor: Option<DescriptorObservation>,
}

#[derive(Debug)]
struct DescriptorObservation {
    expression_matches: bool,
    symbol_matches_read: bool,
    storage_matches_symbol: bool,
    storage_type: Shape,
    null_matches_storage: bool,
    read_matches_storage_inner: bool,
    non_null_when_true: bool,
}

// Group 只用于定位真实 Name reference；生产稳定性判断仍由公开 descriptor 证明。
fn name_span(file: &ParsedFile, mut expression: ExpressionId) -> Option<Span> {
    loop {
        let node = file.ast().expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name => return Some(node.span()),
            _ => return None,
        }
    }
}

fn comparison_nodes(file: &ParsedFile) -> Vec<(ExpressionId, Span, ExpressionId, ExpressionId)> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Binary {
                left,
                operator,
                right,
                ..
            } = node.payload()
            else {
                return None;
            };
            if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
                return None;
            }
            let is_null = |id| {
                file.ast().expressions().get(id).is_ok_and(|node| {
                    matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                })
            };
            let (candidate, null) = match (is_null(*left), is_null(*right)) {
                (false, true) => (*left, *right),
                (true, false) => (*right, *left),
                _ => return None,
            };
            Some((id, node.span(), candidate, null))
        })
        .collect()
}

pub(super) fn observe_single(
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Vec<ComparisonObservation> {
    comparison_nodes(file)
        .into_iter()
        .map(|(expression, span, candidate, null)| {
            let read_type = typed.expression_type(candidate);
            let null_type = typed.expression_type(null);
            let descriptor = typed.null_comparison(expression).map(|fact| {
                let symbol = names.references().iter().find_map(|reference| {
                    if Some(reference.span()) != name_span(file, candidate) {
                        return None;
                    }
                    match reference.target() {
                        ReferenceTarget::Symbol(symbol) => Some(*symbol),
                        _ => None,
                    }
                });
                DescriptorObservation {
                    expression_matches: fact.expression() == expression,
                    symbol_matches_read: symbol == Some(fact.symbol()),
                    storage_matches_symbol: typed.symbol_type(fact.symbol()) == Some(fact.nullable_type()),
                    storage_type: single_shape(typed, fact.nullable_type()),
                    null_matches_storage: null_type == Some(fact.nullable_type()),
                    read_matches_storage_inner: matches!(typed.types().get(fact.nullable_type()), Some(TypeKind::Nullable(inner)) if read_type == Some(*inner)),
                    non_null_when_true: fact.non_null_when_true(),
                }
            });
            ComparisonObservation {
                span,
                read_type: read_type.map(|ty| single_shape(typed, ty)),
                null_type: null_type.map(|ty| single_shape(typed, ty)),
                result_type: typed.expression_type(expression).map(|ty| single_shape(typed, ty)),
                non_null_use: typed.non_null_use(candidate).is_some(),
                descriptor,
            }
        })
        .collect()
}

pub(super) fn observe_unit(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
) -> Vec<ComparisonObservation> {
    comparison_nodes(file)
        .into_iter()
        .map(|(expression, span, candidate, null)| {
            let expression = UnitExpressionId::new(unit, expression);
            let candidate_id = UnitExpressionId::new(unit, candidate);
            let read_type = typed.expression_type(candidate_id);
            let null_type = typed.expression_type(UnitExpressionId::new(unit, null));
            let descriptor = typed.null_comparison(expression).map(|fact| {
                let symbol = names.names().references().iter().find_map(|reference| {
                    if reference.source_unit() != unit
                        || Some(reference.span()) != name_span(file, candidate)
                    {
                        return None;
                    }
                    match reference.target() {
                        UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                        _ => None,
                    }
                });
                DescriptorObservation {
                    expression_matches: fact.expression() == expression,
                    symbol_matches_read: symbol == Some(fact.symbol()),
                    storage_matches_symbol: typed.symbol_type(fact.symbol()) == Some(fact.nullable_type()),
                    storage_type: unit_shape(typed, fact.nullable_type()),
                    null_matches_storage: null_type == Some(fact.nullable_type()),
                    read_matches_storage_inner: matches!(typed.types().get(fact.nullable_type()), Some(UnitTypeKind::Nullable(inner)) if read_type == Some(*inner)),
                    non_null_when_true: fact.non_null_when_true(),
                }
            });
            ComparisonObservation {
                span,
                read_type: read_type.map(|ty| unit_shape(typed, ty)),
                null_type: null_type.map(|ty| unit_shape(typed, ty)),
                result_type: typed.expression_type(expression).map(|ty| unit_shape(typed, ty)),
                non_null_use: typed.non_null_use(candidate_id).is_some(),
                descriptor,
            }
        })
        .collect()
}

fn mark(name: &'static str, text: &'static str, shape: Shape) -> Mark {
    Mark {
        name,
        text,
        kind: NodeKind::Other,
        shape,
    }
}

// 与既有 helper 相同：先完成所有源码、两个入口的 raw/complete 收集并输出，再开始 oracle。
fn collect(cases: &[Case]) -> Vec<(&Case, bool, Result<Observation, String>)> {
    let observations = cases
        .iter()
        .flat_map(|case| {
            [false, true]
                .into_iter()
                .map(move |unit| (case, unit, observe_with_comparisons(case, unit, true)))
        })
        .collect::<Vec<_>>();
    for (case, unit, observation) in &observations {
        eprintln!(
            "SPEC-0330 null fallback case={} unit={unit} source={:?} observation={observation:#?}",
            case.name, case.text
        );
        if let Ok(observation) = observation
            && let Ok(typed) = &observation.typed
        {
            eprintln!("SPEC-0330 null comparisons={:#?}", typed.comparisons);
        }
    }
    observations
}

fn assert_observation(
    case: &Case,
    unit: bool,
    observation: Result<Observation, String>,
) -> TypeObservation {
    let context = format!("{} unit={unit}", case.name);
    let observation = observation.unwrap_or_else(|error| panic!("{context}: {error}"));
    assert!(observation.lex.is_empty(), "{context}: {observation:?}");
    assert!(observation.parse.is_empty(), "{context}: {observation:?}");
    assert!(observation.names.is_empty(), "{context}: {observation:?}");
    let typed = observation
        .typed
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    assert_eq!(typed.raw_after_complete, typed.raw, "{context}");
    assert_eq!(typed.marks.len(), case.marks.len(), "{context}");
    for (expected, actual) in case.marks.iter().zip(&typed.marks) {
        assert_eq!(expected.name, actual.name, "{context}");
        assert!(actual.wanted_range.is_some(), "{context}: {actual:?}");
        assert_eq!(actual.nodes.len(), 1, "{context}: {actual:?}");
        assert_eq!(actual.nodes[0].1, expected.kind, "{context}: {actual:?}");
        assert_eq!(
            actual.nodes[0].2,
            Some(expected.shape),
            "{context}: {actual:?}"
        );
    }
    typed
}

fn comparison<'a>(typed: &'a TypeObservation, marker: &str) -> &'a ComparisonObservation {
    let mark = typed
        .marks
        .iter()
        .find(|mark| mark.name == marker)
        .expect("comparison marker");
    typed
        .comparisons
        .iter()
        .find(|comparison| comparison.span == mark.nodes[0].0)
        .expect("observed comparison")
}

fn assert_descriptor(
    comparison: &ComparisonObservation,
    storage: Shape,
    narrowed: bool,
    positive_edge: bool,
) {
    let fact = comparison
        .descriptor
        .as_ref()
        .expect("real stable-symbol null comparison descriptor");
    assert!(
        fact.expression_matches && fact.symbol_matches_read && fact.storage_matches_symbol,
        "{comparison:?}"
    );
    assert_eq!(fact.storage_type, storage, "{comparison:?}");
    assert!(fact.null_matches_storage, "{comparison:?}");
    assert_eq!(fact.read_matches_storage_inner, narrowed, "{comparison:?}");
    assert_eq!(fact.non_null_when_true, positive_edge, "{comparison:?}");
    assert_eq!(comparison.non_null_use, narrowed, "{comparison:?}");
}

#[test]
fn narrowed_nullable_storage_contextualizes_nested_null_comparisons() {
    let marks = || {
        vec![
            mark("ne_right", "x != null", Shape::Boolean),
            mark("ne_left", "null != x", Shape::Boolean),
            mark("eq_right", "(x) == null", Shape::Boolean),
            mark("eq_left", "null == (x)", Shape::Boolean),
        ]
    };
    let cases = [
        Case {
            name: "int_outer_if",
            text: "fun test(x: Int?): Unit { if (x != null) { while (/*ne_right*/x != null) { break }; while (/*ne_left*/null != x) { break }; while (/*eq_right*/(x) == null) { break }; while (/*eq_left*/null == (x)) { break } } }",
            marks: marks(),
            rejected: None,
        },
        Case {
            name: "int_outer_while",
            text: "fun test(x: Int?): Unit { while (x != null) { while (/*ne_right*/x != null) { break }; while (/*ne_left*/null != x) { break }; while (/*eq_right*/(x) == null) { break }; while (/*eq_left*/null == (x)) { break }; break } }",
            marks: marks(),
            rejected: None,
        },
        Case {
            name: "holder_outer_if",
            text: "class Holder(val value: Int)\nfun test(x: Holder?): Unit { if (x != null) { while (/*ne_right*/x != null) { break }; while (/*ne_left*/null != x) { break }; while (/*eq_right*/(x) == null) { break }; while (/*eq_left*/null == (x)) { break } } }",
            marks: marks(),
            rejected: None,
        },
        Case {
            name: "holder_outer_while",
            text: "class Holder(val value: Int)\nfun test(x: Holder?): Unit { while (x != null) { while (/*ne_right*/x != null) { break }; while (/*ne_left*/null != x) { break }; while (/*eq_right*/(x) == null) { break }; while (/*eq_left*/null == (x)) { break }; break } }",
            marks: marks(),
            rejected: None,
        },
    ];
    let observations = collect(&cases);
    for (case, unit, observation) in observations {
        let typed = assert_observation(case, unit, observation);
        assert!(
            typed.raw.is_empty(),
            "{} unit={unit}: {typed:#?}",
            case.name
        );
        assert!(
            matches!(typed.complete, Ok(true)),
            "{} unit={unit}: {typed:#?}",
            case.name
        );
        let (inner, storage) = if case.name.starts_with("int_") {
            (Shape::Int, Shape::NullableInt)
        } else {
            (Shape::Nominal, Shape::NullableNominal)
        };
        assert_eq!(typed.comparisons.len(), 5, "{typed:#?}");
        for (marker, positive_edge) in [
            ("ne_right", true),
            ("ne_left", true),
            ("eq_right", false),
            ("eq_left", false),
        ] {
            let comparison = comparison(&typed, marker);
            assert_eq!(comparison.read_type, Some(inner), "{comparison:?}");
            assert_eq!(comparison.null_type, Some(storage), "{comparison:?}");
            assert_eq!(
                comparison.result_type,
                Some(Shape::Boolean),
                "{comparison:?}"
            );
            assert_descriptor(comparison, storage, true, positive_edge);
        }
    }
}

#[test]
fn nullable_storage_fallback_preserves_invalidation_and_invalid_neighbors() {
    let cases = [
        Case {
            name: "nonnullable_null",
            text: "fun test(x: Int): Unit { val seen = /*compare*/x == /*bad*/null }",
            marks: vec![
                mark("compare", "x == /*bad*/null", Shape::Boolean),
                mark("bad", "null", Shape::Error),
            ],
            rejected: None,
        },
        Case {
            name: "standalone_null",
            text: "fun test(): Unit { val seen = /*bad*/null }",
            marks: vec![mark("bad", "null", Shape::Error)],
            rejected: None,
        },
        Case {
            name: "both_null",
            text: "fun test(): Unit { val seen = /*bad*/null == /*also_bad*/null }",
            marks: vec![
                mark("bad", "null", Shape::Error),
                mark("also_bad", "null", Shape::Error),
            ],
            rejected: None,
        },
        Case {
            name: "assignment_retest",
            text: "fun test(initial: Int?): Unit { var x = initial; if (x != null) { val before = /*before*/x; x = null; if (/*compare*/(x) != null) { val accepted: Int = /*after*/x } } }",
            marks: vec![
                read("before", Shape::Int),
                mark("compare", "(x) != null", Shape::Boolean),
                read("after", Shape::Int),
            ],
            rejected: None,
        },
        Case {
            name: "inout_retest",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; if (x != null) { val before = /*before*/x; mutate(&x); if (/*compare*/null != (x)) { val accepted: Int = /*after*/x } } }",
            marks: vec![
                read("before", Shape::Int),
                mark("compare", "null != (x)", Shape::Boolean),
                read("after", Shape::Int),
            ],
            rejected: None,
        },
        Case {
            name: "captured_mutable",
            text: "fun test(initial: Int?): Unit { var x = initial; val callback = { x }; while (x != null) { val ignored = /*compare*/(x) != null; val seen = /*body*/x; break } }",
            marks: vec![
                mark("compare", "(x) != null", Shape::Boolean),
                read("body", Shape::NullableInt),
            ],
            rejected: None,
        },
        Case {
            name: "error_read",
            text: "fun test(): Unit { val x = /*error*/1 /*operator*/+ true; val ignored = /*compare*/x == /*bad*/null }",
            marks: vec![
                mark("error", "1 /*operator*/+ true", Shape::Error),
                mark("compare", "x == /*bad*/null", Shape::Boolean),
                mark("bad", "null", Shape::Error),
            ],
            rejected: None,
        },
        Case {
            name: "deferred_read",
            text: "enum class Result<T, E> { Ok(success: T), Err(error: E) }\nfun test(r: Result<Int, Int>): Result<Int, Int> { val x = /*deferred*/r?; val ignored = /*compare*/x == /*bad*/null; return r }",
            marks: vec![
                mark(
                    "deferred",
                    "r?",
                    Shape::Deferred(DeferredReason::ErrorPropagation),
                ),
                mark(
                    "compare",
                    "x == /*bad*/null",
                    Shape::Deferred(DeferredReason::ControlJoin),
                ),
                mark("bad", "null", Shape::Error),
            ],
            rejected: None,
        },
    ];
    let observations = collect(&cases);
    for (case, unit, observation) in observations {
        let typed = assert_observation(case, unit, observation);
        let expected_codes: &[&str] = match case.name {
            "nonnullable_null" | "standalone_null" | "deferred_read" => &["L0083"],
            "both_null" => &["L0083", "L0083"],
            "error_read" => &["L0085", "L0083"],
            _ => &[],
        };
        assert_eq!(
            typed
                .raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes,
            "{} unit={unit}: {typed:#?}",
            case.name
        );
        let mut expected_spans = typed
            .marks
            .iter()
            .filter(|mark| matches!(mark.name, "bad" | "also_bad"))
            .map(|mark| mark.nodes[0].0)
            .collect::<Vec<_>>();
        if case.name == "error_read" {
            let operator =
                case.text.find("/*operator*/").expect("operator marker") + "/*operator*/".len();
            let diagnostic = &typed.raw[0];
            assert_eq!(
                (
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end()
                ),
                (operator, operator + 1)
            );
            expected_spans.insert(0, diagnostic.primary_span());
        }
        assert_eq!(
            typed
                .raw
                .iter()
                .map(Diagnostic::primary_span)
                .collect::<Vec<_>>(),
            expected_spans,
            "{typed:#?}"
        );
        if expected_codes.is_empty() {
            assert!(
                matches!(typed.complete, Ok(true)),
                "{} unit={unit}: {typed:#?}",
                case.name
            );
        } else {
            let Err(FinalTypeAdmissionError::Diagnostics(complete)) = &typed.complete else {
                panic!("{typed:#?}");
            };
            if case.name == "deferred_read" {
                assert_eq!(complete.len(), typed.raw.len() + 1, "{typed:#?}");
                assert!(
                    typed.raw.iter().all(|raw| complete.contains(raw)),
                    "{typed:#?}"
                );
                let deferred = typed
                    .marks
                    .iter()
                    .find(|mark| mark.name == "deferred")
                    .expect("deferred marker")
                    .nodes[0]
                    .0;
                assert!(
                    complete
                        .iter()
                        .any(|diagnostic| diagnostic.code().to_string() == "L0165"
                            && diagnostic.primary_span() == deferred),
                    "{typed:#?}"
                );
            } else {
                assert_eq!(complete, &typed.raw, "{typed:#?}");
            }
        }
        match case.name {
            "assignment_retest" | "inout_retest" | "captured_mutable" => {
                let comparison = comparison(&typed, "compare");
                assert_eq!(
                    comparison.read_type,
                    Some(Shape::NullableInt),
                    "{comparison:?}"
                );
                assert_eq!(
                    comparison.null_type,
                    Some(Shape::NullableInt),
                    "{comparison:?}"
                );
                assert_eq!(
                    comparison.result_type,
                    Some(Shape::Boolean),
                    "{comparison:?}"
                );
                if case.name == "captured_mutable" {
                    assert!(
                        typed
                            .comparisons
                            .iter()
                            .all(|comparison| comparison.descriptor.is_none()
                                && !comparison.non_null_use),
                        "{typed:#?}"
                    );
                } else {
                    assert_descriptor(comparison, Shape::NullableInt, false, true);
                }
            }
            "nonnullable_null" | "error_read" | "deferred_read" => {
                let comparison = comparison(&typed, "compare");
                let read_type = match case.name {
                    "nonnullable_null" => Shape::Int,
                    "error_read" => Shape::Error,
                    _ => Shape::Deferred(DeferredReason::ErrorPropagation),
                };
                assert_eq!(comparison.read_type, Some(read_type), "{comparison:?}");
                assert_eq!(comparison.null_type, Some(Shape::Error), "{comparison:?}");
                assert!(
                    comparison.descriptor.is_none() && !comparison.non_null_use,
                    "{comparison:?}"
                );
            }
            _ => assert!(typed.comparisons.is_empty(), "{typed:#?}"),
        }
    }
}
