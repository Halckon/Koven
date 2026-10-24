//! SPEC-0291: the unchanged native fixture must pass complete analysis and LLVM.
//! Collect both ordinary entries before asserting; this test never emits or runs an object.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, SymbolKind, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

use super::model::{FunctionId, Operation, Program};

const SOURCE: &str = include_str!("../../tests/fixtures/map-consume-0291/owned-locals.ko");
const CONSUMER: &str = "package q\nfun entry(): Unit { p.main() }";

/// Fixed ordinary fixture inputs; the existing wrapper keeps its six-consume contract.
pub(super) struct Fixture {
    pub(super) source_name: &'static str,
    pub(super) source: &'static str,
    pub(super) consumer: &'static str,
    pub(super) consumes: usize,
}

const OWNED_LOCALS: Fixture = Fixture {
    source_name: "map-consume-owned-locals.ko",
    source: SOURCE,
    consumer: CONSUMER,
    consumes: 6,
};

type UnitInspector = fn(
    SourceUnitId,
    &ParsedFile,
    &ValidatedCompilationUnitNames,
    &CompilationUnitTypes,
    &CompilationUnitOwnership,
    &mut Vec<String>,
) -> Outcome<()>;

#[derive(Debug)]
pub(super) struct Failure {
    pub(super) stage: String,
    pub(super) detail: String,
}

pub(super) type Outcome<T> = Result<T, Failure>;

#[derive(Debug)]
struct Observation {
    entry: &'static str,
    stages: Vec<String>,
    outcome: Outcome<()>,
}

fn step<T, E: std::fmt::Debug>(
    stage: &str,
    result: Result<T, E>,
    stages: &mut Vec<String>,
) -> Outcome<T> {
    match result {
        Ok(value) => {
            stages.push(format!("{stage}: reached"));
            Ok(value)
        }
        Err(error) => Err(Failure {
            stage: stage.to_owned(),
            detail: format!("{error:?}"),
        }),
    }
}

pub(super) fn gate(
    stage: &str,
    valid: bool,
    detail: impl std::fmt::Debug,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    if valid {
        stages.push(format!("{stage}: passed"));
        Ok(())
    } else {
        Err(Failure {
            stage: stage.to_owned(),
            detail: format!("{detail:?}"),
        })
    }
}

fn parsed(
    sources: &mut SourceMap,
    name: &str,
    text: &str,
    stages: &mut Vec<String>,
) -> Outcome<ParsedFile> {
    let source = step(
        &format!("source {name}"),
        sources.add_source(name, text),
        stages,
    )?;
    let lexed = step(&format!("lex {name}"), lex(sources, source), stages)?;
    gate(
        &format!("lex diagnostics {name}"),
        lexed.diagnostics().is_empty(),
        lexed.diagnostics(),
        stages,
    )?;
    let parsed = step(
        &format!("parse {name}"),
        parse_file(sources, &lexed),
        stages,
    )?;
    gate(
        &format!("parse diagnostics {name}"),
        parsed.diagnostics().is_empty(),
        parsed.diagnostics(),
        stages,
    )?;
    Ok(parsed)
}

fn lowering_error(sources: &SourceMap, error: super::LoweringError) -> String {
    let origin = error
        .span
        .map(|span| (sources.source_name(span.source_id()), sources.slice(span)));
    format!("{error:?}; real source origin={origin:?}")
}

fn verify_and_render(
    program: &Program,
    entry: FunctionId,
    expected_consumes: usize,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    step("verify SSA", super::verify::verify_program(program), stages)?;
    stages.push(format!(
        "actual entry={entry:?}; SSA:\n{}",
        super::render_program(program)
    ));
    let consumes: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.instructions)
        .filter(|instruction| matches!(instruction.operation, Operation::MapConsume { .. }))
        .collect();
    let consumes_match = gate(
        &format!("{expected_consumes} lowered consumes"),
        consumes.len() == expected_consumes,
        &consumes,
        stages,
    );
    let llvm = step(
        "in-memory LLVM with selected entry",
        crate::llvm::render_verified_program_with_entry(program, entry),
        stages,
    )?;
    let headers: Vec<_> = llvm
        .lines()
        .filter(|line| line.starts_with("%koven.map.t"))
        .collect();
    stages.push(format!(
        "actual LLVM Map headers={headers:?}; bytes={}",
        llvm.len()
    ));
    consumes_match
}

fn record_inspection(outcome: &Outcome<()>, stages: &mut Vec<String>) {
    if let Err(failure) = outcome {
        stages.push(format!(
            "fixture input failed stage={}; detail={}",
            failure.stage, failure.detail,
        ));
    }
}

fn observe_single(stages: &mut Vec<String>) -> Outcome<()> {
    observe_single_fixture(&OWNED_LOCALS, |_, _, _, _, _, _| Ok(()), stages)
}

pub(super) fn observe_single_fixture(
    fixture: &Fixture,
    inspect: UnitInspector,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    observe_fixture(fixture, false, inspect, stages)
}

fn observe_unit(stages: &mut Vec<String>) -> Outcome<()> {
    observe_unit_fixture(&OWNED_LOCALS, |_, _, _, _, _, _| Ok(()), stages)
}

