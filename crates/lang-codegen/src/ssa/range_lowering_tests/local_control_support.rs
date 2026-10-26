//! Source-only setup and execution for the bounded local-control cases.
use super::super::*;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::ssa::{LoweringError, LoweringErrorKind};
use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitKey, SymbolKind},
    parser::{Item, NameMarker},
    source::{SourceId, Span},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Pipeline {
    Single,
    Unit,
    UnitReversed,
    SingleConstant,
    UnitConstant,
    UnitConstantReversed,
}

pub(super) struct SourceLowering {
    pub(super) sources: SourceMap,
    pub(super) expected_span: Span,
    pub(super) result: Result<(Program, FunctionId), LoweringError>,
}

pub(super) struct PositiveLowering {
    pub(super) expected_span: Span,
    products: PositiveProducts,
}

enum PositiveProducts {
    ExistingUnit(SourceLowering),
    MemoryUnit {
        // Keep the complete same-round owner, not an extracted or copied SourceMap.
        analysis: Box<UnitSource>,
        result: Result<(Program, FunctionId), (DeclarationId, LoweringError)>,
    },
}

// This entry is explicit at positive call sites. The old `lower` remains visible
// to boundary submodules and the receiver differential through `use super::*`.
pub(super) fn lower_positive(
    consumer: &str,
    fragment: &str,
    pipeline: Pipeline,
) -> PositiveLowering {
    if !matches!(pipeline, Pipeline::Single | Pipeline::SingleConstant) {
        let lowered = lower(consumer, fragment, pipeline);
        return PositiveLowering {
            expected_span: lowered.expected_span,
            products: PositiveProducts::ExistingUnit(lowered),
        };
    }
    let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
    let text = format!(
        "{}\n{consumer}",
        provider.strip_prefix("package koven.algorithms\n").unwrap()
    );
    let mut authorized_source = None;
    let analysis = UnitSource::analyze_with_type_environment(
        "range-local-control-single.ko",
        &text,
        |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
            authorized_source = Some(source);
        },
    )
    .unwrap_or_else(|error| panic!("original Range local source admission: {error:?}"));
    analysis.assert_frontend_clean();
    let source = analysis.parsed().source_id();
    assert_eq!(authorized_source, Some(source));
    let [input] = analysis.names().names().index().source_units() else {
        panic!("one original Range local MemoryDocument");
    };
    assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(input.source_id(), source);
    assert_eq!(
        analysis.sources().source_name(source).unwrap(),
        "range-local-control-single.ko"
    );
    assert_eq!(analysis.sources().source_text(source).unwrap(), text);
    let expected_span = fragment_span(analysis.sources(), source, consumer, fragment);
    let result = lower_positive_roots(&analysis);
    PositiveLowering {
        expected_span,
        products: PositiveProducts::MemoryUnit {
            analysis: Box::new(analysis),
            result,
        },
    }
}

