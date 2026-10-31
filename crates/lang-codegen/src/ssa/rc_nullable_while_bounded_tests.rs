//! SPEC-0332 bounded ordinary sources; SSA/LLVM only, no native execution.
use std::fmt::Debug;

#[path = "rc_nullable_while_memory_document_tests.rs"]
mod memory_document;

#[path = "rc_nullable_while_presence_identity.rs"]
mod identity;
#[path = "rc_nullable_while_presence_facts.rs"]
mod identity_facts;

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::{Item, NameMarker, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

use super::{
    LoweringError, lower_scalar_file_with_entry,
    model::{FunctionId, Operation, Origin, Program},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    verify::verify_program,
};

/// Shared real source bytes for this SSA test and a separately reviewed native test.
pub(crate) fn sources() -> [(&'static str, &'static str); 4] {
    [
        (
            "normal",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/normal.ko"),
        ),
        (
            "break",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/break.ko"),
        ),
        (
            "continue",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/continue.ko"),
        ),
        (
            "return",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/return.ko"),
        ),
    ]
}

struct Failure {
    stage: &'static str,
    detail: String,
    lower: Option<LoweringError>,
    span_text: Option<String>,
}

fn failure(stage: &'static str, error: impl Debug) -> Failure {
    Failure {
        stage,
        detail: format!("{error:?}"),
        lower: None,
        span_text: None,
    }
}

struct Observation {
    case: &'static str,
    unit: bool,
    complete_ok: bool,
    ownership_ok: bool,
    lower_attempted: bool,
    entry_identity_ok: bool,
    outcome: Result<(), Failure>,
}

fn source_functions(
    sources: &SourceMap,
    file: &ParsedFile,
) -> Result<Vec<(&'static str, Span)>, Failure> {
    if file.roots().len() != 3 {
        return Err(failure(
            "source-identity",
            "expected exactly three function roots",
        ));
    }
    let mut result = Vec::new();
    for expected in ["entry", "inspect", "readResult"] {
        let mut spans = Vec::new();
        for root in file.roots() {
            let node = file
                .ast()
                .items()
                .get(*root)
                .map_err(|error| failure("source-identity", error))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
                && sources.slice(*name).ok() == Some(expected)
            {
                spans.push(node.span());
            }
        }
        let [span] = spans.as_slice() else {
            return Err(failure("source-identity", (expected, spans)));
        };
        result.push((expected, *span));
    }
    Ok(result)
}

fn verify_entry_sources(
    sources: &SourceMap,
    program: &Program,
    entry: FunctionId,
    expected: &[(&str, Span)],
) -> Result<(), Failure> {
    let [module] = program.modules.as_slice() else {
        return Err(failure("entry-identity", "expected one real source module"));
    };
    let mut functions = Vec::new();
    for (name, span) in expected {
        let matches = module
            .functions
            .iter()
            .filter(|function| matches!(&function.origin, Origin::Source(actual) if actual == span))
            .collect::<Vec<_>>();
        let [function] = matches.as_slice() else {
            return Err(failure("entry-identity", (name, span, matches.len())));
        };
        println!(
            "SPEC0332 BOUNDED FUNCTION source_name={name} id={:?} name={:?} span={span:?} text={:?}",
            function.id,
            function.name,
            sources.slice(*span)
        );
        if *name == "entry" && function.id != entry {
            return Err(failure(
                "entry-identity",
                "lower returned a different source function",
            ));
        }
        functions.push(function.id);
    }
    let mut pending = vec![entry];
    let mut reachable = Vec::new();
    while let Some(id) = pending.pop() {
        if reachable.contains(&id) {
            continue;
        }
        let function = module
            .function(id)
            .ok_or_else(|| failure("entry-call-graph", ("missing real function", id)))?;
        reachable.push(id);
        let entry_block = function
            .entry_block()
            .ok_or_else(|| failure("entry-call-graph", ("missing entry block", id)))?;
        let mut pending_blocks = vec![entry_block];
        let mut reachable_blocks = Vec::new();
        while let Some(block_id) = pending_blocks.pop() {
            if reachable_blocks.contains(&block_id) {
                continue;
            }
            let block = function
                .block(block_id)
                .ok_or_else(|| failure("entry-call-graph", ("missing real block", block_id)))?;
            reachable_blocks.push(block_id);
            for instruction_id in &block.instructions {
                let instruction = function.instruction(*instruction_id).ok_or_else(|| {
                    failure(
                        "entry-call-graph",
                        ("missing real instruction", instruction_id),
                    )
                })?;
                if let Operation::DirectCall { callee, .. } = &instruction.operation {
                    println!(
                        "SPEC0332 BOUNDED CALL caller={id:?} block={block_id:?} callee={callee:?} origin={:?}",
                        instruction.origin
                    );
                    pending.push(*callee);
                }
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                failure("entry-call-graph", ("missing real terminator", block_id))
            })?;
            pending_blocks.extend(terminator.kind.targets());
        }
    }
    if !functions.iter().all(|id| reachable.contains(id)) {
        return Err(failure("entry-call-graph", (functions, reachable)));
    }
    Ok(())
}

