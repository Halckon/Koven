//! SPEC-0330 首批：普通源码的 while 保守 flow；只到 raw / complete 类型入口。
//! 每组先收集并打印全部源码、两个入口和 marker 事实，再执行 oracle。
//! 不修改 typed facts，不检查 ownership，不执行 Koven 源码或 native 产物。
use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, FinalTypeAdmissionError, TypeId, TypeKind, TypedFile,
        UnitExpressionId, UnitTypeId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Int,
    NullableInt,
    Unit,
    Error,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    Read,
    Assignment,
    Other,
}

struct Mark {
    name: &'static str,
    text: &'static str,
    kind: NodeKind,
    shape: Shape,
}

fn read(name: &'static str, shape: Shape) -> Mark {
    Mark {
        name,
        text: "x",
        kind: NodeKind::Read,
        shape,
    }
}

fn write() -> Mark {
    Mark {
        name: "write",
        text: "x = null",
        kind: NodeKind::Assignment,
        shape: Shape::Unit,
    }
}

struct Case {
    name: &'static str,
    text: &'static str,
    marks: Vec<Mark>,
    // 非法 Int 消费必须只在此 marker 的真实读取 Span 上报 L0084。
    rejected: Option<&'static str>,
}

#[derive(Debug)]
struct MarkObservation {
    name: &'static str,
    wanted_range: Option<(usize, usize)>,
    nodes: Vec<(Span, NodeKind, Option<Shape>)>,
}

#[derive(Debug)]
struct TypeObservation {
    marks: Vec<MarkObservation>,
    raw: Vec<Diagnostic>,
    raw_after_complete: Vec<Diagnostic>,
    primary_text: Vec<String>,
    // 成功时同时验证 complete 仍指向原始分析，不能靠另一轮分析通过。
    complete: Result<bool, FinalTypeAdmissionError>,
}

#[derive(Debug)]
struct Observation {
    lex: Vec<Diagnostic>,
    parse: Vec<Diagnostic>,
    names: Vec<Diagnostic>,
    typed: Result<TypeObservation, String>,
}

fn single_shape(typed: &TypedFile, id: TypeId) -> Shape {
    match typed.types().get(id) {
        Some(TypeKind::Builtin(BuiltinType::Int)) => Shape::Int,
        Some(TypeKind::Builtin(BuiltinType::Unit)) => Shape::Unit,
        Some(TypeKind::Nullable(inner))
            if typed.types().get(*inner) == Some(&TypeKind::Builtin(BuiltinType::Int)) =>
        {
            Shape::NullableInt
        }
        Some(TypeKind::Error) => Shape::Error,
        _ => Shape::Other,
    }
}

fn unit_shape(typed: &CompilationUnitTypes, id: UnitTypeId) -> Shape {
    match typed.types().get(id) {
        Some(UnitTypeKind::Builtin(BuiltinType::Int)) => Shape::Int,
        Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => Shape::Unit,
        Some(UnitTypeKind::Nullable(inner))
            if typed.types().get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::Int)) =>
        {
            Shape::NullableInt
        }
        Some(UnitTypeKind::Error) => Shape::Error,
        _ => Shape::Other,
    }
}

fn marker_range(text: &str, mark: &Mark) -> Option<(usize, usize)> {
    let marker = format!("/*{}*/", mark.name);
    let mut positions = text.match_indices(&marker);
    let (offset, _) = positions.next()?;
    if positions.next().is_some() {
        return None;
    }
    let start = offset + marker.len();
    text[start..]
        .starts_with(mark.text)
        .then_some((start, start + mark.text.len()))
}

fn observe_marks(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    shape: impl Fn(ExpressionId) -> Option<Shape>,
) -> Vec<MarkObservation> {
    case.marks
        .iter()
        .map(|mark| {
            let wanted_range = marker_range(case.text, mark);
            let nodes = file
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| {
                    wanted_range == Some((node.span().start(), node.span().end()))
                        && sources.slice(node.span()) == Ok(mark.text)
                })
                .map(|(id, node)| {
                    let kind = match node.payload() {
                        Expression::Name => NodeKind::Read,
                        Expression::Assignment { .. } => NodeKind::Assignment,
                        _ => NodeKind::Other,
                    };
                    (node.span(), kind, shape(id))
                })
                .collect();
            MarkObservation {
                name: mark.name,
                wanted_range,
                nodes,
            }
        })
        .collect()
}