pub(super) fn observe_unit_fixture(
    fixture: &Fixture,
    inspect: UnitInspector,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    observe_fixture(fixture, true, inspect, stages)
}

// Each source shape gets its own real analysis. Result-based stages keep a failure in
// either entry from preventing collection of the other; no panic-based UnitSource.
fn observe_fixture(
    fixture: &Fixture,
    cross_file: bool,
    inspect: UnitInspector,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    let mut sources = SourceMap::new();
    let provider = if cross_file {
        parsed(
            &mut sources,
            "p/provider.ko",
            &format!("package p\n{}", fixture.source),
            stages,
        )?
    } else {
        parsed(&mut sources, fixture.source_name, fixture.source, stages)?
    };
    let consumer = if cross_file {
        Some(parsed(
            &mut sources,
            "q/consumer.ko",
            fixture.consumer,
            stages,
        )?)
    } else {
        None
    };
    let inputs = if let Some(consumer) = &consumer {
        vec![
            SourceUnitInput::new("root", "p/provider.ko", provider.source_id(), &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer.source_id(), consumer),
        ]
    } else {
        vec![SourceUnitInput::memory_document(
            provider.source_id(),
            &provider,
        )]
    };
    let (environment, types) = standard_environments();
    let index = step("index", index_compilation_unit(&sources, &inputs), stages)?;
    let names = step(
        "names",
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
        stages,
    )?;
    gate(
        "names diagnostics",
        names.diagnostics().is_empty(),
        names.diagnostics(),
        stages,
    )?;
    let names = step(
        "qualified names",
        names
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let index = names.names().index();
    let actual_sources = index.source_units();
    let provider_units: Vec<_> = actual_sources
        .iter()
        .filter(|unit| {
            unit.source_id() == provider.source_id()
                && if cross_file {
                    unit.key()
                        .logical_path()
                        .is_some_and(|path| path.as_str() == "p/provider.ko")
                } else {
                    unit.key() == &SourceUnitKey::MemoryDocument
                }
        })
        .collect();
    let [provider_unit] = provider_units.as_slice() else {
        return Err(Failure {
            stage: "unique real fixture source unit".to_owned(),
            detail: format!("provider={provider_units:?}; all={actual_sources:?}"),
        });
    };
    let (entry_unit, entry_source, entry_name, entry_label) = if let Some(consumer) = &consumer {
        let consumer_units: Vec<_> = actual_sources
            .iter()
            .filter(|unit| {
                unit.source_id() == consumer.source_id()
                    && unit
                        .key()
                        .logical_path()
                        .is_some_and(|path| path.as_str() == "q/consumer.ko")
            })
            .collect();
        let [consumer_unit] = consumer_units.as_slice() else {
            return Err(Failure {
                stage: "unique real consumer source unit".to_owned(),
                detail: format!("consumer={consumer_units:?}; all={actual_sources:?}"),
            });
        };
        gate(
            "two distinct source-qualified packages",
            actual_sources.len() == 2
                && provider_unit.id() != consumer_unit.id()
                && index.packages().iter().any(|package| {
                    package.id() == provider_unit.package()
                        && package.name().is_some_and(|name| name.segments() == ["p"])
                })
                && index.packages().iter().any(|package| {
                    package.id() == consumer_unit.package()
                        && package.name().is_some_and(|name| name.segments() == ["q"])
                }),
            (actual_sources, index.packages()),
            stages,
        )?;
        (*consumer_unit, consumer.source_id(), "entry", "q.entry")
    } else {
        gate(
            "one actual MemoryDocument in the default package",
            actual_sources.len() == 1
                && index.packages().len() == 1
                && index.packages().iter().any(|package| {
                    package.id() == provider_unit.package()
                        && package.name().is_some_and(|name| name.is_default())
                }),
            (actual_sources, index.packages()),
            stages,
        )?;
        (*provider_unit, provider.source_id(), "main", "single main")
    };
    let entries: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == entry_name
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == entry_unit.id()
                && declaration.package() == entry_unit.package()
                && declaration.name_span().source_id() == entry_source
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: format!("unique real {entry_label}"),
            detail: format!(
                "candidates={entries:?}; all declarations={:?}",
                index.declarations()
            ),
        });
    };
    // Preserve the single-file non-synthetic symbol oracle through the selected
    // declaration's actual source-qualified symbol, not a name-only replacement.
    gate(
        "selected entry is the real non-synthetic source symbol",
        names
            .names()
            .declaration_symbol(entry.id())
            .is_some_and(|selected| {
                selected.source_unit() == entry_unit.id()
                    && names.names().source_units().iter().any(|unit| {
                        unit.source_unit() == selected.source_unit()
                            && unit.resolution().symbols().iter().any(|symbol| {
                                symbol.id() == selected.symbol()
                                    && symbol.name() == entry_name
                                    && symbol.kind() == SymbolKind::Function
                                    && !symbol.is_synthetic()
                                    && symbol.span() == entry.name_span()
                            })
                    })
            }),
        entry,
        stages,
    )?;
    stages.push(format!(
        "selected real {entry_label}={entry:?}; source units={actual_sources:?}"
    ));
    let typed = step(
        "raw types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
        stages,
    )?;
    gate(
        "raw type diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
        stages,
    )?;
    stages.push(format!(
        "all raw qualified consume descriptors={:#?}",
        typed.map_consumes()
    ));
    gate(
        &format!("{} real consume descriptors", fixture.consumes),
        typed.map_consumes().len() == fixture.consumes,
        typed.map_consumes(),
        stages,
    )?;
    for descriptor in typed.map_consumes() {
        gate(
            "raw consume source-qualified receiver/root/storage/read/result identities",
            descriptor.expression().source_unit() == provider_unit.id()
                && descriptor.receiver().source_unit() == provider_unit.id()
                && descriptor.source().is_some_and(|(root, storage)| {
                    root.source_unit() == provider_unit.id()
                        && typed.symbol_type(root) == Some(storage)
                        && names.names().source_units().iter().any(|unit| {
                            unit.source_unit() == root.source_unit()
                                && unit.resolution().symbols().iter().any(|symbol| {
                                    symbol.id() == root.symbol()
                                        && symbol.kind() == SymbolKind::Variable
                                        && symbol.span().source_id() == provider.source_id()
                                })
                        })
                })
                && typed.expression_type(descriptor.receiver()) == Some(descriptor.receiver_type())
                && typed.expression_type(descriptor.expression()) == Some(descriptor.result_type()),
            descriptor,
            stages,
        )?;
        let call = step(
            "qualified consume call AST",
            provider
                .ast()
                .expressions()
                .get(descriptor.expression().expression()),
            stages,
        )?;
        let receiver = step(
            "qualified consume receiver AST",
            provider
                .ast()
                .expressions()
                .get(descriptor.receiver().expression()),
            stages,
        )?;
        stages.push(format!(
            "real qualified descriptor={descriptor:?}; call span={:?}, text={:?}; receiver span={:?}, text={:?}",
            call.span(), sources.slice(call.span()), receiver.span(), sources.slice(receiver.span()),
        ));
        gate(
            "qualified consume call/receiver retain the actual source spans",
            call.span().source_id() == provider.source_id()
                && receiver.span().source_id() == provider.source_id(),
            (call.span(), receiver.span()),
            stages,
        )?;
    }
    let complete = step(
        "complete types",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "complete same analysis",
        complete.is_same_analysis(&typed) && std::ptr::eq(complete.types(), &typed),
        "original body typed identity",
        stages,
    )?;
    gate(
        "complete diagnostics",
        complete.types().diagnostics().is_empty(),
        complete.types().diagnostics(),
        stages,
    )?;
    gate(
        &format!("complete preserves all {} descriptors", fixture.consumes),
        complete.types().map_consumes() == typed.map_consumes(),
        complete.types().map_consumes(),
        stages,
    )?;
    let typed = step(
        "qualified types",
        typed
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let owned = step(
        "ownership",
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "ownership diagnostics",
        owned.diagnostics().is_empty(),
        owned.diagnostics(),
        stages,
    )?;
    gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
        stages,
    )?;
    let owned = step(
        "qualified ownership",
        owned
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let inspected = inspect(
        provider_unit.id(),
        &provider,
        &names,
        typed.types(),
        owned.ownership(),
        stages,
    );
    record_inspection(&inspected, stages);
    // The existing adapter seals these same validated products into an owned-unit
    // view and immediately enters lower_owned_unit_with_entry.
    let (program, function) = step(
        &format!("lower SSA with real {entry_label}"),
        super::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            entry.id(),
        )
        .map_err(|error| lowering_error(&sources, error)),
        stages,
    )?;
    let rendered = verify_and_render(&program, function, fixture.consumes, stages);
    rendered?;
    inspected
}