fn observe(case: &'static str, source_text: &str, unit: bool) -> Observation {
    observe_with_identity(case, source_text, unit, false).0
}

fn observe_with_identity(
    case: &'static str,
    source_text: &str,
    unit: bool,
    inspect_identity: bool,
) -> (Observation, Result<(), String>) {
    let mut source_identity = Err("real FE identity query not reached".to_owned());
    let mut identity = Err("real SSA identity query not reached".to_owned());
    let mut complete_ok = false;
    let mut ownership_ok = false;
    let mut lower_attempted = false;
    let mut entry_identity_ok = false;
    let outcome = (|| {
        let mut sources = SourceMap::new();
        let (path, text) = if unit {
            (format!("p/{case}.ko"), format!("package p\n{source_text}"))
        } else {
            (format!("{case}.ko"), source_text.to_owned())
        };
        println!("SPEC0332 BOUNDED INPUT case={case} unit={unit} path={path:?} source={text:?}");
        let source = sources
            .add_source(&path, &text)
            .map_err(|error| failure("source-map", error))?;
        let lexed = lex(&sources, source).map_err(|error| failure("lex-api", error))?;
        if !lexed.diagnostics().is_empty() {
            return Err(failure("lex-diagnostics", lexed.diagnostics()));
        }
        let file = parse_file(&sources, &lexed).map_err(|error| failure("parse-api", error))?;
        if !file.diagnostics().is_empty() {
            return Err(failure("parse-diagnostics", file.diagnostics()));
        }
        let function_spans = source_functions(&sources, &file)?;
        println!("SPEC0332 BOUNDED STAGE case={case} unit={unit} parse-and-source-identity=ok");
        let (environment, types) = standard_environments();
        let lowered = if unit {
            let inputs = [SourceUnitInput::new("root", &path, source, &file)];
            let index = index_compilation_unit(&sources, &inputs)
                .map_err(|error| failure("index-api", error))?;
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .map_err(|error| failure("names-api", error))?
                .validate()
                .map_err(|error| failure("names-validation", error))?;
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            let typed = typed
                .validate()
                .map_err(|error| failure("types-validation", error))?;
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| failure("ownership-api", error))?;
            if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
                return Err(failure(
                    "ownership-diagnostics-or-deferred",
                    (owned.diagnostics(), owned.deferred()),
                ));
            }
            let owned = owned
                .validate()
                .map_err(|error| failure("ownership-validation", error))?;
            ownership_ok = true;
            if inspect_identity {
                source_identity =
                    identity_facts::unit(&sources, &file, &names, &typed, owned.ownership(), case);
                println!(
                    "SPEC0332 PRESENCE FE IDENTITY case={case} unit={unit} query={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            let entries = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.name() == "entry"
                        && declaration.kind() == SymbolKind::Function
                        && names.names().index().packages()[declaration.package().index()]
                            .name()
                            .expect("named package")
                            .segments()
                            .iter()
                            .map(String::as_str)
                            .eq(["p"])
                })
                .collect::<Vec<_>>();
            let [entry] = entries.as_slice() else {
                return Err(failure("entry-selection", "expected exactly one p.entry"));
            };
            lower_attempted = true;
            println!(
                "SPEC0332 BOUNDED STAGE case={case} unit={unit} complete=ok ownership=ok deferred=0 lower=attempt"
            );
            lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                entry.id(),
            )
        } else {
            let names = resolve_names(&sources, &file, &environment)
                .map_err(|error| failure("names-api", error))?;
            if !names.diagnostics().is_empty() {
                return Err(failure("names-diagnostics", names.diagnostics()));
            }
            let typed = check_types(&sources, &file, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_file_types(&sources, &file, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            let owned = check_ownership(&sources, &file, &names, &typed)
                .map_err(|error| failure("ownership-api", error))?;
            if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
                return Err(failure(
                    "ownership-diagnostics-or-deferred",
                    (owned.diagnostics(), owned.deferred()),
                ));
            }
            ownership_ok = true;
            if inspect_identity {
                source_identity =
                    identity_facts::single(&sources, &file, &names, &typed, &owned, case);
                println!(
                    "SPEC0332 PRESENCE FE IDENTITY case={case} unit={unit} query={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            let entries = names
                .symbols()
                .iter()
                .filter(|symbol| symbol.name() == "entry" && symbol.kind() == SymbolKind::Function)
                .collect::<Vec<_>>();
            let [entry] = entries.as_slice() else {
                return Err(failure("entry-selection", "expected exactly one entry"));
            };
            lower_attempted = true;
            println!(
                "SPEC0332 BOUNDED STAGE case={case} unit={unit} complete=ok ownership=ok deferred=0 lower=attempt"
            );
            lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry.id())
        };
        let (program, entry) = lowered.map_err(|error| Failure {
            stage: "lower",
            detail: format!("{error:?}"),
            lower: Some(error),
            span_text: error.span.map(|span| format!("{:?}", sources.slice(span))),
        })?;
        println!(
            "SPEC0332 BOUNDED SSA case={case} unit={unit}\n{}",
            render_program(&program)
        );
        if inspect_identity {
            identity = source_identity
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|facts| identity::inspect_program(&program, entry, case, facts));
            println!("SPEC0332 PRESENCE SSA IDENTITY case={case} unit={unit} query={identity:?}");
        }
        verify_entry_sources(&sources, &program, entry, &function_spans)?;
        entry_identity_ok = true;
        verify_program(&program).map_err(|error| failure("ssa-verify", error))?;
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("llvm-render-verify", error))?;
        println!("SPEC0332 BOUNDED LLVM case={case} unit={unit}\n{llvm}");
        Ok(())
    })();
    (
        Observation {
            case,
            unit,
            complete_ok,
            ownership_ok,
            lower_attempted,
            entry_identity_ok,
            outcome,
        },
        identity,
    )
}

