//! SPEC-0238: execute current Guide examples through both frontend entry points.
//! This is a diagnostic/ownership regression gate, not SSA or native conformance.

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_constant_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        TypeKind, UnitExpressionId, UnitTypeKind, UnitTypeRefId, check_compilation_unit_types,
        check_types, standard_environments,
    },
};

const GUIDE: &str = include_str!("../../../docs/guide/15-conformance-and-staging.md");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Parse,
    Names,
    Types,
    Ownership,
}

#[derive(Debug, PartialEq, Eq)]
struct Failure {
    stage: Stage,
    // Exact ordered codes and source ranges; a disappearing known gap also fails.
    diagnostics: Vec<(String, usize, usize, String)>,
}

fn failure(stage: Stage, sources: &SourceMap, diagnostics: &[Diagnostic]) -> Option<Failure> {
    (!diagnostics.is_empty()).then(|| Failure {
        stage,
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| {
                let span = diagnostic.primary_span();
                (
                    diagnostic.code().to_string(),
                    span.start(),
                    span.end(),
                    sources
                        .slice(span)
                        .expect("diagnostic source range")
                        .to_owned(),
                )
            })
            .collect(),
    })
}

fn expected(text: &str, stage: Stage, diagnostics: &[(&str, &str)]) -> Option<Failure> {
    (!diagnostics.is_empty()).then(|| Failure {
        stage,
        diagnostics: diagnostics
            .iter()
            .map(|(code, source)| {
                let start = text.find(source).expect("expected span exists in Guide");
                (
                    code.to_string(),
                    start,
                    start + source.len(),
                    source.to_string(),
                )
            })
            .collect(),
    })
}

fn examples(guide: &str) -> Vec<&str> {
    let sections = guide.split("\n### Litmus ").skip(1).collect::<Vec<_>>();
    assert_eq!(
        sections.len(),
        12,
        "update the explicit coverage ledger for new Litmus cases"
    );
    sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            assert!(
                section.starts_with(&format!("{}: ", index + 1)),
                "ordered unique Litmus IDs"
            );
            assert_eq!(
                section.matches("```kotlin\n").count(),
                1,
                "exactly one source per Litmus"
            );
            let (_, source) = section.split_once("```kotlin\n").expect("Kotlin fence");
            source.split_once("\n```").expect("closed Kotlin fence").0
        })
        .collect()
}