fn lower_positive_roots(
    analysis: &UnitSource,
) -> Result<(Program, FunctionId), (DeclarationId, LoweringError)> {
    let parsed = analysis.parsed();
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original Range local MemoryDocument");
    };
    let mut functions = Vec::new();
    let mut concrete_roots = Vec::new();
    // Start from the actual AST, so an omitted index declaration cannot silently
    // turn the old whole-file obligation into main-only reachability.
    for root in parsed.roots() {
        let original = parsed.ast().items().get(*root).unwrap();
        let mut node = original;
        while let Item::Modified { declaration, .. } = node.payload() {
            node = parsed.ast().items().get(*declaration).unwrap();
        }
        let Item::Function {
            name: NameMarker::Present(name),
            type_parameters,
            parameters,
            ..
        } = node.payload()
        else {
            continue;
        };
        let declarations = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.root() == *root
                    && declaration.source_unit() == source.id()
                    && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("each original function root has one real declaration: {declarations:?}");
        };
        assert_eq!(declaration.package(), source.package());
        assert_eq!(declaration.name_span(), *name);
        assert_eq!(name.source_id(), parsed.source_id());
        assert_eq!(analysis.sources().slice(*name).unwrap(), declaration.name());
        assert_eq!(analysis.declaration_span(declaration.id()), original.span());
        let signature = analysis
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
            .expect("every original function has a same-round callable signature");
        assert_eq!(signature.type_parameters().len(), type_parameters.len());
        assert_eq!(signature.parameters().len(), parameters.len());
        assert!(!functions.contains(&declaration.id()));
        functions.push(declaration.id());
        if type_parameters.is_empty() {
            concrete_roots.push(*declaration);
        }
    }
    assert_eq!(
        functions.len(),
        index
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() == SymbolKind::Function)
            .count(),
        "all original function roots and index declarations are reconciled"
    );
    let main = analysis.declaration("main", SymbolKind::Function);
    assert!(concrete_roots.iter().any(|root| root.id() == main));
    let mut programs = Vec::new();
    for root in concrete_roots {
        let (program, entry) = analysis
            .lower_entry(root.id())
            .map_err(|error| (root.id(), error))?;
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        assert_eq!(
            function.origin,
            Origin::Source(analysis.declaration_span(root.id()))
        );
        verify_program(&program)
            .unwrap_or_else(|error| panic!("Range local root {root:?} SSA: {error:?}"));
        programs.push((root, program, entry));
    }
    // All roots must lower and verify before any LLVM/native work. Each extra
    // Program retains its own IDs; never merge or compare entities across roots.
    let mut main_program = None;
    for (root, program, entry) in programs {
        if root.id() == main {
            assert!(main_program.replace((program, entry)).is_none());
        } else {
            crate::llvm::render_verified_program(&program)
                .unwrap_or_else(|error| panic!("Range local root {root:?} LLVM: {error:?}"));
        }
    }
    Ok(main_program.expect("the original main was lowered exactly once"))
}

impl PositiveLowering {
    pub(super) fn sources(&self) -> &SourceMap {
        match &self.products {
            PositiveProducts::ExistingUnit(lowered) => &lowered.sources,
            PositiveProducts::MemoryUnit { analysis, .. } => analysis.sources(),
        }
    }

    pub(super) fn require_success(self, label: &str) -> (Program, FunctionId) {
        match self.products {
            PositiveProducts::ExistingUnit(lowered) => lowered.require_success(label),
            PositiveProducts::MemoryUnit { analysis, result } => {
                let (program, entry) = result.unwrap_or_else(|(root, error)| {
                    let fragment = error.span.and_then(|span| analysis.sources().slice(span).ok());
                    let declaration = analysis
                        .names()
                        .names()
                        .index()
                        .declarations()
                        .iter()
                        .find(|declaration| declaration.id() == root)
                        .expect("failed root belongs to the original source");
                    if root == analysis.declaration("main", SymbolKind::Function) {
                        assert_eq!(
                            error.kind,
                            LoweringErrorKind::UnsupportedNode,
                            "{label}: unexpected lowering failure: {error:?}; source={fragment:?}"
                        );
                        assert_eq!(
                            error.span,
                            Some(self.expected_span),
                            "{label}: wrong rejection span: {error:?}; source={fragment:?}"
                        );
                    }
                    // An auxiliary-root error is not relabeled as the main probe.
                    panic!(
                        "{label}: positive source root {root:?} {} must lower successfully: {error:?}; source={fragment:?}; probe={:?}",
                        declaration.name(), self.expected_span
                    );
                });
                verify_program(&program).unwrap();
                (program, entry)
            }
        }
    }
}

