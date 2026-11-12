//! Resource observations plus the reviewed Resource compatibility expectations.
//! The collector alone remains insufficient for a compatibility verdict.
//! Resource source wholes include real assertion Abort edges and remain CI-only.

use super::{
    Failure, FunctionId, Outcome, ParsedFile, Program, SourceMap, Span, Trace, admission_error,
    diagnostic_text, diagnostics, gate, origin, parsed, step,
};
use crate::ssa::{LoweringError, LoweringErrorKind};
use crate::test_support::diagnostics::diagnostic;
use sources::{CASES, Case};

mod contract;
mod formed_locals;
mod memory {
    //! Original single-source bytes admitted as one same-object MemoryDocument Unit.

    use lang_frontend::{
        name_resolution::{
            DeclarationId, SourceUnitId, SourceUnitInput, SourceUnitInputKey, SourceUnitKey,
            SymbolKind, ValidatedCompilationUnitNames, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        ownership_checking::{
            CompilationUnitOwnership, NonNullAssertionTransferKind, OwnedCompilationUnitView,
            check_compilation_unit_ownership, owned_compilation_unit_view,
        },
        type_checking::{
            CompilationUnitTypes, UnitCallTarget, check_compilation_unit_types,
            complete_compilation_unit_types, standard_environments,
        },
    };

    use super::*;

    pub(super) fn observe(
        case: &Case,
        strong_contract: bool,
        trace: &mut Trace,
    ) -> Outcome<LowerObservation> {
        observe_source(
            case.path,
            case.single,
            |sources, file, names, typed, trace| {
                unit::inspect(case, sources, file, names, typed, trace)
            },
            |_, _, _, _, _, _| {},
            case.counts[5],
            |view, entry, program, lowered_entry, trace| {
                if strong_contract {
                    contract::inspect(view, entry, program, lowered_entry, trace);
                }
            },
            trace,
        )
    }

    pub(super) fn observe_source(
        path: &str,
        text: &str,
        inspect: impl FnOnce(
            &SourceMap,
            (SourceUnitId, &ParsedFile),
            &ValidatedCompilationUnitNames,
            &CompilationUnitTypes,
            &mut Trace,
        ),
        inspect_owned: impl FnOnce(
            &SourceMap,
            (SourceUnitId, &ParsedFile),
            &ValidatedCompilationUnitNames,
            &CompilationUnitTypes,
            &CompilationUnitOwnership,
            &mut Trace,
        ),
        assertion_count: usize,
        inspect_lower: impl FnOnce(
            &OwnedCompilationUnitView<'_, '_>,
            DeclarationId,
            &Program,
            FunctionId,
            &mut Trace,
        ),
        trace: &mut Trace,
    ) -> Outcome<LowerObservation> {
        let mut sources = SourceMap::new();
        let parsed = parsed(&mut sources, path, text, trace)?;
        let inputs = [SourceUnitInput::memory_document(
            parsed.source_id(),
            &parsed,
        )];
        gate(
            "same-object original MemoryDocument",
            std::ptr::eq(inputs[0].parsed(), &parsed)
                && inputs[0].source_id() == parsed.source_id()
                && matches!(inputs[0].key(), SourceUnitInputKey::MemoryDocument)
                && sources.source_name(parsed.source_id()).ok() == Some(path)
                && sources.source_text(parsed.source_id()).ok() == Some(text)
                && parsed.package().is_none(),
            (&inputs, path),
        )?;
        let (environment, types) = standard_environments();
        let index = step("index", index_compilation_unit(&sources, &inputs), trace)?;
        diagnostics("index diagnostics", &sources, index.diagnostics(), trace)?;
        let names = step(
            "names",
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
            trace,
        )?;
        diagnostics("names diagnostics", &sources, names.diagnostics(), trace)?;
        let names = step(
            "qualified names",
            names
                .validate()
                .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
            trace,
        )?;
        let index = names.names().index();
        let [source_unit] = index.source_units() else {
            return Err(Failure {
                stage: "one original MemoryDocument".to_owned(),
                detail: format!("{:#?}", index.source_units()),
            });
        };
        gate(
            "original default-package source unit",
            source_unit.source_id() == parsed.source_id()
                && source_unit.key() == &SourceUnitKey::MemoryDocument
                && index
                    .packages()
                    .get(source_unit.package().index())
                    .is_some_and(|package| {
                        package
                            .name()
                            .is_some_and(|name| name.segments().is_empty())
                    }),
            source_unit,
        )?;
        let entries: Vec<_> = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "main"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.source_unit() == source_unit.id()
                    && declaration.package() == source_unit.package()
                    && declaration.name_span().source_id() == parsed.source_id()
            })
            .collect();
        let [entry] = entries.as_slice() else {
            return Err(Failure {
                stage: "unique original main declaration".to_owned(),
                detail: format!("{entries:#?}"),
            });
        };
        let entry_span = parsed
            .ast()
            .items()
            .get(entry.root())
            .expect("real main item")
            .span();
        trace
            .stages
            .push(format!("selected real MemoryDocument main={entry:#?}"));
        let typed = step(
            "raw types",
            check_compilation_unit_types(&sources, &inputs, &names, &types),
            trace,
        )?;
        let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
        inspect(&sources, (source_unit.id(), &parsed), &names, &typed, trace);
        {
            let complete = step(
                "complete types",
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                    .map_err(|error| admission_error(&sources, error)),
                trace,
            )?;
            gate(
                "same complete analysis and original facts",
                complete.is_same_analysis(&typed)
                    && std::ptr::eq(complete.types(), &typed)
                    && typed.is_compatible_with(&sources, &inputs, &names, &types),
                "complete types borrow the original raw analysis",
            )?;
            diagnostics(
                "complete diagnostics",
                &sources,
                complete.types().diagnostics(),
                trace,
            )?;
        }
        raw?;
        let typed = step(
            "qualified types",
            typed
                .validate()
                .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
            trace,
        )?;
        let owned = step(
            "ownership",
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
            trace,
        )?;
        let clean = diagnostics(
            "ownership diagnostics",
            &sources,
            owned.diagnostics(),
            trace,
        );
        trace.stages.push(format!(
            "ownership deferred={:#?}; drops={:#?}; assertion plans={:#?}",
            owned.deferred(),
            owned.drops(),
            owned.non_null_assertions(),
        ));
        gate(
            "ownership completeness",
            owned.deferred().is_empty(),
            owned.deferred(),
        )?;
        trace.check(
            "real qualified assertion ownership plans",
            owned.non_null_assertions().len() == assertion_count
                && owned.non_null_assertions().iter().all(|plan| {
                    typed
                        .types()
                        .non_null_assertions()
                        .contains(plan.descriptor())
                        && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
                }),
            owned.non_null_assertions(),
        );
        gate(
            "ownership analysis identity",
            owned.is_compatible_with(&typed),
            owned.deferred(),
        )?;
        inspect_owned(
            &sources,
            (source_unit.id(), &parsed),
            &names,
            typed.types(),
            &owned,
            trace,
        );
        clean?;
        let owned = step(
            "qualified ownership",
            owned.validate().map_err(|value| {
                format!(
                    "{}; deferred={:#?}",
                    diagnostic_text(&sources, value.diagnostics()),
                    value.deferred()
                )
            }),
            trace,
        )?;
        let view = step(
            "owned view",
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned),
            trace,
        )?;
        gate(
            "owned view keeps all original references",
            std::ptr::eq(view.sources(), &sources)
                && std::ptr::eq(view.inputs(), inputs.as_slice())
                && std::ptr::eq(view.names(), &names)
                && std::ptr::eq(view.types(), typed.types())
                && std::ptr::eq(view.ownership(), owned.ownership()),
            "sources/inputs/names/types/ownership",
        )?;
        trace
            .stages
            .push("attempt lower SSA with original MemoryDocument main".to_owned());
        let result = crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, entry.id());
        if let Ok((program, lowered_entry)) = &result {
            trace.check(
                "selected SSA entry retains exact original Function Item origin",
                program
                    .module(lowered_entry.module())
                    .and_then(|module| module.function(*lowered_entry))
                    .is_some_and(|function| {
                        function.origin == crate::ssa::model::Origin::Source(entry_span)
                    }),
                (entry.id(), lowered_entry, entry_span),
            );
            inspect_dependencies(&view, &parsed, program, trace);
            inspect_lower(&view, entry.id(), program, *lowered_entry, trace);
        }
        Ok(LowerObservation {
            entry: selected_entry(&sources, entry.name(), entry.name_span()),
            outcome: finish_lower(&sources, result, trace),
        })
    }

    // The frozen P3/K6 inputs contain a real run(Boolean) dependency of main.
    // Keep the declaration and source-call identities, not merely a successful empty entry.
    fn inspect_dependencies(
        view: &OwnedCompilationUnitView<'_, '_>,
        parsed: &ParsedFile,
        program: &Program,
        trace: &mut Trace,
    ) {
        use crate::ssa::model::{Operation, Origin};

        for declaration in view
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() == SymbolKind::Function)
        {
            let span = parsed
                .ast()
                .items()
                .get(declaration.root())
                .expect("original function item")
                .span();
            let functions: Vec<_> = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .filter(|function| function.origin == Origin::Source(span))
                .collect();
            trace.check(
                "original function declaration has exactly one real emitted body",
                functions.len() == 1,
                (declaration, &functions),
            );
            let [target] = functions.as_slice() else {
                continue;
            };
            for call in view
                .types()
                .calls()
                .iter()
                .filter(|call| call.target() == UnitCallTarget::Declaration(declaration.id()))
            {
                let call_span = parsed
                    .ast()
                    .expressions()
                    .get(call.expression().expression())
                    .expect("original source call")
                    .span();
                let actual: Vec<_> = program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .flat_map(|function| &function.instructions)
                    .filter(|instruction| {
                        instruction.origin == Origin::Source(call_span)
                            && matches!(instruction.operation, Operation::DirectCall { callee, .. }
                                if callee == target.id())
                    })
                    .collect();
                trace.check(
                    "real source call reaches its exact source-qualified dependency",
                    actual.len() == 1,
                    (call, target.id(), actual),
                );
            }
        }
    }
}
mod runtime;
mod sources {
    //! Frozen four-source Resource baseline; source order is not mapper registration order.