fn check(text: &str, unit: bool, expected_facts: &[(&str, &str)]) -> Option<Failure> {
    let package = text.lines().find_map(|line| line.strip_prefix("package "));
    let path = package.map_or_else(
        || "litmus.ko".to_owned(),
        |package| format!("{}/litmus.ko", package.trim().replace('.', "/")),
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source(&path, text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    if let Some(failure) = failure(Stage::Parse, &sources, parsed.diagnostics()) {
        return Some(failure);
    }
    let (names, environment) = standard_environments();
    if unit {
        let inputs = [SourceUnitInput::new("root", &path, source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &names).expect("unit names");
        if let Some(failure) = failure(Stage::Names, &sources, names.diagnostics()) {
            return Some(failure);
        }
        let names = names.validate().expect("validated unit names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .expect("unit types");
        if let Some(failure) = failure(Stage::Types, &sources, typed.diagnostics()) {
            return Some(failure);
        }
        let source_unit = names.names().index().source_units()[0].id();
        let expressions = parsed.ast().expressions().iter().map(|(id, node)| {
            (
                node.span(),
                typed.expression_type(UnitExpressionId::new(source_unit, id)),
            )
        });
        let type_refs = parsed.ast().type_refs().iter().map(|(id, node)| {
            (
                node.span(),
                typed.type_ref_type(UnitTypeRefId::new(source_unit, id)),
            )
        });
        let incomplete = expressions
            .chain(type_refs)
            .filter_map(|(span, ty)| {
                let kind = ty.and_then(|ty| typed.types().get(ty));
                matches!(
                    kind,
                    None | Some(UnitTypeKind::Deferred(_) | UnitTypeKind::Error)
                )
                .then(|| (sources.slice(span).expect("fact span"), format!("{kind:?}")))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            incomplete
                .iter()
                .map(|(source, kind)| (*source, kind.as_str()))
                .collect::<Vec<_>>(),
            expected_facts,
            "unit typed snapshot changed"
        );
        let typed = typed
            .validate_constants()
            .expect("complete unit constant facts");
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .expect("unit ownership");
        assert!(
            owned.ownership().deferred().is_empty(),
            "{:?}",
            owned.ownership().deferred()
        );
        let result = failure(Stage::Ownership, &sources, owned.ownership().diagnostics());
        if result.is_none() {
            owned
                .validate()
                .expect("complete unit ownership constant capability");
        }
        result
    } else {
        let names = resolve_names(&sources, &parsed, &names).expect("single names");
        if let Some(failure) = failure(Stage::Names, &sources, names.diagnostics()) {
            return Some(failure);
        }
        let typed = check_types(&sources, &parsed, &names, &environment).expect("single types");
        if let Some(failure) = failure(Stage::Types, &sources, typed.diagnostics()) {
            return Some(failure);
        }
        let expressions = parsed
            .ast()
            .expressions()
            .iter()
            .map(|(id, node)| (node.span(), typed.expression_type(id)));
        let type_refs = parsed
            .ast()
            .type_refs()
            .iter()
            .map(|(id, node)| (node.span(), typed.type_ref_type(id)));
        let incomplete = expressions
            .chain(type_refs)
            .filter_map(|(span, ty)| {
                let kind = ty.and_then(|ty| typed.types().get(ty));
                matches!(kind, None | Some(TypeKind::Deferred(_) | TypeKind::Error))
                    .then(|| (sources.slice(span).expect("fact span"), format!("{kind:?}")))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            incomplete
                .iter()
                .map(|(source, kind)| (*source, kind.as_str()))
                .collect::<Vec<_>>(),
            expected_facts,
            "single typed snapshot changed"
        );
        assert!(
            typed.constants().is_some(),
            "complete single-file constant facts"
        );
        let owned = check_ownership(&sources, &parsed, &names, &typed).expect("single ownership");
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let result = failure(Stage::Ownership, &sources, owned.diagnostics());
        if result.is_none() {
            assert!(
                owned.constant_materializations().is_some(),
                "complete single ownership constant capability"
            );
        }
        result
    }
}

// Exact per-entry-point typed snapshots: no blanket allowance for Deferred, Error, or missing facts.
// A source type target is not a runtime expression value. Other deferred entries remain stage gaps.
fn assert_both_with_facts(
    text: &str,
    stage: Stage,
    diagnostics: &[(&str, &str)],
    facts: [&[(&str, &str)]; 2],
) {
    for (unit, facts) in [(false, facts[0]), (true, facts[1])] {
        assert_eq!(
            check(text, unit, facts),
            expected(text, stage, diagnostics),
            "unit={unit}"
        );
    }
}

fn assert_both(text: &str, stage: Stage, diagnostics: &[(&str, &str)]) {
    assert_both_with_facts(text, stage, diagnostics, [&[], &[]]);
}

macro_rules! litmus {
    ($name:ident, $id:literal, [$($diagnostic:expr),*]) => {
        litmus!($name, $id, [$($diagnostic),*], [], []);
    };
    ($name:ident, $id:literal, [$($diagnostic:expr),*], [$($single:expr),*], [$($unit:expr),*]) => {
        #[test]
        fn $name() {
            let source = examples(GUIDE)[$id - 1];
            assert_both_with_facts(source, Stage::Types, &[$($diagnostic),*], [&[$($single),*], &[$($unit),*]]);
        }
    };
}

litmus!(
    litmus_01_basic_frontend,
    1,
    [],
    [("result = result + 10", "Some(Deferred(Assignment))")],
    []
);
litmus!(litmus_02_newlines_frontend, 2, []);
litmus!(
    litmus_03_control_frontend,
    3,
    [],
    [
        ("count = count + 1", "Some(Deferred(Assignment))"),
        ("count = count - 1", "Some(Deferred(Assignment))")
    ],
    []
);
// TODO: implement return-when operand parsing; do not parenthesize the normative example to hide it.
litmus!(litmus_04_return_when_known_gap, 4, [("L0087", "return")]);
litmus!(
    litmus_05_inout_member_frontend,
    5,
    [],
    [("count = count + 1", "Some(Deferred(Assignment))")],
    []
);
litmus!(litmus_06_generics_frontend, 6, []);
litmus!(
    litmus_07_ownership_frontend,
    7,
    [],
    [("Resource", "Some(Error)")],
    [("Resource", "None")]
);
litmus!(
    litmus_08_borrowing_frontend,
    8,
    [],
    [
        ("n.value = n.value + delta", "Some(Deferred(Assignment))"),
        ("Node", "Some(Error)")
    ],
    [("Node", "None")]
);
litmus!(litmus_09_nullable_frontend, 9, []);
litmus!(litmus_10_closure_frontend, 10, []);
// TODO: close the unit for-body typed facts separately from diagnostic/ownership acceptance.
litmus!(
    litmus_11_iteration_frontend,
    11,
    [],
    [
        ("listOf", "Some(Deferred(Call))"),
        ("sum = sum + i", "Some(Deferred(Assignment))")
    ],
    [
        ("listOf", "Some(Deferred(Call))"),
        ("i", "Some(Deferred(LoopSource))"),
        ("sum + i", "Some(Deferred(ControlJoin))"),
        ("sum = sum + i", "Some(Deferred(Assignment))")
    ]
);
// TODO: implement Guide05 const named-bitwise evaluation before moving this to the positive set.
litmus!(
    litmus_12_const_bitwise_known_gap,
    12,
    [("L0156", "1 shl 0"), ("L0156", "1 shl 1")]
);

fn replace_once(source: &str, old: &str, new: &str) -> String {
    assert_eq!(
        source.matches(old).count(),
        1,
        "derived sample must replace exactly one {old:?}"
    );
    source.replacen(old, new, 1)
}

#[test]
fn parenthesized_return_when_is_a_distinct_workaround() {
    let source = replace_once(
        examples(GUIDE)[3],
        "return when (s) {",
        "return (when (s) {",
    );
    let source = replace_once(
        &source,
        "        Shape.Point -> 0\n    }",
        "        Shape.Point -> 0\n    })",
    );
    assert_both_with_facts(
        &source,
        Stage::Types,
        &[],
        [&[("Shape", "Some(Error)")], &[("Shape", "None")]],
    );
}

#[test]
fn runtime_bitwise_does_not_claim_const_evaluation() {
    let source = replace_once(
        examples(GUIDE)[11],
        "const val READ: Int = 1 shl 0",
        "const val READ: Int = 1",
    );
    let source = replace_once(
        &source,
        "const val WRITE: Int = 1 shl 1",
        "const val WRITE: Int = 2",
    );
    let source = replace_once(
        &source,
        "val mask = BitMasks.READ or BitMasks.WRITE",
        "val shifted = BitMasks.READ shl 32\n    val mask = shifted or BitMasks.WRITE",
    );
    assert_both_with_facts(
        &source,
        Stage::Types,
        &[],
        [
            &[("BitMasks", "Some(Error)"), ("BitMasks", "Some(Error)")],
            &[("BitMasks", "None"), ("BitMasks", "None")],
        ],
    );
}

#[test]
fn counter_requires_inout_receiver() {
    let source = replace_once(examples(GUIDE)[4], "inout fun increment", "fun increment");
    for (unit, code) in [(false, "L0135"), (true, "L0134")] {
        // The primary span is the assignment's field use, not the constructor declaration.
        let start = source.find("count = count").expect("field assignment");
        let expected = Some(Failure {
            stage: Stage::Ownership,
            diagnostics: vec![(
                code.to_owned(),
                start,
                start + "count".len(),
                "count".to_owned(),
            )],
        });
        let facts = if unit {
            &[][..]
        } else {
            &[("count = count + 1", "Some(Deferred(Assignment))")][..]
        };
        assert_eq!(check(&source, unit, facts), expected, "unit={unit}");
    }
}

#[test]
fn const_bitwise_gap_covers_all_six_normative_operators() {
    for operator in ["shl", "shr", "ushr", "and", "or", "xor"] {
        let expression = format!("3 {operator} 1");
        let source = format!("const val MASK: Int = {expression}");
        assert_both(&source, Stage::Types, &[("L0156", &expression)]);
    }
}

#[test]
fn ownership_signature_examples_follow_canonical_parameter_mode_grammar() {
    for (guide, heading) in [
        (
            include_str!("../../../docs/guide/10-ownership-borrowing-drop.md"),
            "### 原地置换原子原语：replace 与 swap",
        ),
        (
            include_str!("../../../docs/guide/13-program-runtime-standard-library.md"),
            "### 标准原子所有权原语",
        ),
    ] {
        let (_, section) = guide.split_once(heading).expect("signature heading");
        let (_, source) = section.split_once("```kotlin\n").expect("signature source");
        let (source, _) = source.split_once("\n```").expect("closed signature source");
        assert_eq!(
            source,
            "fun <T> replace(inout place: T, own new: T): T\nfun <T> swap(inout a: T, inout b: T): Unit"
        );
        // These are abstract API signatures: syntax validity does not claim primitive execution.
        let mut sources = SourceMap::new();
        let id = sources.add_source("signatures.ko", source).expect("source");
        let parsed = parse_file(&sources, &lex(&sources, id).expect("lex")).expect("parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn recursive_enum_example_uses_payloads_without_field_markers() {
    let guide = include_str!("../../../docs/guide/11-copyability-layout-construction.md");
    let (_, source) = guide
        .split_once("`enum class Expr {")
        .expect("recursive enum example");
    let source = format!(
        "enum class Expr {{{}",
        source.split_once('`').expect("inline source").0
    );
    assert_both(&source, Stage::Types, &[]);
}

#[test]
fn raw_pointer_name_is_unresolved_not_proven_copyable() {
    let source = "value class Pointer(val pointer: RawPtr<Int>)";
    assert_both(source, Stage::Names, &[("L0080", "RawPtr")]);
}

#[test]
fn overlapping_borrow_passed_to_callee_conflicts_with_exclusive_receiver() {
    let source = "class Worker { inout fun update(other: Worker): Unit {} }\n\
                  fun invalid(own worker: Worker): Unit { worker.update(worker) }";
    // Select the actual argument's occurrence rather than the earlier parameter name.
    for unit in [false, true] {
        let start = source.rfind("worker)").expect("overlapping argument");
        assert_eq!(
            check(source, unit, &[]),
            Some(Failure {
                stage: Stage::Ownership,
                diagnostics: vec![(
                    "L0135".into(),
                    start,
                    start + "worker".len(),
                    "worker".into()
                )],
            }),
            "unit={unit}"
        );
    }
}

#[test]
fn two_phase_receiver_reservation_remains_an_implementation_gap() {
    let source = "class Worker {\n\
                      fun read(): Int = 1\n\
                      inout fun update(own count: Int): Unit {}\n\
                  }\n\
                  fun example(own worker: Worker): Unit { worker.update(worker.read()) }";
    // TODO: Reserved/Activate must allow this nested read after its call-scoped loan ends.
    // Keep the callee-overlap negative above when this known gap becomes a positive test.
    let start = source.rfind("worker.read").expect("nested read receiver");
    for unit in [false, true] {
        assert_eq!(
            check(source, unit, &[]),
            Some(Failure {
                stage: Stage::Ownership,
                diagnostics: vec![(
                    "L0135".into(),
                    start,
                    start + "worker".len(),
                    "worker".into()
                )],
            }),
            "unit={unit}"
        );
    }
}
