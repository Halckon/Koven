//! SPEC-0291: ordinary nullable Map sources, complete FE, ownership, SSA and LLVM.
//! Collectors gather their entire layer before asserting; input oracles alone do not prove SSA identity.
//! Resource identity contracts are explicit, and resource::runtime is separate CI-only
//! object/native/count/Abort coverage rather than part of the ordinary collectors.

use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

use super::model::{FunctionId, Program};
use sources::{CASES, Case};

pub(in crate::ssa) mod consume_identity;
mod read_cross_join {
    //! B3 first observation: the original nullable owner remains live after its If.
    //! This independent source does not change the frozen A/B/C source partitions.
    //! Full raw ownership/SSA observations precede independent owner/proof identity review.

    use super::*;

    pub(super) const CROSS_JOIN: Case = Case {
        id: "B3",
        label: "if_size_keep_nullable_owner",
        path: "b3_if_size_keep_nullable_owner.ko",
        single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) { val count = source.size }
      val kept = source
      return
    }
    "#,
        provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) { val count = source.size }
      val kept = source
      return
    }
    "#,
        consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
        counts: [0, 0, 1],
        non_null_when_true: Some(true),
        scalar: lang_frontend::type_checking::BuiltinType::Int,
        roots: &[("source", true), ("kept", true)],
    };

    #[test]
    fn nullable_map_if_size_keeps_owner_across_join() {
        collect_layer(std::slice::from_ref(&CROSS_JOIN), 2);
    }
}
mod read_identity;
mod resource;
pub(in crate::ssa) mod single;
mod sources {
    //! Exact approved eight sources; no synthetic pre-registration or source rewriting.

    use lang_frontend::type_checking::BuiltinType;