pub(super) enum UnitExpectation<'a> {
    Reject(&'a str),
    Success(&'a [u8]),
}

// Stage A adds one main-entry observation per original legacy label. It neither
// retires that arm nor upgrades previously unobserved auxiliary roots to success
// obligations. In particular, do not route these probes through lower_positive_roots.
pub(super) fn observe_original_unit(
    consumer: &str,
    fragment: &str,
    pipeline: Pipeline,
    legacy_sources: &SourceMap,
    legacy_probe: Span,
    expected: UnitExpectation<'_>,
    label: &str,
) {
    assert!(matches!(
        pipeline,
        Pipeline::Single | Pipeline::SingleConstant
    ));
    let label = format!("local24-stage-a-{label}-{pipeline:?}");
    let name = "range-local-control-single.ko";
    let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
    let text = format!(
        "{}\n{consumer}",
        provider.strip_prefix("package koven.algorithms\n").unwrap()
    );
    let legacy_source = legacy_probe.source_id();
    assert_eq!(legacy_sources.source_name(legacy_source).unwrap(), name);
    assert_eq!(legacy_sources.source_text(legacy_source).unwrap(), text);
    assert_eq!(
        fragment_span(legacy_sources, legacy_source, consumer, fragment),
        legacy_probe
    );
    // Read the old SourceMap directly, without cloning it or comparing its IDs
    // with a new analysis. This parse is identity evidence, not borrowed facts.
    let legacy_lexed = lang_frontend::lexer::lex(legacy_sources, legacy_source).unwrap();
    let legacy_parsed = lang_frontend::parser::parse_file(legacy_sources, &legacy_lexed).unwrap();
    assert!(legacy_parsed.diagnostics().is_empty());
    eprintln!(
        "{label}: stage=admission; source={name}; entry=main; probe={legacy_probe:?}; fragment={fragment:?}"
    );
    let mut authorized_source = None;
    let analysis =
        UnitSource::analyze_with_type_environment(name, &text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
            authorized_source = Some(source);
        })
        .unwrap_or_else(|error| {
            panic!("{label}: stage=admission, not a backend rejection: {error:?}")
        });
    analysis.assert_frontend_clean();
    let source = analysis.parsed().source_id();
    assert_eq!(authorized_source, Some(source));
    let [input] = analysis.names().names().index().source_units() else {
        panic!("{label}: one original MemoryDocument");
    };
    assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(input.source_id(), source);
    assert_eq!(analysis.sources().source_name(source).unwrap(), name);
    assert_eq!(analysis.sources().source_text(source).unwrap(), text);
    let probe = fragment_span(analysis.sources(), source, consumer, fragment);
    assert_eq!(
        (probe.start(), probe.end()),
        (legacy_probe.start(), legacy_probe.end())
    );
    let main = analysis.declaration("main", SymbolKind::Function);
    let scan = analysis.declaration("scan", SymbolKind::Function);
    for (function, declaration) in [("main", main), ("scan", scan)] {
        let old = original_function_spans(legacy_sources, &legacy_parsed, function);
        let new = original_function_spans(analysis.sources(), analysis.parsed(), function);
        assert_eq!(analysis.declaration_span(declaration), new[0]);
        let indexed = analysis
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .find(|candidate| candidate.id() == declaration)
            .unwrap();
        assert_eq!(indexed.name_span(), new[1]);
        for (old, new) in old.into_iter().zip(new) {
            assert_eq!(old.source_id(), legacy_source);
            assert_eq!(new.source_id(), source);
            assert_eq!((old.start(), old.end()), (new.start(), new.end()));
            assert_eq!(
                legacy_sources.slice(old).unwrap(),
                analysis.sources().slice(new).unwrap()
            );
        }
        if function == "scan" {
            assert!(new[2].start() <= probe.start() && probe.end() <= new[2].end());
        }
    }
    eprintln!(
        "{label}: stage=lowering; entry=main/{main:?}/{:?}; scan={scan:?}/{:?}; probe={probe:?}",
        analysis.declaration_span(main),
        analysis.declaration_span(scan)
    );
    // UnitSource chooses basic/constant only from the admitted typed products;
    // the legacy SingleConstant label does not itself confer const capability.
    let result = analysis.lower_entry(main);
    if let Err(error) = &result {
        let actual = error
            .span
            .and_then(|span| analysis.sources().slice(span).ok());
        let roots = analysis
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| analysis.parsed().roots().contains(&declaration.root()))
            .filter_map(|declaration| {
                let span = analysis.declaration_span(declaration.id());
                error
                    .span
                    .filter(|error_span| {
                        error_span.source_id() == span.source_id()
                            && span.start() <= error_span.start()
                            && error_span.end() <= span.end()
                    })
                    .map(|_| (declaration.name(), declaration.id(), span))
            })
            .collect::<Vec<_>>();
        eprintln!(
            "{label}: stage=lowering; actual_roots={roots:?}; error={error:?}; source={actual:?}"
        );
    }
    match expected {
        UnitExpectation::Reject(expected_fragment) => {
            assert_eq!(expected_fragment, fragment);
            let error = result.err().unwrap_or_else(|| {
                panic!("{label}: stage=lowering; main unexpectedly succeeded; frozen rejection retained")
            });
            assert_eq!(
                error.kind,
                LoweringErrorKind::UnsupportedNode,
                "{label}: {error:?}"
            );
            assert_eq!(error.span, Some(probe), "{label}: {error:?}");
            assert_eq!(analysis.sources().slice(probe).unwrap(), expected_fragment);
            eprintln!("{label}: outcome=exact-backend-reject; probe={probe:?}");
        }
        UnitExpectation::Success(stdout) => {
            let (program, entry) = result.unwrap_or_else(|error| {
                panic!("{label}: stage=lowering; main must succeed: {error:?}")
            });
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            assert_eq!(
                function.origin,
                Origin::Source(analysis.declaration_span(main))
            );
            verify_program(&program)
                .unwrap_or_else(|error| panic!("{label}: stage=SSA: {error:?}"));
            eprintln!("{label}: stage=LLVM/native; entry={entry:?}");
            run_unmodified(&program, entry, &label, stdout);
            eprintln!("{label}: outcome=native-success; exit=0; stderr=empty; stdout={stdout:?}");
        }
    }
}

