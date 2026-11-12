//! SPEC-0332 bounded ordinary sources; SSA/LLVM only, no native execution.
use crate::test_support::diagnostics::diagnostic;
use std::fmt::Debug;

#[path = "rc_nullable_while_memory_document_tests.rs"]
pub(in crate::ssa) mod memory_document;

pub(in crate::ssa) use crate::ssa::loan_region_ssa_tests::named_literal_mapping::rc_nullable_while_presence_identity as identity;
#[path = "rc_nullable_while_presence_facts.rs"]
pub(in crate::ssa) mod identity_facts;

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
        diagnostic!(
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
                    diagnostic!(
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

pub(in crate::ssa) fn observe_with_identity(
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
        diagnostic!("SPEC0332 BOUNDED INPUT case={case} unit={unit} path={path:?} source={text:?}");
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
        diagnostic!("SPEC0332 BOUNDED STAGE case={case} unit={unit} parse-and-source-identity=ok");
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
                diagnostic!(
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
            diagnostic!(
                "SPEC0332 BOUNDED STAGE case={case} unit={unit} complete=ok ownership=ok deferred=0 lower=attempt"
            );
            crate::ssa::with_per_family_lowering(|| {
                lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    &owned,
                    entry.id(),
                )
            })
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
                diagnostic!(
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
            diagnostic!(
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
        diagnostic!(
            "SPEC0332 BOUNDED SSA case={case} unit={unit}\n{}",
            render_program(&program)
        );
        if inspect_identity {
            identity = source_identity
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|facts| identity::inspect_program(&program, entry, case, facts));
            diagnostic!(
                "SPEC0332 PRESENCE SSA IDENTITY case={case} unit={unit} query={identity:?}"
            );
        }
        verify_entry_sources(&sources, &program, entry, &function_spans)?;
        entry_identity_ok = true;
        verify_program(&program).map_err(|error| failure("ssa-verify", error))?;
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("llvm-render-verify", error))?;
        diagnostic!("SPEC0332 BOUNDED LLVM case={case} unit={unit}\n{llvm}");
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
        diagnostic!(
            "SPEC0332 BOUNDED PRECONDITIONS case={case} unit={unit} complete={} ownership={} lower_attempted={} entry_identity={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.lower_attempted,
            observation.entry_identity_ok
        );
        match &observation.outcome {
            Ok(()) => diagnostic!(
                "SPEC0332 BOUNDED OUTCOME case={case} unit={unit} stage=llvm-render-verify result=ok"
            ),
            Err(error) => diagnostic!(
                "SPEC0332 BOUNDED OUTCOME case={case} unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage,
                error.detail,
                error.lower,
                error.span_text
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
                        // SPEC-0401 retired the Rc If proof entry that used to reject
                        // the enclosing reader return; the Unit route now rejects the
                        // payload operation itself before invalid SSA is built. These
                        // are the unchanged source's `.value` read spans after its
                        // existing "package p\n" prefix, not a success oracle.
                        let expected_span = match observation.case {
                            "normal" => (272, 283),
                            "break" => (228, 239),
                            "continue" => (291, 302),
                            "return" => (236, 247),
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
                            && error.span_text.as_deref() == Some("Ok(\"owner.value\")")
                    }
                    _ => false,
                }
        }),
        "observed boundary is exactly four single LLVM successes and four source-anchored Unit payload-operation rejections"
    );
}

/// Independent presence-only source bytes and golden output for later native reuse.
pub(crate) pub(in crate::ssa) fn presence_cases() -> [(&'static str, &'static str, &'static str); 4] {
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
        observations.push(observe(case, source, true));
    }
    // Original bytes now have the strict MemoryDocument success witness in
    // memory_document. Keep these four package-qualified Rooted observations;
    // the separate Single cleanup-identity and payload-boundary witnesses stay.
    for observation in &observations {
        let case = observation.case;
        let unit = observation.unit;
        diagnostic!(
            "SPEC0332 PRESENCE PRECONDITIONS case={case} unit={unit} complete={} ownership={} lower_attempted={} entry_identity={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.lower_attempted,
            observation.entry_identity_ok
        );
        match &observation.outcome {
            Ok(()) => diagnostic!(
                "SPEC0332 PRESENCE OUTCOME case={case} unit={unit} stage=llvm-render-verify result=ok"
            ),
            Err(error) => diagnostic!(
                "SPEC0332 PRESENCE OUTCOME case={case} unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage,
                error.detail,
                error.lower,
                error.span_text
            ),
        }
    }
    assert_eq!(observations.len(), 4);
    assert!(
        observations.iter().all(|observation| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.lower_attempted
                && observation.entry_identity_ok
                && observation.outcome.is_ok()
        }),
        "all four Rooted presence pipelines must reach source-anchored verified LLVM"
    );
}