fn primary_text(sources: &SourceMap, diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            sources
                .slice(diagnostic.primary_span())
                .map(str::to_owned)
                .unwrap_or_else(|error| format!("invalid diagnostic span: {error:?}"))
        })
        .collect()
}

// 此处不作成功断言。基础设施错误也作为 observation 返回，避免第一例失败遮住后续证据。
fn observe(case: &Case, unit_mode: bool) -> Result<Observation, String> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("main.ko", case.text)
        .map_err(|error| format!("source: {error:?}"))?;
    let tokens = lex(&sources, source).map_err(|error| format!("lex: {error:?}"))?;
    let file = parse_file(&sources, &tokens).map_err(|error| format!("parse: {error:?}"))?;
    let (environment, types) = standard_environments();
    let (name_diagnostics, typed) = if unit_mode {
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs)
            .map_err(|error| format!("unit index: {error:?}"))?;
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .map_err(|error| format!("unit names: {error:?}"))?;
        let diagnostics = names.diagnostics().to_vec();
        let typed = (|| {
            let names = names
                .validate()
                .map_err(|_| "unit names did not qualify".to_owned())?;
            let unit = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source)
                .ok_or_else(|| "source has no unit identity".to_owned())?
                .id();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .map_err(|error| format!("unit raw: {error:?}"))?;
            let raw = typed.diagnostics().to_vec();
            let marks = observe_marks(case, &sources, &file, |id| {
                typed
                    .expression_type(UnitExpressionId::new(unit, id))
                    .map(|ty| unit_shape(&typed, ty))
            });
            let complete = complete_compilation_unit_types(
                &sources, &inputs, &names, &types, &typed,
            )
            .map(|complete| {
                std::ptr::eq(complete.types(), &typed) && complete.is_same_analysis(&typed)
            });
            Ok(TypeObservation {
                marks,
                primary_text: primary_text(&sources, &raw),
                raw,
                raw_after_complete: typed.diagnostics().to_vec(),
                complete,
            })
        })();
        (diagnostics, typed)
    } else {
        let names = resolve_names(&sources, &file, &environment)
            .map_err(|error| format!("single names: {error:?}"))?;
        let diagnostics = names.diagnostics().to_vec();
        let typed = (|| {
            let typed = check_types(&sources, &file, &names, &types)
                .map_err(|error| format!("single raw: {error:?}"))?;
            let raw = typed.diagnostics().to_vec();
            let marks = observe_marks(case, &sources, &file, |id| {
                typed.expression_type(id).map(|ty| single_shape(&typed, ty))
            });
            let complete =
                complete_file_types(&sources, &file, &names, &types, &typed).map(|complete| {
                    std::ptr::eq(complete.types(), &typed) && complete.is_same_analysis(&typed)
                });
            Ok(TypeObservation {
                marks,
                primary_text: primary_text(&sources, &raw),
                raw,
                raw_after_complete: typed.diagnostics().to_vec(),
                complete,
            })
        })();
        (diagnostics, typed)
    };
    Ok(Observation {
        lex: tokens.diagnostics().to_vec(),
        parse: file.diagnostics().to_vec(),
        names: name_diagnostics,
        typed,
    })
}