// Declaration, name, and body spans from each real AST, used only for identity.
fn original_function_spans(
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    function: &str,
) -> [Span; 3] {
    use lang_frontend::parser::{FunctionBody, FunctionForm};
    let mut matches = Vec::new();
    for root in parsed.roots() {
        let original = parsed.ast().items().get(*root).unwrap();
        let mut node = original;
        while let Item::Modified { declaration, .. } = node.payload() {
            node = parsed.ast().items().get(*declaration).unwrap();
        }
        let Item::Function {
            name: NameMarker::Present(name),
            form,
            ..
        } = node.payload()
        else {
            continue;
        };
        if sources.slice(*name).unwrap() != function {
            continue;
        }
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => panic!("original {function} has a block body"),
        };
        matches.push([
            original.span(),
            *name,
            parsed.ast().statements().get(body).unwrap().span(),
        ]);
    }
    let [spans] = matches.as_slice() else {
        panic!("one real original top-level {function}: {matches:?}");
    };
    *spans
}

// Retain the Result and SourceMap: the initial positive-test failure must identify
// the exact rejected expression, rather than panic inside an unwrap-only harness.
pub(super) fn lower(consumer: &str, fragment: &str, pipeline: Pipeline) -> SourceLowering {
    let mut sources = SourceMap::new();
    let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
    let (expected_span, result) = match pipeline {
        Pipeline::Single | Pipeline::SingleConstant => {
            use lang_frontend::{
                name_resolution::resolve_names, ownership_checking::check_ownership,
                type_checking::check_types,
            };
            let text = format!(
                "{}\n{consumer}",
                provider.strip_prefix("package koven.algorithms\n").unwrap()
            );
            let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(
                &mut sources,
                "range-local-control-single.ko",
                &text,
            );
            let expected_span = fragment_span(&sources, source, consumer, fragment);
            let (environment, mut types) = standard_environments();
            types.authorize_range_source(&sources, source).unwrap();
            types
                .authorize_range_extension_source(&sources, source)
                .unwrap();
            let names = resolve_names(&sources, &parsed, &environment).unwrap();
            assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
            let typed = check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let entry = names
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == "main")
                .unwrap()
                .id();
            let result = crate::ssa::lower_frontend::orchestrate::lower_scalar_file_with_entry(
                &sources, &parsed, &names, &typed, &owned, entry,
            );
            (expected_span, result)
        }
        Pipeline::Unit
        | Pipeline::UnitReversed
        | Pipeline::UnitConstant
        | Pipeline::UnitConstantReversed => {
            let (provider_source, provider) =
                crate::ssa::unit_lower_test_support::parsed(&mut sources, "ranges.ko", provider);
            let text = format!("package app\nimport koven.algorithms.take\n{consumer}");
            let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(
                &mut sources,
                "range-local-control-unit.ko",
                &text,
            );
            let expected_span = fragment_span(&sources, source, consumer, fragment);
            let mut inputs = [
                SourceUnitInput::new(
                    "std",
                    "koven/algorithms/ranges.ko",
                    provider_source,
                    &provider,
                ),
                SourceUnitInput::new("app", "app/main.ko", source, &parsed),
            ];
            if matches!(
                pipeline,
                Pipeline::UnitReversed | Pipeline::UnitConstantReversed
            ) {
                inputs.reverse();
            }
            let (environment, mut types) = standard_environments();
            types
                .authorize_range_source(&sources, provider_source)
                .unwrap();
            types
                .authorize_range_extension_source(&sources, provider_source)
                .unwrap();
            let result = if matches!(
                pipeline,
                Pipeline::UnitConstant | Pipeline::UnitConstantReversed
            ) {
                use lang_frontend::{
                    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
                    ownership_checking::check_compilation_unit_constant_ownership,
                    type_checking::check_compilation_unit_types,
                };
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                let owned = check_compilation_unit_constant_ownership(
                    &sources, &inputs, &names, &types, &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                let entry = crate::ssa::unit_lower_test_support::declaration(&names, "app", "main");
                crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            } else {
                let (names, typed, owned) = crate::ssa::unit_lower_test_support::analyze(
                    &sources,
                    &inputs,
                    &environment,
                    &types,
                );
                let entry = crate::ssa::unit_lower_test_support::declaration(&names, "app", "main");
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            };
            (expected_span, result)
        }
    };
    SourceLowering {
        sources,
        expected_span,
        result,
    }
}