    pub(super) struct Case {
        pub id: &'static str,
        pub path: &'static str,
        pub single: &'static str,
        pub provider: &'static str,
        pub consumer: &'static str,
        pub counts: [usize; 6],
        pub has_ordinary: bool,
    }

    pub(super) const CASES: &[Case] = &[
        Case {
            id: "R0",
            path: "r0_remove_only.ko",
            single: r#"class Resource { deinit() {} }
    fun main(): Unit {
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
    }
    "#,
            provider: r#"package p
    class Resource { deinit() {} }
    fun main(): Unit {
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [2, 2, 1, 1, 0, 1],
            has_ordinary: false,
        },
        Case {
            id: "R1",
            path: "r1_ordinary_source_before_remove.ko",
            single: r#"class Resource { deinit() {} }
    fun main(): Unit {
      val ordinary: MutableMap<Int, Resource>? = null
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
    }
    "#,
            provider: r#"package p
    class Resource { deinit() {} }
    fun main(): Unit {
      val ordinary: MutableMap<Int, Resource>? = null
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [2, 2, 1, 1, 0, 1],
            has_ordinary: true,
        },
        Case {
            id: "R2",
            path: "r2_ordinary_source_after_remove.ko",
            single: r#"class Resource { deinit() {} }
    fun main(): Unit {
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
      val ordinary: MutableMap<Int, Resource>? = null
    }
    "#,
            provider: r#"package p
    class Resource { deinit() {} }
    fun main(): Unit {
      var inner = mutableMapOf<Int, Resource>()
      inner.put(1, Resource())
      var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
      outer.put(1, inner)
      val got = outer.remove(1)!!
      val count = got.size
      val ordinary: MutableMap<Int, Resource>? = null
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [2, 2, 1, 1, 0, 1],
            has_ordinary: true,
        },
        Case {
            id: "R3",
            path: "r3_ordinary_null_only.ko",
            single: r#"class Resource { deinit() {} }
    fun main(): Unit {
      val ordinary: MutableMap<Int, Resource>? = null
    }
    "#,
            provider: r#"package p
    class Resource { deinit() {} }
    fun main(): Unit {
      val ordinary: MutableMap<Int, Resource>? = null
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 0, 0, 0, 0],
            has_ordinary: true,
        },
    ];
}
mod unit;
mod unreachable_storage;

/// Path, single-file text, provider text and consumer text from the existing R1 case.
pub(super) fn registration_r1_source() -> (&'static str, &'static str, &'static str, &'static str) {
    let case = CASES
        .iter()
        .find(|case| case.id == "R1")
        .expect("frozen R1");
    assert!(case.has_ordinary);
    assert_eq!(case.counts, [2, 2, 1, 1, 0, 1]);
    (case.path, case.single, case.provider, case.consumer)
}

#[derive(Debug)]
enum LowerOutcome {
    VerifiedLlvm,
    LowerRejected {
        error: LoweringError,
        origin: Option<String>,
        location: Option<ErrorLocation>,
    },
    VerifierRejected {
        detail: String,
    },
    LlvmRejected {
        detail: String,
    },
}

#[derive(Debug)]
struct ErrorLocation {
    path: Result<String, String>,
    text: Result<String, String>,
}

#[derive(Debug)]
struct SelectedEntry {
    name: String,
    path: Result<String, String>,
    span: Span,
}

fn selected_entry(sources: &SourceMap, name: &str, span: Span) -> SelectedEntry {
    SelectedEntry {
        name: name.to_owned(),
        path: sources
            .source_name(span.source_id())
            .map(str::to_owned)
            .map_err(|error| format!("{error:#?}")),
        span,
    }
}

#[derive(Debug)]
struct LowerObservation {
    // Captured from the same real declaration used as the explicit lowering entry.
    entry: SelectedEntry,
    outcome: LowerOutcome,
}

#[derive(Debug)]
struct Observation {
    label: String,
    trace: Trace,
    // Err is a prerequisite failure. Ok records one of four actual lower outcomes.
    outcome: Outcome<LowerObservation>,
}

fn finish_lower(
    sources: &SourceMap,
    result: Result<(Program, FunctionId), LoweringError>,
    trace: &mut Trace,
) -> LowerOutcome {
    let (program, entry) = match result {
        Ok(produced) => produced,
        Err(error) => {
            return LowerOutcome::LowerRejected {
                origin: error.span.map(|span| origin(sources, span)),
                location: error.span.map(|span| ErrorLocation {
                    path: sources
                        .source_name(span.source_id())
                        .map(str::to_owned)
                        .map_err(|error| format!("{error:#?}")),
                    text: sources
                        .slice(span)
                        .map(str::to_owned)
                        .map_err(|error| format!("{error:#?}")),
                }),
                error,
            };
        }
    };
    trace.stages.push("lower SSA: reached".to_owned());
    trace.stages.push(format!(
        "actual selected SSA entry={entry:?}; actual modules={:#?}",
        program.modules,
    ));
    if let Err(error) = crate::ssa::verify::verify_program(&program) {
        return LowerOutcome::VerifierRejected {
            detail: format!("{error:#?}"),
        };
    }
    trace.stages.push("verify SSA: reached".to_owned());
    trace.stages.push(format!(
        "verified SSA:\n{}",
        crate::ssa::render_program(&program)
    ));
    match crate::llvm::render_verified_program_with_entry(&program, entry) {
        Ok(llvm) => {
            trace
                .stages
                .push(format!("actual LLVM bytes={}; LLVM:\n{llvm}", llvm.len()));
            LowerOutcome::VerifiedLlvm
        }
        Err(error) => LowerOutcome::LlvmRejected {
            detail: format!("{error:#?}"),
        },
    }
}

fn print_outcome(outcome: &Outcome<LowerObservation>) {
    let outcome = match outcome {
        Ok(observation) => &observation.outcome,
        Err(failure) => {
            diagnostic!(
                "prerequisite failed stage={}; detail={}",
                failure.stage,
                failure.detail,
            );
            return;
        }
    };
    match outcome {
        LowerOutcome::VerifiedLlvm => diagnostic!(
            "observed outcome=VerifiedLlvm; per-source compatibility verdict still pending review"
        ),
        LowerOutcome::LowerRejected { error, origin, .. } => {
            let category = if error.kind == LoweringErrorKind::UnsupportedNode {
                "unreviewed capability rejection"
            } else {
                "unresolved lowering error (including MissingFact/InvalidModel)"
            };
            diagnostic!(
                "observed outcome=LowerRejected; category={category}; error={error:#?}; origin={origin:?}"
            );
        }
        LowerOutcome::VerifierRejected { detail } => diagnostic!(
            "observed outcome=VerifierRejected; unresolved verifier defect; detail={detail}"
        ),
        LowerOutcome::LlvmRejected { detail } => {
            diagnostic!("observed outcome=LlvmRejected; unresolved LLVM defect; detail={detail}")
        }
    }
}

fn collect_observations(strong_contract: bool) -> Vec<Observation> {
    let mut observations = Vec::with_capacity(8);
    for case in CASES {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                unit::observe(case, strong_contract, &mut trace)
            } else {
                memory::observe(case, strong_contract, &mut trace)
            };
            observations.push(Observation {
                label: format!("{}/{}", case.id, if unit { "unit" } else { "single" }),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        diagnostic!(
            "RESOURCE BASELINE {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        diagnostic!("input failures={:?}", observation.trace.input_failures);
        print_outcome(&observation.outcome);
    }
    // No per-source behavior assertion precedes collection of all eight ordinary entries.
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(|observation| {
            observation.outcome.is_ok() && observation.trace.input_failures.is_empty()
        }),
        "all eight sources must pass input/FE/ownership gates and attempt real lowering"
    );
    observations
}

#[test]
fn nullable_map_resource_sources_collect_baseline_outcomes() {
    let _ = collect_observations(false);
    diagnostic!(
        "COLLECTION ONLY: FE/ownership/input gates passed; eight lower outcomes recorded. \
         No per-source success or rejection contract has been accepted by this collector."
    );
}

#[derive(Debug)]
struct Baseline {
    label: &'static str,
    entry_name: &'static str,
    entry_path: &'static str,
}

// Both original MemoryDocument and rooted p/q layouts now require verified LLVM.
// Historical exact p/q rejections are retained outside this active contract.
// Never allow wildcard errors, ignored rows, or skipped entries to hide a regression.
const RESOURCE_COMPATIBILITY: &[Baseline] = &[
    Baseline {
        label: "R0/single",
        entry_name: "main",
        entry_path: "r0_remove_only.ko",
    },
    Baseline {
        label: "R0/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R1/single",
        entry_name: "main",
        entry_path: "r1_ordinary_source_before_remove.ko",
    },
    Baseline {
        label: "R1/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R2/single",
        entry_name: "main",
        entry_path: "r2_ordinary_source_after_remove.ko",
    },
    Baseline {
        label: "R2/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R3/single",
        entry_name: "main",
        entry_path: "r3_ordinary_null_only.ko",
    },
    Baseline {
        label: "R3/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
];

#[test]
fn nullable_map_resource_sources_preserve_compatibility() {
    let observations = collect_observations(true);
    assert_eq!(observations.len(), RESOURCE_COMPATIBILITY.len());
    let mut mismatches = Vec::new();
    for (actual, expected) in observations.iter().zip(RESOURCE_COMPATIBILITY) {
        let matched = actual.label == expected.label
            && actual.outcome.as_ref().is_ok_and(|observed| {
                observed.entry.name == expected.entry_name
                    && observed
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|path| path.as_str() == expected.entry_path)
                    && !observed.entry.span.is_empty()
                    && matches!(&observed.outcome, LowerOutcome::VerifiedLlvm)
            });
        diagnostic!(
            "RESOURCE COMPATIBILITY {}: matched={matched}",
            expected.label
        );
        if !matched {
            mismatches.push(format!("expected={expected:#?}; actual={actual:#?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "exact Resource compatibility mismatches:\n{}",
        mismatches.join("\n")
    );
}