pub(in crate::ssa) mod unit_root_primitive_tests {
//! SPEC-0244 compilation-unit owned-root atomic replace/swap regressions.

use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::super::{
    LoweringError, LoweringErrorKind,
    model::{Function, Operation, Program},
    render::render_program,
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    unit_lower_test_support::{analyze, declaration, parsed},
};

fn lower(text: &str, constants: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        &format!("package test\n{text}"),
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    if !constants {
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        return lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .map(|(program, _)| program);
    }
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .map(|(program, _)| program)
}

fn entry(program: &Program) -> &Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("test.entry."))
        .unwrap()
}

fn count(function: &Function, prefix: &str) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| format!("{:?}", instruction.operation).starts_with(prefix))
        .count()
}

#[test]
fn unit_root_primitive_replace_and_swap_keep_each_string_owner() {
    for constants in [false, true] {
        let program = lower(
            "fun entry(): Unit {\nvar left = \"left\"\nvar right = \"right\"\nval old = replace(&left, \"new\")\nswap(&left, &right)\nprintln(old)\nprintln(left)\nprintln(right)\n}",
            constants,
        ).expect("owned-root primitives lower to verified SSA");
        let function = entry(&program);
        assert_eq!(count(function, "RootReplace"), 1);
        assert_eq!(count(function, "RootSwap"), 1);
        assert_eq!(count(function, "Drop"), 3);
    }
}

#[test]
fn unit_root_primitive_named_new_value_is_delivered_once() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nval replacement = \"new\"\nval previous = replace(&target, replacement)\nprintln(target)\nreturn previous\n}", false,
    ).expect("Value delivery removes the named replacement owner");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 1);
    assert_eq!(count(function, "Drop"), 1);
}

#[test]
fn unit_root_primitive_conditional_new_value_keeps_root_and_loan_through_cfg() {
    let program = lower(
        "fun entry(own flag: Boolean): String {\nvar target = \"old\"\nval previous = replace(&target, if (flag) { \"yes\" } else { \"no\" })\nprintln(target)\nreturn previous\n}", true,
    ).expect("pending root owner and exclusive loan are rebound through CFG");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}

#[test]
fn unit_root_primitive_return_prefix_ends_loan_without_committing() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nval unused = replace(&target, return \"early\")\nreturn unused\n}", true,
    ).expect("early return cancels the pending primitive before cleanup");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "BorrowEnd"), 1);
    let end = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .unwrap();
    let drop = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .unwrap();
    assert!(end < drop);
}

#[test]
fn unit_root_primitive_cross_file_input_order_is_deterministic() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun make(): String = \"new\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        "package q\nfun entry(): String {\nvar target = \"old\"\nval old = replace(&target, p.make())\nprintln(target)\nreturn old\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &name_environment, &environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .unwrap();
    let (reverse, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .unwrap();
    assert_text_eq!(render_program(&forward), render_program(&reverse));
}

#[test]
fn unit_root_primitive_inout_parameter_remains_unsupported() {
    let error = lower(
        "fun entry(inout target: String): String = replace(&target, \"new\")",
        false,
    )
    .err()
    .expect("Inout parameter is not an owned root");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn unit_root_primitive_existing_storage_families_lower() {
    for (declarations, ty, first, second) in [
        ("", "Int", "11", "22"),
        ("", "Boolean", "true", "false"),
        (
            "class Item(val number: Int)",
            "Item",
            "Item(11)",
            "Item(22)",
        ),
        (
            "value class Item(val text: String)",
            "Item",
            "Item(\"one\")",
            "Item(\"two\")",
        ),
        (
            "value class Item(val number: Int)",
            "Item",
            "Item(11)",
            "Item(22)",
        ),
        (
            "enum class Item { Full(text: String), Empty }",
            "Item",
            "Item.Full(\"one\")",
            "Item.Full(\"two\")",
        ),
        (
            "value class Item(val number: Int)",
            "Box<Item>",
            "Box(Item(11))",
            "Box(Item(22))",
        ),
        ("", "Rc<Int>", "Rc(11)", "Rc(22)"),
        ("", "List<Int>", "listOf(11)", "listOf(22)"),
        ("", "Array<String>", "arrayOf(\"one\")", "arrayOf(\"two\")"),
        ("", "Rc<Int>?", "null", "Rc(22)"),
    ] {
        let source = format!(
            "{declarations}\nfun entry(): {ty} {{\nvar left: {ty} = {first}\nvar right: {ty} = {second}\nval old = replace(&left, {second})\nswap(&left, &right)\nreturn old\n}}"
        );
        let program = lower(&source, false).unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        assert_eq!(count(entry(&program), "RootReplace"), 1, "{ty}");
        assert_eq!(count(entry(&program), "RootSwap"), 1, "{ty}");
    }
}

#[test]
fn unit_root_primitive_copyable_bindings_do_not_alias_storage() {
    let program = lower(
        "fun entry(): Int {\nvar left = 7\nvar right = left\nval replacement = right\nval old = replace(&left, replacement)\nswap(&left, &right)\nreturn old + left + right\n}", false,
    ).expect("independent copyable mutable roots have independent SSA identities");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
    assert_eq!(count(entry(&program), "RootSwap"), 1);
}

#[test]
fn unit_root_primitive_abort_has_no_commit_or_cleanup() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nreturn replace(&target, error(\"stop\"))\n}",
        true,
    )
    .expect("aborting new-value expression only lowers the evaluated prefix");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "Drop"), 0);
}