// Characterizes observed implementation debt; not a language rejection contract.
#[test]
fn bounded_payload_and_owner_return_sources_record_current_stage_boundary() {
    let mut observations = Vec::new();
    for (case, source) in sources() {
        for unit in [false, true] {
            observations.push(observe(case, source, unit));
        }
    }
    // Every reachable stage in all eight ordinary pipelines finishes before an oracle.
    for observation in &observations {
        let case = observation.case;
        let unit = observation.unit;
        println!(
            "SPEC0332 BOUNDED PRECONDITIONS case={case} unit={unit} complete={} ownership={} lower_attempted={} entry_identity={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.lower_attempted,
            observation.entry_identity_ok
        );
        match &observation.outcome {
            Ok(()) => println!(
                "SPEC0332 BOUNDED OUTCOME case={case} unit={unit} stage=llvm-render-verify result=ok"
            ),
            Err(error) => println!(
                "SPEC0332 BOUNDED OUTCOME case={case} unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage, error.detail, error.lower, error.span_text
            ),
        }
    }
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(|observation| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.lower_attempted
                && match (&observation.outcome, observation.unit) {
                    (Ok(()), false) => observation.entry_identity_ok,
                    (Err(error), true) => {
                        // Published 054a74d: the real Rc If proof now rejects the
                        // reader return before the historical invalid SSA is built.
                        // These are the unchanged source's full return spans after
                        // its existing "package p\n" prefix, not a success oracle.
                        let expected_span = match observation.case {
                            "normal" => (265, 283),
                            "break" => (221, 239),
                            "continue" => (284, 302),
                            "return" => (229, 247),
                            _ => return false,
                        };
                        !observation.entry_identity_ok
                            && error.stage == "lower"
                            && matches!(
                                error.lower,
                                Some(LoweringError {
                                    kind: super::LoweringErrorKind::UnsupportedNode,
                                    span: Some(span),
                                }) if (span.start(), span.end()) == expected_span
                            )
                            && error.span_text.as_deref() == Some("Ok(\"return owner.value\")")
                    }
                    _ => false,
                }
        }),
        "observed boundary is exactly four single LLVM successes and four source-anchored Unit live-If return rejections"
    );
}

/// Independent presence-only source bytes and golden output for later native reuse.
pub(crate) fn presence_cases() -> [(&'static str, &'static str, &'static str); 4] {
    [
        (
            "normal-presence",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/normal-presence.ko"),
            include_str!("../../tests/fixtures/rc-nullable-while-0332/normal-presence.stdout"),
        ),
        (
            "break-presence",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/break-presence.ko"),
            include_str!("../../tests/fixtures/rc-nullable-while-0332/break-presence.stdout"),
        ),
        (
            "continue-presence",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/continue-presence.ko"),
            include_str!("../../tests/fixtures/rc-nullable-while-0332/continue-presence.stdout"),
        ),
        (
            "return-presence",
            include_str!("../../tests/fixtures/rc-nullable-while-0332/return-presence.ko"),
            include_str!("../../tests/fixtures/rc-nullable-while-0332/return-presence.stdout"),
        ),
    ]
}

#[test]
fn bounded_presence_rc_nullable_while_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (case, source, _golden) in presence_cases() {
        for unit in [false, true] {
            observations.push(observe(case, source, unit));
        }
    }
    // All eight independent presence pipelines finish before the success oracle.
    for observation in &observations {
        let case = observation.case;
        let unit = observation.unit;
        println!(
            "SPEC0332 PRESENCE PRECONDITIONS case={case} unit={unit} complete={} ownership={} lower_attempted={} entry_identity={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.lower_attempted,
            observation.entry_identity_ok
        );
        match &observation.outcome {
            Ok(()) => println!(
                "SPEC0332 PRESENCE OUTCOME case={case} unit={unit} stage=llvm-render-verify result=ok"
            ),
            Err(error) => println!(
                "SPEC0332 PRESENCE OUTCOME case={case} unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage, error.detail, error.lower, error.span_text
            ),
        }
    }
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(|observation| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.lower_attempted
                && observation.entry_identity_ok
                && observation.outcome.is_ok()
        }),
        "all eight bounded presence pipelines must reach source-anchored verified LLVM"
    );
}