    pub(super) struct Case {
        pub id: &'static str,
        pub label: &'static str,
        pub path: &'static str,
        pub single: &'static str,
        pub provider: &'static str,
        pub consumer: &'static str,
        pub counts: [usize; 3],
        pub non_null_when_true: Option<bool>,
        pub scalar: BuiltinType,
        pub roots: &'static [(&'static str, bool)],
    }

    // A root's bool selects p/provider.ko (true) or q/consumer.ko (false) in unit mode.
    // Single mode resolves every listed root in its one actual source.
    pub(super) const CASES: &[Case] = &[
        Case {
            id: "A1",
            label: "null_decl",
            path: "a1_null_decl.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = null
      println("null done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = null
      println("null done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "A2",
            label: "present_decl",
            path: "a2_present_decl.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      println("present done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      println("present done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "A3",
            label: "remove_canonical",
            path: "a3_remove_canonical.ko",
            single: r#"fun main(): Unit {
      val directNull: MutableMap<Int, Int>? = null
      val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
      outer.put(1, mutableMapOf<Int, Int>())
      val removed: MutableMap<Int, Int>? = outer.remove(1)
      println("canonical done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val directNull: MutableMap<Int, Int>? = null
      val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
      outer.put(1, mutableMapOf<Int, Int>())
      val removed: MutableMap<Int, Int>? = outer.remove(1)
      println("canonical done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 1, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[
                ("directNull", true),
                ("directPresent", true),
                ("removed", true),
            ],
        },
        Case {
            id: "B1",
            label: "if_group_size",
            path: "b1_if_group_size.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if ((source) != null) { val count = ((source)).size }
      println("read done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if ((source) != null) { val count = ((source)).size }
      println("read done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 1],
            non_null_when_true: Some(true),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "B2",
            label: "if_else_var_size",
            path: "b2_if_else_var_size.ko",
            single: r#"fun main(): Unit {
      var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source == null) {} else { val count = source.size }
      println("else read done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source == null) {} else { val count = source.size }
      println("else read done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 1],
            non_null_when_true: Some(false),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "C1",
            label: "val_empty",
            path: "c1_val_empty.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) {
        val result = source.consume()
        if (result.size == 0) { println("consumed empty") }
      }
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) {
        val result = source.consume()
        if (result.size == 0) { println("consumed empty") }
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [1, 0, 1],
            non_null_when_true: Some(true),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "C2",
            label: "var_string_group_discard",
            path: "c2_var_string_group_discard.ko",
            single: r#"fun main(): Unit {
      var seed = mutableMapOf<String, String>()
      seed.put("key".clone(), "old".clone())
      seed.put("key".clone(), "value".clone())
      var source: MutableMap<String, String>? = seed
      if (source != null) { ((source)).consume() }
      println("consumed strings")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      var seed = mutableMapOf<String, String>()
      seed.put("key".clone(), "old".clone())
      seed.put("key".clone(), "value".clone())
      var source: MutableMap<String, String>? = seed
      if (source != null) { ((source)).consume() }
      println("consumed strings")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [1, 0, 0],
            non_null_when_true: Some(true),
            scalar: BuiltinType::String,
            roots: &[("source", true)],
        },
        Case {
            id: "C3",
            label: "nullable_input_both_edges",
            path: "c3_nullable_input_both_edges.ko",
            single: r#"fun run(own input: MutableMap<Int, Int>?): Unit {
      val source: MutableMap<Int, Int>? = input
      if (source == null) {} else {
        val result = source.consume()
        if (result.size == 0) { println("input consumed") }
      }
    }
    fun main(): Unit {
      val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      run(present)
      val absent: MutableMap<Int, Int>? = null
      run(absent)
    }
    "#,
            provider: r#"package p
    fun run(own input: MutableMap<Int, Int>?): Unit {
      val source: MutableMap<Int, Int>? = input
      if (source == null) {} else {
        val result = source.consume()
        if (result.size == 0) { println("input consumed") }
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit {
      val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      p.run(present)
      val absent: MutableMap<Int, Int>? = null
      p.run(absent)
    }
    "#,
            counts: [1, 0, 1],
            non_null_when_true: Some(false),
            scalar: BuiltinType::Int,
            roots: &[
                ("input", true),
                ("source", true),
                ("present", false),
                ("absent", false),
            ],
        },
    ];
}
mod unit;

/// Share the frozen R1 fixture without loading its module more than once.
pub(in crate::ssa) fn registration_r1_source()
-> (&'static str, &'static str, &'static str, &'static str) {
    resource::registration_r1_source()
}

/// Reuse the exact C1/C2/C3 sources in ordinary native tests without loading them twice.
pub(crate) fn ordinary_nullable_consume_sources() -> [(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
); 3] {
    [&CASES[5], &CASES[6], &CASES[7]].map(|case| {
        (
            case.id,
            case.path,
            case.single,
            case.provider,
            case.consumer,
        )
    })
}

/// The unchanged ordinary B/C inputs shared by CI-only backend guard checks.
#[cfg(test)]
pub(in crate::ssa) fn nullable_map_guard_sources() -> [(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
); 6] {
    [
        &CASES[3],
        &CASES[4],
        &read_cross_join::CROSS_JOIN,
        &CASES[5],
        &CASES[6],
        &CASES[7],
    ]
    .map(|case| {
        (
            case.id,
            case.path,
            case.single,
            case.provider,
            case.consumer,
        )
    })
}

#[derive(Debug)]
struct Failure {
    stage: String,
    detail: String,
}

type Outcome<T> = Result<T, Failure>;

#[derive(Debug, Default)]
struct Trace {
    stages: Vec<String>,
    input_failures: Vec<String>,
}

impl Trace {
    fn check(&mut self, label: &str, valid: bool, detail: impl std::fmt::Debug) {
        self.stages
            .push(format!("input {label}: valid={valid}; actual={detail:#?}"));
        if !valid {
            self.input_failures.push(label.to_owned());
        }
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    trace: Trace,
    outcome: Outcome<()>,
}

fn step<T, E: std::fmt::Debug>(stage: &str, result: Result<T, E>, trace: &mut Trace) -> Outcome<T> {
    match result {
        Ok(value) => {
            trace.stages.push(format!("{stage}: reached"));
            Ok(value)
        }
        Err(error) => {
            let detail = format!("{error:#?}");
            trace.stages.push(format!("{stage}: failed; {detail}"));
            Err(Failure {
                stage: stage.to_owned(),
                detail,
            })
        }
    }
}

fn gate(stage: &str, valid: bool, detail: impl std::fmt::Debug) -> Outcome<()> {
    if valid {
        Ok(())
    } else {
        Err(Failure {
            stage: stage.to_owned(),
            detail: format!("{detail:#?}"),
        })
    }
}

fn origin(sources: &SourceMap, span: Span) -> String {
    format!(
        "span={span:?}; source={:?}; text={:?}",
        sources.source_name(span.source_id()),
        sources.slice(span),
    )
}

fn diagnostic_text(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{diagnostic:#?}; {}",
                origin(sources, diagnostic.primary_span())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn diagnostics(
    stage: &str,
    sources: &SourceMap,
    values: &[Diagnostic],
    trace: &mut Trace,
) -> Outcome<()> {
    let detail = diagnostic_text(sources, values);
    trace
        .stages
        .push(format!("{stage}: count={}; {detail}", values.len()));
    gate(stage, values.is_empty(), detail)
}

fn admission_error(sources: &SourceMap, error: FinalTypeAdmissionError) -> String {
    match &error {
        FinalTypeAdmissionError::Diagnostics(values) => diagnostic_text(sources, values),
        FinalTypeAdmissionError::Internal(
            FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. },
        ) => format!("{error:#?}; {}", origin(sources, *span)),
        _ => format!("{error:#?}"),
    }
}

fn parsed(
    sources: &mut SourceMap,
    name: &str,
    text: &str,
    trace: &mut Trace,
) -> Outcome<ParsedFile> {
    let source = step("source", sources.add_source(name, text), trace)?;
    let lexed = step("lex", lex(sources, source), trace)?;
    let lexical = diagnostics("lex diagnostics", sources, lexed.diagnostics(), trace);
    let parsed = step("parse", parse_file(sources, &lexed), trace)?;
    let syntax = diagnostics("parse diagnostics", sources, parsed.diagnostics(), trace);
    lexical?;
    syntax?;
    trace
        .stages
        .push(format!("parsed source={source:?}; logical path={name}"));
    Ok(parsed)
}

fn ungroup(parsed: &ParsedFile, mut expression: ExpressionId) -> Option<ExpressionId> {
    loop {
        match parsed.ast().expressions().get(expression).ok()?.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name => return Some(expression),
            _ => return None,
        }
    }
}

fn lowering_error(sources: &SourceMap, error: super::LoweringError) -> String {
    let actual_origin = error.span.map(|span| origin(sources, span));
    format!("{error:#?}; actual source origin={actual_origin:?}")
}

fn verify_and_render(program: &Program, entry: FunctionId, trace: &mut Trace) -> Outcome<()> {
    // Record the real program even if its verifier rejects it; never mutate it.
    trace.stages.push(format!(
        "actual selected SSA entry={entry:?}; actual modules={:#?}",
        program.modules,
    ));
    step("verify SSA", super::verify::verify_program(program), trace)?;
    trace
        .stages
        .push(format!("verified SSA:\n{}", super::render_program(program)));
    let llvm = step(
        "in-memory LLVM with selected entry",
        crate::llvm::render_verified_program_with_entry(program, entry),
        trace,
    )?;
    trace
        .stages
        .push(format!("actual LLVM bytes={}; LLVM:\n{llvm}", llvm.len()));
    Ok(())
}

fn collect_layer(cases: &[Case], expected: usize) {
    let mut observations = Vec::with_capacity(expected);
    for case in cases {
        for unit in [false, true] {
            // A1, A2 and every other entry receive independent sources and fresh environments.
            let mut trace = Trace::default();
            let outcome = if unit {
                unit::observe(case, &mut trace)
            } else {
                single::observe(case, &mut trace)
            };
            observations.push(Observation {
                label: format!(
                    "{}/{}:{}",
                    case.id,
                    if unit { "unit" } else { "single" },
                    case.label
                ),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        diagnostic!(
            "NULLABLE MAP {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        diagnostic!("input failures={:?}", observation.trace.input_failures);
        match &observation.outcome {
            Ok(()) => diagnostic!("pipeline outcome=verified LLVM"),
            Err(failure) => {
                diagnostic!("failed stage={}; detail={}", failure.stage, failure.detail)
            }
        }
    }
    // These assertions happen only after every ordinary observation in this layer returned.
    assert_eq!(observations.len(), expected);
    assert!(
        observations
            .iter()
            .all(|value| value.outcome.is_ok() && value.trace.input_failures.is_empty()),
        "all {expected} ordinary observations must pass input oracles and reach verified LLVM; see observations above"
    );
}

#[test]
fn nullable_map_storage_and_remove_canonical_sources_reach_verified_llvm() {
    collect_layer(&CASES[..3], 6);
}

#[test]
fn nullable_map_if_read_views_reach_verified_llvm() {
    collect_layer(&CASES[3..5], 4);
}

#[test]
fn nullable_map_if_owned_consumes_reach_verified_llvm() {
    collect_layer(&CASES[5..], 6);
}

pub(in crate::ssa) mod lower_frontend_rc_unit_comparison_tests {
//! Same-source Unit coverage for the two retired nonnullable Rc legacy tests.
//! Preserves their explicit SSA/LLVM oracles, not whole-file or native equivalence.
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind},
    type_checking::UnitConstructionTarget,
};

use super::super::{
    model::{EntityId, EntityType, Function, LoanKind, Module, Operation, Origin, SsaTypeKind},
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

// Bind the rendered owner to the admitted source construction, not a legacy TypeId.
fn assert_rc_identity(
    unit: &UnitSource,
    module: &Module,
    function: &Function,
    source: &str,
) -> String {
    let constructions = unit
        .types()
        .constructions()
        .iter()
        .filter(|construction| construction.target() == UnitConstructionTarget::IntrinsicRc)
        .collect::<Vec<_>>();
    let [construction] = constructions.as_slice() else {
        panic!("one source Rc construction")
    };
    let [document] = unit.names().names().index().source_units() else {
        panic!("one source")
    };
    assert_eq!(construction.expression().source_unit(), document.id());
    let expression = unit
        .parsed()
        .ast()
        .expressions()
        .get(construction.expression().expression())
        .expect("original Rc expression");
    assert_eq!(unit.sources().slice(expression.span()), Ok(source));
    let allocations = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { owner, .. } => Some((owner, &instruction.origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(owner, origin)] = allocations.as_slice() else {
        panic!("one root Rc allocation")
    };
    assert_eq!(**origin, Origin::Source(expression.span()));
    let SsaTypeKind::SharedOwner {
        name,
        payload: Some(_),
    } = &module.types[owner.index()]
    else {
        panic!("defined Rc owner")
    };
    let expected = format!("Rc#u{}", construction.result_type().index());
    assert_eq!(name, &expected);
    expected
}

fn ssa_body<'a>(ssa: &'a str, function: &Function) -> &'a str {
    let marker = format!("  func {:?}(", function.name);
    let (_, body) = ssa
        .split_once(&marker)
        .expect("source-selected SSA function");
    body.split_once("\n  }").expect("SSA function end").0
}

fn llvm_body<'a>(llvm: &'a str, function: &Function) -> &'a str {
    let marker = format!("@f{}.{}(", function.id.index(), function.name);
    let definition = llvm
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(&marker))
        .expect("source-selected LLVM definition");
    let (_, body) = llvm.split_once(definition).expect("LLVM definition");
    body.split_once("\n}").expect("LLVM function end").0
}

#[test]
fn intrinsic_rc_construction_share_and_copyable_payload_read_unit_comparison() {
    let text = "fun shared(): Int {\n\
             val first = Rc(40)\n\
             val second = first.share()\n\
             val copied = second.value\n\
             return copied + first.value\n\
         }";
    let unit = analyze(text);
    let declaration = unit.declaration("shared", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(unit.sources().slice(span), Ok(text));
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original shared Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    assert!(!function.blocks.is_empty());
    let rc_name = assert_rc_identity(&unit, module, function, "Rc(40)");
    let owners = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { owner, .. } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [owner] = owners.as_slice() else {
        panic!("one actual Rc allocation");
    };
    let SsaTypeKind::SharedOwner {
        payload: Some(payload),
        ..
    } = &module.types[owner.index()]
    else {
        panic!("actual allocation has SharedOwner storage");
    };
    assert_eq!(
        module.types[payload.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    verify_program(&program).expect("complete Unit SSA verification");
    let ssa = render_program(&program);
    assert_diagnostic!(
        ssa.contains(&format!("shared_owner {rc_name:?} payload ")),
        "{ssa}"
    );
    assert_eq_diagnostic!(ssa.matches("shared.allocate").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("shared.retain").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("shared.payload_place").count(), 2, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("drop ").count(), 2, "{ssa}");
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    let root_ssa = ssa_body(&ssa, function);
    for (operation, count) in [
        ("shared.allocate", 1),
        ("shared.retain", 1),
        ("shared.payload_place", 2),
        ("drop ", 2),
    ] {
        assert_eq!(root_ssa.matches(operation).count(), count, "{root_ssa}");
    }
    let root_llvm = llvm_body(&llvm, function);
    assert_eq!(
        root_llvm.matches("call ptr @malloc").count(),
        1,
        "{root_llvm}"
    );
    assert!(root_llvm.contains(".overflow = icmp eq"), "{root_llvm}");
    assert_eq_diagnostic!(llvm.matches("call ptr @malloc").count(), 1, "{llvm}");
    assert_eq_diagnostic!(llvm.matches("call void @free").count(), 1, "{llvm}");
    assert_diagnostic!(llvm.contains(".overflow = icmp eq"), "{llvm}");
    assert_eq_diagnostic!(llvm.matches("strong.next = sub").count(), 1, "{llvm}");
    assert_diagnostic!(!llvm.contains("atomicrmw"), "{llvm}");
    assert_diagnostic!(!llvm.contains("cmpxchg"), "{llvm}");
}

#[test]
fn move_only_rc_payload_borrow_call_unit_comparison() {
    let text = "class Resource {}\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun exercise(): Unit {\n\
             val owner = Rc(Resource())\n\
             val inspected = inspect(owner.value)\n\
             val retained = owner.share()\n\
         }";
    let unit = analyze(text);
    let declaration = unit.declaration("exercise", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    let original_root = &text[text
        .find("fun exercise")
        .expect("original exercise declaration")..];
    assert_eq!(unit.sources().slice(span), Ok(original_root));
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original exercise Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    let rc_name = assert_rc_identity(&unit, module, function, "Rc(Resource())");
    let inspect = unit.declaration("inspect", SymbolKind::Function);
    let inspect_span = unit.declaration_span(inspect);
    assert_eq!(
        unit.sources().slice(inspect_span),
        Ok("fun inspect(resource: Resource): Unit {}")
    );
    let callees = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(inspect_span))
        .collect::<Vec<_>>();
    let [callee] = callees.as_slice() else {
        panic!("one exact inspect declaration");
    };
    let [parameter] = callee.blocks[0].parameters.as_slice() else {
        panic!("one Borrow parameter");
    };
    assert!(matches!(
        callee.entity(*parameter).map(|entity| entity.ty),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee: target,
                receiver,
                arguments,
            } if *target == callee.id => Some((receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receiver, arguments)] = calls.as_slice() else {
        panic!("one call to exact inspect declaration");
    };
    assert!(receiver.is_none());
    let [EntityId::Loan(loan)] = arguments.as_slice() else {
        panic!("exact caller loan")
    };
    let payloads = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::SharedPayloadPlace { .. }))
        .collect::<Vec<_>>();
    let [payload] = payloads.as_slice() else {
        panic!("one payload projection")
    };
    let [EntityId::Place(place)] = payload.results.as_slice() else {
        panic!("payload place result")
    };
    // Check execution order in the containing block, not instruction arena order.
    let block = &function.blocks[payload.block.index()];
    let operations = block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let projection = operations
        .iter()
        .position(|instruction| instruction.id == payload.id)
        .unwrap();
    let borrow = operations
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared,
                }
                && instruction.results == [EntityId::Loan(*loan)]
        })
        .expect("payload place produces the exact call loan");
    let call = operations
        .iter()
        .position(|instruction| {
            matches!(&instruction.operation,
                Operation::DirectCall { callee: target, receiver: None, arguments }
                    if *target == callee.id && arguments == &[EntityId::Loan(*loan)]
            )
        })
        .expect("same-block call consumes exact loan");
    let end = operations
        .iter()
        .position(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
        .expect("same-block end of exact loan");
    assert!(projection < borrow && borrow < call && call < end);
    let EntityId::Loan(parameter) = parameter else {
        panic!("callee Borrow parameter")
    };
    verify_program(&program).expect("complete Unit SSA verification");
    let ssa = render_program(&program);
    assert_diagnostic!(
        ssa.contains(&format!("shared_owner {rc_name:?} payload ")),
        "{ssa}"
    );
    let root_ssa = ssa_body(&ssa, function);
    assert!(
        root_ssa.contains(&format!("call @f{}(%l{})", callee.id.index(), loan.index())),
        "{root_ssa}"
    );
    for operation in [
        "shared.payload_place",
        "borrow.shared",
        "end_borrow",
        "shared.retain",
    ] {
        assert_eq_diagnostic!(ssa.matches(operation).count(), 1, "{operation}: {ssa}");
        assert_eq!(root_ssa.matches(operation).count(), 1, "{root_ssa}");
    }
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    let symbol = format!("f{}.{}", callee.id.index(), callee.name);
    let root_llvm = llvm_body(&llvm, function);
    let pointer_definition = format!("%p{} = getelementptr", place.index());
    let pointer_position = root_llvm
        .find(&pointer_definition)
        .expect("payload GEP defines caller pointer");
    let call_text = format!("call void @{symbol}(ptr %p{})", place.index());
    let call_position = root_llvm
        .find(&call_text)
        .expect("exact payload pointer passed to inspect");
    assert!(pointer_position < call_position);
    assert_diagnostic!(
        llvm.contains(&format!(
            "define internal void @{symbol}(ptr %l{})",
            parameter.index()
        )),
        "{llvm}"
    );
    assert_eq_diagnostic!(llvm.matches("call ptr @malloc").count(), 2, "{llvm}");
    assert_eq_diagnostic!(llvm.matches("call void @free").count(), 2, "{llvm}");
}
}