#[test]
fn unit_root_primitive_projection_boundaries_remain_explicit() {
    let source = "fun entry(): String {\nvar items = arrayOf(\"old\")\nreturn replace(&items[0], \"new\")\n}";
    let error = lower(source, false)
        .err()
        .expect("indexed roots remain outside this slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn unit_root_primitive_conditional_control_exits_cancel_only_the_exit_path() {
    for exit in ["return", "break", "continue"] {
        let source = format!(
            "fun entry(own flag: Boolean): Unit {{\nvar target = \"old\"\nvar once = true\nloop {{\nif (!once) {{ break }}\nonce = false\nval previous = replace(&target, if (flag) {{ {exit} }} else {{ \"new\" }})\nprintln(previous)\nbreak\n}}\nprintln(target)\n}}"
        );
        let program = lower(&source, true).unwrap_or_else(|error| panic!("{exit}: {error:?}"));
        let function = entry(&program);
        assert_eq!(count(function, "RootReplace"), 1, "{exit}");
        assert!(count(function, "BorrowEnd") >= 1, "{exit}");
    }
}

#[test]
fn unit_root_primitive_named_source_calls_do_not_gain_intrinsic_identity() {
    let source = "fun replace(own left: Int, own right: Int): Int = left + right\nfun swap(own left: Int, own right: Int): Int = left - right\nfun entry(): Int = replace(7, swap(4, 2))";
    let program = lower(source, false).expect("same-named declarations are ordinary calls");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "RootSwap"), 0);
    assert_eq!(count(function, "DirectCall"), 2);
}

#[test]
fn unit_root_primitive_nested_replace_commits_each_distinct_root_once() {
    let program = lower(
        "fun entry(): String {\nvar first = \"first\"\nvar second = \"second\"\nval previous = replace(&first, replace(&second, \"third\"))\nprintln(first)\nprintln(second)\nreturn previous\n}", false,
    ).expect("nested primitive frames preserve the outer exclusive loan");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 2);
    assert_eq!(count(function, "Drop"), 2);
}

#[test]
fn unit_root_primitive_unit_values_materialize_storage() {
    let program = lower(
        "fun make(): Unit {}\nfun entry(): Unit {\nvar left = make()\nvar right = make()\nval old = replace(&left, make())\nswap(&left, &right)\nold\nreturn\n}", false,
    ).expect("Unit roots materialize zero-sized values for atomic operations");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 1);
    assert_eq!(count(function, "RootSwap"), 1);
    assert_eq!(count(function, "Drop"), 0);
}

#[test]
fn unit_root_primitive_copyable_pending_root_survives_inner_loop() {
    let program = lower(
        "fun entry(own flag: Boolean): Int {\nvar target = 7\nval old = replace(&target, if (flag) { loop { break }\n9 } else { 11 })\nreturn old + target\n}", true,
    ).expect("the pending exclusive root remains the same binding after an inner loop");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}

#[test]
fn unit_root_primitive_unit_pending_root_survives_conditional_loop_exit() {
    let program = lower(
        "fun make(): Unit {}\nfun entry(own flag: Boolean): Unit {\nvar target = make()\nloop {\nval old = replace(&target, if (flag) { break } else { make() })\nbreak\n}\ntarget\n}", true,
    ).expect("Unit root binding representation stays consistent across early and committed exits");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}
}
