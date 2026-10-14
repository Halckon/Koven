//! Source-only setup and execution for the bounded local-control cases.
use super::super::*;
use crate::ssa::{LoweringError, LoweringErrorKind};
use lang_frontend::source::{SourceId, Span};

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