fn assert_cases(cases: &[Case]) {
    let observations = cases
        .iter()
        .flat_map(|case| {
            [false, true]
                .into_iter()
                .map(move |unit_mode| (case, unit_mode, observe(case, unit_mode)))
        })
        .collect::<Vec<_>>();
    for (case, unit_mode, observation) in &observations {
        eprintln!(
            "SPEC-0330 case={} mode={} source={:?} observation={observation:#?}",
            case.name,
            if *unit_mode { "unit" } else { "single" },
            case.text,
        );
    }
    // 从这里开始才执行 oracle；两个入口及全部正反例的真实结果已经收齐。
    for (case, unit_mode, observation) in observations {
        let context = format!("case={} unit={unit_mode}", case.name);
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
            assert_eq!(actual.name, expected.name, "{context}");
            assert!(actual.wanted_range.is_some(), "{context}: {actual:?}");
            assert_eq!(actual.nodes.len(), 1, "{context}: {actual:?}");
            assert_eq!(actual.nodes[0].1, expected.kind, "{context}: {actual:?}");
            assert_eq!(
                actual.nodes[0].2,
                Some(expected.shape),
                "{context}: {actual:?}"
            );
        }
        if let Some(rejected) = case.rejected {
            assert_eq!(typed.raw.len(), 1, "{context}: {typed:?}");
            assert_eq!(typed.raw[0].code().to_string(), "L0084", "{context}");
            let rejected_read = typed
                .marks
                .iter()
                .find(|mark| mark.name == rejected)
                .expect("rejection names an observed marker");
            assert_eq!(
                typed.raw[0].primary_span(),
                rejected_read.nodes[0].0,
                "{context}"
            );
            assert_eq!(typed.primary_text, ["x"], "{context}");
            let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = &typed.complete else {
                panic!("{context}: {typed:?}");
            };
            assert_eq!(
                diagnostics, &typed.raw,
                "{context}: preserve exact raw diagnostics"
            );
        } else {
            assert!(typed.raw.is_empty(), "{context}: {typed:?}");
            assert!(matches!(typed.complete, Ok(true)), "{context}: {typed:?}");
        }
    }
}