#[test]
fn complete_map_consume_fixture_reaches_verified_llvm() {
    let mut observations = Vec::with_capacity(2);
    for unit in [false, true] {
        let mut stages = Vec::new();
        let outcome = if unit {
            observe_unit(&mut stages)
        } else {
            observe_single(&mut stages)
        };
        observations.push(Observation {
            entry: if unit {
                "unit q.entry -> p.main"
            } else {
                "single main"
            },
            stages,
            outcome,
        });
    }
    // No assertion or old panic-based helper may stop collection after the first entry.
    let failed = observations
        .iter()
        .any(|observation| observation.outcome.is_err());
    for observation in &observations {
        if !failed {
            println!(
                "MAP CONSUME FIXTURE {}: verified LLVM ({} recorded stages)",
                observation.entry,
                observation.stages.len(),
            );
            continue;
        }
        println!(
            "MAP CONSUME FIXTURE {}\n{}",
            observation.entry,
            observation.stages.join("\n")
        );
        match &observation.outcome {
            Ok(()) => println!("outcome=verified LLVM"),
            Err(failure) => println!("failed stage={}; detail={}", failure.stage, failure.detail),
        }
    }
    assert_eq!(observations.len(), 2);
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()),
        "both complete ordinary fixture entries must reach verified LLVM: {observations:#?}"
    );
}