fn fragment_span(sources: &SourceMap, source: SourceId, consumer: &str, fragment: &str) -> Span {
    let text = sources.source_text(source).unwrap();
    assert!(text.ends_with(consumer));
    let consumer_start = text.len() - consumer.len();
    let offsets = consumer
        .match_indices(fragment)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(
        offsets.len(),
        1,
        "the source probe must be unique: {fragment}"
    );
    sources
        .span(
            source,
            consumer_start + offsets[0],
            consumer_start + offsets[0] + fragment.len(),
        )
        .unwrap()
}

impl SourceLowering {
    pub(super) fn require_success(self, label: &str) -> (Program, FunctionId) {
        if let Err(error) = self.result {
            let fragment = error.span.and_then(|span| self.sources.slice(span).ok());
            assert_eq!(
                error.kind,
                LoweringErrorKind::UnsupportedNode,
                "{label}: unexpected lowering failure: {error:?}; source={fragment:?}"
            );
            assert_eq!(
                error.span,
                Some(self.expected_span),
                "{label}: wrong rejection span: {error:?}; source={fragment:?}"
            );
            panic!(
                "{label}: positive source must lower successfully: {error:?}; source={fragment:?}"
            );
        }
        let (program, entry) = self.result.unwrap();
        verify_program(&program).unwrap();
        (program, entry)
    }
}

pub(super) fn run_unmodified(program: &Program, entry: FunctionId, label: &str, expected: &[u8]) {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!(
        "koven-range-local-control-{}-{label}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let ir = directory.join("program.ll");
    let executable = directory.join("program");
    let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{label}: {linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
    assert_eq!(run.stdout, expected, "{label}: {run:?}");
    assert!(run.stderr.is_empty(), "{label}: {run:?}");
    fs::remove_dir_all(directory).unwrap();
}