#[test]
fn while_true_condition_narrows_stable_nullable_reads() {
    assert_cases(&[
        Case {
            name: "while_parameter_true",
            text: "fun test(x: Int?): Unit { while (x != null) { val seen = /*body*/x; val accepted: Int = /*consume*/x; break } }",
            marks: vec![read("body", Shape::Int), read("consume", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "independent_if_parameter_true",
            text: "fun test(x: Int?): Unit { if (x != null) { val seen = /*body*/x; val accepted: Int = /*consume*/x } }",
            marks: vec![read("body", Shape::Int), read("consume", Shape::Int)],
            rejected: None,
        },
    ]);
}

#[test]
fn while_assignment_uses_storage_type_and_kills_old_read_facts() {
    assert_cases(&[
        Case {
            name: "while_assignment_nullable_read",
            text: "fun test(initial: Int?): Unit { var x: Int? = initial; while (x != null) { val before = /*before*/x; /*write*/x = null; val after = /*after*/x; val accepted: Int? = /*consume*/x; break } }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("consume", Shape::NullableInt),
            ],
            rejected: None,
        },
        Case {
            name: "while_assignment_rejects_int_read",
            text: "fun test(initial: Int?): Unit { var x: Int? = initial; while (x != null) { val before = /*before*/x; /*write*/x = null; val after = /*after*/x; val rejected: Int = /*reject*/x; break } }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "independent_if_assignment_nullable_read",
            text: "fun test(initial: Int?): Unit { var x: Int? = initial; if (x != null) { val before = /*before*/x; /*write*/x = null; val after = /*after*/x; val accepted: Int? = /*consume*/x } }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("consume", Shape::NullableInt),
            ],
            rejected: None,
        },
        Case {
            name: "independent_if_assignment_rejects_int_read",
            text: "fun test(initial: Int?): Unit { var x: Int? = initial; if (x != null) { val before = /*before*/x; /*write*/x = null; val after = /*after*/x; val rejected: Int = /*reject*/x } }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
    ]);
}

#[test]
fn while_exit_adds_no_facts_and_preserves_immutable_outer_facts() {
    assert_cases(&[
        Case {
            name: "zero_iterations_do_not_export_body_facts",
            text: "fun test(x: Int?): Unit { while (false) { if (x != null) { val inside = /*body*/x } }; val after = /*after*/x; val rejected: Int = /*reject*/x }",
            marks: vec![
                read("body", Shape::Int),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "condition_false_adds_no_non_null_fact",
            text: "fun test(x: Int?): Unit { while (x == null) {}; val after = /*after*/x; val rejected: Int = /*reject*/x }",
            marks: vec![
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "break_adds_no_non_null_fact",
            text: "fun test(x: Int?): Unit { while (true) { if (x != null) { val inside = /*body*/x; break } }; val after = /*after*/x; val rejected: Int = /*reject*/x }",
            marks: vec![
                read("body", Shape::Int),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "immutable_outer_if_fact_survives_while",
            text: "fun test(x: Int?, flag: Boolean): Unit { if (x != null) { while (flag) { val inside = /*body*/x; break }; val after = /*after*/x; val accepted: Int = /*consume*/x } }",
            marks: vec![
                read("body", Shape::Int),
                read("after", Shape::Int),
                read("consume", Shape::Int),
            ],
            rejected: None,
        },
        Case {
            name: "loop_written_outer_fact_is_killed_even_for_zero_iterations",
            text: "fun test(initial: Int?, flag: Boolean): Unit { var x: Int? = initial; if (x != null) { val before = /*before*/x; while (flag) { val atHead = /*head*/x; /*write*/x = null; break }; val after = /*after*/x; val rejected: Int = /*reject*/x } }",
            marks: vec![
                read("before", Shape::Int),
                read("head", Shape::NullableInt),
                write(),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
    ]);
}

#[test]
fn while_condition_effect_order_preserves_only_current_facts() {
    assert_cases(&[
        Case {
            name: "condition_inout_after_test_kills_true_fact",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; while (x != null && mutate(&x)) { val seen = /*body*/x; val rejected: Int = /*reject*/x; break } }",
            marks: vec![
                read("body", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "condition_test_after_inout_reestablishes_true_fact",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; while (mutate(&x) && x != null) { val seen = /*body*/x; val accepted: Int = /*consume*/x; break } }",
            marks: vec![read("body", Shape::Int), read("consume", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "body_inout_kills_condition_fact",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; while (x != null) { mutate(&x); val seen = /*body*/x; val rejected: Int = /*reject*/x; break } }",
            marks: vec![
                read("body", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "ordinary_call_preserves_immutable_condition_fact",
            text: "fun inspect(): Boolean = true\nfun test(x: Int?): Unit { while (x != null && inspect()) { val seen = /*body*/x; val accepted: Int = /*consume*/x; break } }",
            marks: vec![read("body", Shape::Int), read("consume", Shape::Int)],
            rejected: None,
        },
    ]);
}

#[test]
fn while_nested_and_continue_paths_do_not_reuse_stale_facts() {
    assert_cases(&[
        Case {
            name: "continue_rechecks_condition_after_assignment",
            text: "fun test(initial: Int?): Unit { var x = initial; while (x != null) { val before = /*before*/x; /*write*/x = null; continue }; val after = /*after*/x; val rejected: Int = /*reject*/x }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "inner_write_invalidates_outer_body_fact",
            text: "fun test(initial: Int?, flag: Boolean): Unit { var x = initial; while (x != null) { val before = /*before*/x; while (flag) { /*write*/x = null; break }; val after = /*after*/x; val rejected: Int = /*reject*/x; break } }",
            marks: vec![
                read("before", Shape::Int),
                write(),
                read("after", Shape::NullableInt),
                read("reject", Shape::Error),
            ],
            rejected: Some("reject"),
        },
        Case {
            name: "nested_immutable_condition_facts_remain_stable",
            text: "fun test(x: Int?, flag: Boolean): Unit { while (x != null) { while (flag) { val inside = /*body*/x; break }; val after = /*after*/x; val accepted: Int = /*consume*/x; break } }",
            marks: vec![
                read("body", Shape::Int),
                read("after", Shape::Int),
                read("consume", Shape::Int),
            ],
            rejected: None,
        },
    ]);
}

#[test]
fn while_enum_case_true_fact_matches_existing_if_payload_access() {
    let payload = || Mark {
        name: "payload",
        text: "x.value",
        kind: NodeKind::Other,
        shape: Shape::Int,
    };
    assert_cases(&[
        Case {
            name: "enum_while_payload",
            text: "enum class Shape { A(value: Int), B(value: Int) }\nfun test(x: Shape): Unit { while (x is Shape.A) { val accepted: Int = /*payload*/x.value; break } }",
            marks: vec![payload()],
            rejected: None,
        },
        Case {
            name: "enum_if_payload_control",
            text: "enum class Shape { A(value: Int), B(value: Int) }\nfun test(x: Shape): Unit { if (x is Shape.A) { val accepted: Int = /*payload*/x.value } }",
            marks: vec![payload()],
            rejected: None,
        },
    ]);
}
