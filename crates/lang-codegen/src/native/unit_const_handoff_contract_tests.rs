//! const unit 的六输入交接 oracle；先冻结旧 native/lower 的可观察合同。

use std::{cell::Cell, ffi::OsString, path::Path, sync::atomic::Ordering};

use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstantMaterializationKind, UnitShortCircuitRhs,
        check_compilation_unit_constant_ownership,
    },
    parser::ParsedFile,
    source::SourceId,
    type_checking::{ConstEnabledTypedUnit, ConstValue},
};

use super::super::{
    Command, SourceMap, SourceUnitInput, TestDirectory, TypeEnvironment,
    ValidatedCompilationUnitNames, assert_no_sibling_temporary, check_compilation_unit_types, fs,
    index_compilation_unit, lex, parse_file, parsed, resolve_compilation_unit_names,
    standard_environments,
};
use crate::{
    NativeObjectError, NativeObjectErrorKind, NativeUnitEntry, emit_native_constant_unit_object,
    native::NEXT_UNIT_OBJECT_TEMPORARY,
    ssa::{
        LoweringError, LoweringErrorKind,
        model::{FunctionId, Program},
        render_program,
        unit_lower::constant::lower_constant_unit_with_entry,
        verify::verify_program,
    },
};

const COUNTER_CHILD: &str = "KOVEN_CONST_UNIT_HANDOFF_COUNTER_CHILD";
const COUNTER_TEST: &str = "native::unit_tests::constants::handoff_contracts::identity_rejection_never_reserves_sibling_output";

#[derive(Clone, Copy, Debug)]
enum Replacement {
    Sources,
    Inputs,
    Names,
    Environment,
    Typed,
    Owned,
}

const REPLACEMENTS: [Replacement; 6] = [
    Replacement::Sources,
    Replacement::Inputs,
    Replacement::Names,
    Replacement::Environment,
    Replacement::Typed,
    Replacement::Owned,
];

struct ConstAnalysis {
    sources: SourceMap,
    provider_source: SourceId,
    provider: ParsedFile,
    consumer_source: SourceId,
    consumer: ParsedFile,
    names: ValidatedCompilationUnitNames,
    environment: TypeEnvironment,
    typed: ConstEnabledTypedUnit,
    owned: ConstEnabledOwnedUnit,
}

impl ConstAnalysis {
    fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new(
                "root",
                "p/provider.ko",
                self.provider_source,
                &self.provider,
            ),
            SourceUnitInput::new(
                "root",
                "q/consumer.ko",
                self.consumer_source,
                &self.consumer,
            ),
        ]
    }

    fn declaration(&self, name: &str) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == name)
            .expect("fixture declaration exists")
            .id()
    }

    fn entries(&self) -> [NativeUnitEntry; 2] {
        [
            self.declaration("entry").into(),
            self.declaration("invalid").into(),
        ]
    }
}

#[derive(Clone, Copy)]
struct Handoff<'a> {
    sources: &'a SourceMap,
    inputs: &'a [SourceUnitInput<'a>],
    names: &'a ValidatedCompilationUnitNames,
    environment: &'a TypeEnvironment,
    typed: &'a ConstEnabledTypedUnit,
    owned: &'a ConstEnabledOwnedUnit,
}

impl<'a> Handoff<'a> {
    fn new(analysis: &'a ConstAnalysis, inputs: &'a [SourceUnitInput<'a>]) -> Self {
        Self {
            sources: &analysis.sources,
            inputs,
            names: &analysis.names,
            environment: &analysis.environment,
            typed: &analysis.typed,
            owned: &analysis.owned,
        }
    }

    fn emit(
        self,
        entry: impl Into<NativeUnitEntry>,
        output: &Path,
    ) -> Result<(), NativeObjectError> {
        emit_native_constant_unit_object(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
            entry,
            output,
        )
    }

    fn lower(self, entry: NativeUnitEntry) -> Result<(Program, FunctionId), LoweringError> {
        lower_constant_unit_with_entry(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
            entry.declaration(),
        )
    }

    fn verified_ssa(self, entry: NativeUnitEntry) -> String {
        let (program, function) = self.lower(entry).expect("matching const handoff lowers");
        verify_program(&program).expect("lowered const program verifies");
        format!("{function:?}\n{}", render_program(&program))
    }
}

fn fixture() -> ConstAnalysis {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nconst val TEXT = \"handoff-界\"\nconst val FLAG = true",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun probe(text: String): Boolean { println(text)\nreturn true }\n\
         fun entry(): Unit {\n\
             if (p.FLAG || probe(p.TEXT)) { println(p.TEXT) }\n\
             if (p.FLAG && probe(p.TEXT)) {}\n\
         }\n\
         fun invalid(number: Int): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid input index");
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
    assert_eq!(typed.constants().declarations().len(), 2);
    assert_eq!(typed.constants().uses().len(), 5);
    assert_eq!(owned.materializations().len(), 4);
    assert_eq!(
        owned
            .materializations()
            .iter()
            .filter(|plan| {
                plan.kind() == ConstantMaterializationKind::StringTemporary
                    && matches!(plan.descriptor().value(), ConstValue::String(_))
            })
            .count(),
        2
    );
    assert_eq!(
        owned
            .short_circuits()
            .iter()
            .map(|plan| plan.rhs())
            .collect::<Vec<_>>(),
        [UnitShortCircuitRhs::Never, UnitShortCircuitRhs::Always]
    );
    ConstAnalysis {
        sources,
        provider_source,
        provider,
        consumer_source,
        consumer,
        names,
        environment,
        typed,
        owned,
    }
}

fn directory_entries(directory: &Path) -> Vec<OsString> {
    let mut entries = fs::read_dir(directory)
        .expect("read directory")
        .map(|entry| entry.expect("directory entry").file_name())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn reservation_count(isolated: bool) -> Option<u64> {
    isolated.then(|| NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed))
}

fn assert_reservation_delta(before: Option<u64>, expected: u64) {
    if let Some(before) = before {
        assert_eq!(
            NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed) - before,
            expected,
            "isolated process reservation count"
        );
    }
}

fn mismatch_display(kind: LoweringErrorKind) -> &'static str {
    match kind {
        LoweringErrorKind::MismatchedSource => {
            "native object MismatchedAnalysis: frontend lowering failed with MismatchedSource"
        }
        LoweringErrorKind::MismatchedAnalysis => {
            "native object MismatchedAnalysis: frontend lowering failed with MismatchedAnalysis"
        }
        _ => panic!("only handoff identity failures belong in this oracle"),
    }
}

fn assert_matching_control(original: Handoff<'_>, entries: [NativeUnitEntry; 2], isolated: bool) {
    let directory = TestDirectory::create();
    let object = directory.join("control.o");
    let before = reservation_count(isolated);
    original
        .emit(entries[0], &object)
        .expect("matching chain emits");
    assert_reservation_delta(before, 1);
    crate::test_support::assert_native_object(&fs::read(&object).expect("control object"));
    assert_no_sibling_temporary(&directory.0);

    let invalid = directory.join("invalid.o");
    let before = reservation_count(isolated);
    let error = original
        .emit(entries[1], &invalid)
        .expect_err("matching chain reaches entry-shape validation");
    assert_reservation_delta(before, 0);
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
    assert_eq!(
        error.span(),
        Some(
            original.names.names().index().declarations()[entries[1].declaration().index()]
                .name_span()
        )
    );
    assert_eq!(
        error.to_string(),
        "native object InvalidEntry: entry must match its declared native process shape and return Unit"
    );
    assert!(!invalid.exists());
}

fn assert_rejected(
    handoff: Handoff<'_>,
    entries: [NativeUnitEntry; 2],
    expected: LoweringErrorKind,
    isolated: bool,
) {
    for entry in entries {
        let before = reservation_count(isolated);
        assert_eq!(
            handoff
                .lower(entry)
                .err()
                .expect("legacy lower rejects mismatch"),
            LoweringError {
                kind: expected,
                span: None
            }
        );
        assert_reservation_delta(before, 0);
    }
    let directory = TestDirectory::create();
    fs::write(directory.join("unrelated"), b"keep unrelated").unwrap();
    for entry in entries {
        for existing in [false, true] {
            let object = directory.join(if existing { "existing.o" } else { "absent.o" });
            if existing {
                fs::write(&object, b"preserve prior object").unwrap();
            }
            let before_entries = directory_entries(&directory.0);
            let before = reservation_count(isolated);
            let error = handoff
                .emit(entry, &object)
                .expect_err("identity rejects before entry");
            assert_reservation_delta(before, 0);
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert_eq!(error.span(), None);
            assert_eq!(error.to_string(), mismatch_display(expected));
            if existing {
                assert_eq!(fs::read(&object).unwrap(), b"preserve prior object");
            } else {
                assert!(!object.exists());
            }
            assert_eq!(directory_entries(&directory.0), before_entries);
            assert_eq!(
                fs::read(directory.join("unrelated")).unwrap(),
                b"keep unrelated"
            );
        }
    }
    // 即便 reserve 后立即清理，此路径仍会变成 Backend，不能伪装成没有 reserve。
    let missing_parent = directory.join("missing");
    let before_entries = directory_entries(&directory.0);
    let before = reservation_count(isolated);
    let error = handoff
        .emit(entries[0], &missing_parent.join("output.o"))
        .unwrap_err();
    assert_reservation_delta(before, 0);
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert_eq!(error.span(), None);
    assert_eq!(error.to_string(), mismatch_display(expected));
    assert!(!missing_parent.exists());
    assert_eq!(directory_entries(&directory.0), before_entries);
}

fn check_replacement(replacement: Replacement, isolated: bool) {
    let analysis = fixture();
    let inputs = analysis.inputs();
    let original = Handoff::new(&analysis, &inputs);
    let entries = analysis.entries();
    assert!(original.typed.types().is_compatible_with(
        original.sources,
        original.inputs,
        original.names,
        original.environment
    ));
    assert!(original.owned.is_compatible_with(original.typed));
    assert_matching_control(original, entries, isolated);
    match replacement {
        Replacement::Sources => {
            let foreign = fixture();
            assert_rejected(
                Handoff {
                    sources: &foreign.sources,
                    ..original
                },
                entries,
                LoweringErrorKind::MismatchedSource,
                isolated,
            );
        }
        Replacement::Inputs => {
            let changed_root = [
                SourceUnitInput::new(
                    "different-root",
                    "p/provider.ko",
                    analysis.provider_source,
                    &analysis.provider,
                ),
                inputs[1],
            ];
            let changed_path = [
                SourceUnitInput::new(
                    "root",
                    "p/renamed.ko",
                    analysis.provider_source,
                    &analysis.provider,
                ),
                inputs[1],
            ];
            let invalid_path = [
                SourceUnitInput::new(
                    "root",
                    "../provider.ko",
                    analysis.provider_source,
                    &analysis.provider,
                ),
                inputs[1],
            ];
            let duplicate = [inputs[0], inputs[0]];
            for (label, inputs, expected) in [
                (
                    "root",
                    &changed_root[..],
                    LoweringErrorKind::MismatchedAnalysis,
                ),
                (
                    "path",
                    &changed_path[..],
                    LoweringErrorKind::MismatchedAnalysis,
                ),
                (
                    "invalid-path",
                    &invalid_path[..],
                    LoweringErrorKind::MismatchedSource,
                ),
                (
                    "duplicate",
                    &duplicate[..],
                    LoweringErrorKind::MismatchedSource,
                ),
                (
                    "missing",
                    &inputs[..1],
                    LoweringErrorKind::MismatchedAnalysis,
                ),
            ] {
                assert!(
                    !original.typed.types().is_compatible_with(
                        original.sources,
                        inputs,
                        original.names,
                        original.environment
                    ),
                    "{label}"
                );
                assert_rejected(Handoff { inputs, ..original }, entries, expected, isolated);
            }
        }
        Replacement::Names => {
            let (name_environment, _) = standard_environments();
            let index = index_compilation_unit(original.sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(
                original.sources,
                &inputs,
                &index,
                &name_environment,
            )
            .unwrap()
            .validate()
            .unwrap();
            assert_eq!(names.names().index(), original.names.names().index());
            assert!(!original.typed.types().is_compatible_with(
                original.sources,
                &inputs,
                &names,
                original.environment
            ));
            assert_rejected(
                Handoff {
                    names: &names,
                    ..original
                },
                entries,
                LoweringErrorKind::MismatchedAnalysis,
                isolated,
            );
        }
        Replacement::Environment => {
            let (_, environment) = standard_environments();
            assert!(!original.typed.types().is_compatible_with(
                original.sources,
                &inputs,
                original.names,
                &environment
            ));
            assert_rejected(
                Handoff {
                    environment: &environment,
                    ..original
                },
                entries,
                LoweringErrorKind::MismatchedAnalysis,
                isolated,
            );
        }
        Replacement::Typed | Replacement::Owned => {
            let typed = check_compilation_unit_types(
                original.sources,
                &inputs,
                original.names,
                original.environment,
            )
            .unwrap()
            .validate_constants()
            .unwrap();
            assert!(typed.types().is_compatible_with(
                original.sources,
                &inputs,
                original.names,
                original.environment
            ));
            assert!(!typed.types().is_same_analysis(original.typed.types()));
            assert_eq!(typed, *original.typed);
            if matches!(replacement, Replacement::Typed) {
                assert!(!original.owned.is_compatible_with(&typed));
                assert_rejected(
                    Handoff {
                        typed: &typed,
                        ..original
                    },
                    entries,
                    LoweringErrorKind::MismatchedAnalysis,
                    isolated,
                );
            } else {
                // O1 来自 fresh T1；同 T0 重新检查 ownership 的合法性另有正例。
                let owned = check_compilation_unit_constant_ownership(
                    original.sources,
                    &inputs,
                    original.names,
                    original.environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                assert!(owned.is_compatible_with(&typed));
                assert!(!owned.is_compatible_with(original.typed));
                assert_eq!(owned, *original.owned);
                assert_rejected(
                    Handoff {
                        owned: &owned,
                        ..original
                    },
                    entries,
                    LoweringErrorKind::MismatchedAnalysis,
                    isolated,
                );
            }
        }
    }
}

#[test]
fn rejects_foreign_sources_before_output() {
    check_replacement(Replacement::Sources, false);
}

#[test]
fn rejects_changed_inputs_before_output() {
    check_replacement(Replacement::Inputs, false);
}

#[test]
fn rejects_fresh_names_before_output() {
    check_replacement(Replacement::Names, false);
}

#[test]
fn rejects_foreign_environment_before_output() {
    check_replacement(Replacement::Environment, false);
}

#[test]
fn rejects_fresh_typed_before_output() {
    check_replacement(Replacement::Typed, false);
}

#[test]
fn rejects_foreign_owned_before_output() {
    check_replacement(Replacement::Owned, false);
}

#[test]
fn accepts_cloned_chain_permuted_inputs_and_rechecked_ownership() {
    let analysis = fixture();
    let inputs = analysis.inputs();
    let original = Handoff::new(&analysis, &inputs);
    let names = analysis.names.clone();
    let environment = analysis.environment.clone();
    let typed = analysis.typed.clone();
    let owned = analysis.owned.clone();
    let rechecked_owned = check_compilation_unit_constant_ownership(
        original.sources,
        &inputs,
        original.names,
        original.environment,
        original.typed,
    )
    .unwrap()
    .validate()
    .unwrap();
    assert_eq!(names, analysis.names);
    assert_eq!(typed, analysis.typed);
    assert_eq!(owned, analysis.owned);
    assert!(typed.types().is_same_analysis(original.typed.types()));
    assert!(
        owned
            .ownership()
            .is_same_analysis(original.owned.ownership())
    );
    assert!(rechecked_owned.is_compatible_with(original.typed));
    assert!(
        !rechecked_owned
            .ownership()
            .is_same_analysis(original.owned.ownership())
    );
    assert_eq!(rechecked_owned, analysis.owned);

    let (fresh_name_environment, fresh_environment) = standard_environments();
    let fresh_names = resolve_compilation_unit_names(
        original.sources,
        &inputs,
        original.names.names().index(),
        &fresh_name_environment,
    )
    .unwrap()
    .validate()
    .unwrap();
    let fresh_typed =
        check_compilation_unit_types(original.sources, &inputs, &fresh_names, &fresh_environment)
            .unwrap()
            .validate_constants()
            .unwrap();
    let fresh_owned = check_compilation_unit_constant_ownership(
        original.sources,
        &inputs,
        &fresh_names,
        &fresh_environment,
        &fresh_typed,
    )
    .unwrap()
    .validate()
    .unwrap();
    assert_eq!(fresh_names, analysis.names);
    assert_eq!(fresh_typed, analysis.typed);
    assert_eq!(fresh_owned, analysis.owned);
    assert!(!original.typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        original.environment
    ));
    assert!(fresh_typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        &fresh_environment
    ));
    assert!(!fresh_typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        original.environment
    ));
    assert!(!fresh_typed.types().is_same_analysis(original.typed.types()));
    assert!(
        !fresh_owned
            .ownership()
            .is_same_analysis(original.owned.ownership())
    );
    assert!(fresh_owned.is_compatible_with(&fresh_typed));
    assert!(!fresh_owned.is_compatible_with(original.typed));

    let rebuilt = analysis.inputs();
    let reversed = [rebuilt[1], rebuilt[0]];
    // 同一 SourceMap 同字节重 parse 保留旧 index 合同；并未重新建立 names/typed 身份。
    let provider_lexed = lex(original.sources, analysis.provider_source).unwrap();
    let consumer_lexed = lex(original.sources, analysis.consumer_source).unwrap();
    let provider_reparsed = parse_file(original.sources, &provider_lexed).unwrap();
    let consumer_reparsed = parse_file(original.sources, &consumer_lexed).unwrap();
    assert!(provider_reparsed.diagnostics().is_empty());
    assert!(consumer_reparsed.diagnostics().is_empty());
    let reparsed_inputs = [
        SourceUnitInput::new(
            "root",
            "p/provider.ko",
            analysis.provider_source,
            &provider_reparsed,
        ),
        SourceUnitInput::new(
            "root",
            "q/consumer.ko",
            analysis.consumer_source,
            &consumer_reparsed,
        ),
    ];
    assert_eq!(
        index_compilation_unit(original.sources, &reparsed_inputs).unwrap(),
        *original.names.names().index()
    );
    assert!(original.typed.types().is_compatible_with(
        original.sources,
        &reparsed_inputs,
        original.names,
        original.environment
    ));
    let entry = analysis.entries()[0];
    let expected_ssa = original.verified_ssa(entry);
    let mut expected_object = None;
    for (label, handoff) in [
        ("original", original),
        (
            "reparsed",
            Handoff {
                inputs: &reparsed_inputs,
                ..original
            },
        ),
        (
            "names",
            Handoff {
                names: &names,
                ..original
            },
        ),
        (
            "environment",
            Handoff {
                environment: &environment,
                ..original
            },
        ),
        (
            "typed",
            Handoff {
                typed: &typed,
                ..original
            },
        ),
        (
            "owned",
            Handoff {
                owned: &owned,
                ..original
            },
        ),
        (
            "all-clones",
            Handoff {
                inputs: &rebuilt,
                names: &names,
                environment: &environment,
                typed: &typed,
                owned: &owned,
                ..original
            },
        ),
        (
            "reordered",
            Handoff {
                inputs: &reversed,
                names: &names,
                environment: &environment,
                typed: &typed,
                owned: &owned,
                ..original
            },
        ),
        (
            "rechecked-owned",
            Handoff {
                owned: &rechecked_owned,
                ..original
            },
        ),
        (
            "fresh-chain",
            Handoff {
                names: &fresh_names,
                environment: &fresh_environment,
                typed: &fresh_typed,
                owned: &fresh_owned,
                ..original
            },
        ),
    ] {
        assert_eq!(
            handoff.typed.constants(),
            original.typed.constants(),
            "{label}"
        );
        assert_eq!(
            handoff.owned.materializations(),
            original.owned.materializations(),
            "{label}"
        );
        assert_eq!(
            handoff.owned.short_circuits(),
            original.owned.short_circuits(),
            "{label}"
        );
        assert_eq!(handoff.verified_ssa(entry), expected_ssa, "{label}");
        let directory = TestDirectory::create();
        let object = directory.join("program.o");
        handoff.emit(entry, &object).expect(label);
        let bytes = fs::read(&object).expect("native object");
        crate::test_support::assert_native_object(&bytes);
        if let Some(expected) = &expected_object {
            assert_eq!(&bytes, expected, "{label}");
        } else {
            expected_object = Some(bytes);
        }
        assert_no_sibling_temporary(&directory.0);
        let executable = directory.join("program");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{label}: {linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
        assert_eq!(run.stdout, "handoff-界\nhandoff-界\n".as_bytes(), "{label}");
        assert!(run.stderr.is_empty(), "{label}: {run:?}");
    }
}

struct CountedEntry<'a> {
    entry: NativeUnitEntry,
    conversions: &'a Cell<usize>,
}

impl From<CountedEntry<'_>> for NativeUnitEntry {
    fn from(value: CountedEntry<'_>) -> Self {
        value.conversions.set(value.conversions.get() + 1);
        value.entry
    }
}

#[test]
fn legacy_entry_conversion_occurs_once_before_identity_validation() {
    let analysis = fixture();
    let inputs = analysis.inputs();
    let original = Handoff::new(&analysis, &inputs);
    let foreign = fixture();
    let directory = TestDirectory::create();
    for (handoff, matching) in [
        (original, true),
        (
            Handoff {
                sources: &foreign.sources,
                ..original
            },
            false,
        ),
    ] {
        for (entry_index, entry) in analysis.entries().into_iter().enumerate() {
            let conversions = Cell::new(0);
            let entry = CountedEntry {
                entry,
                conversions: &conversions,
            };
            let object = directory.join(&format!("{matching}-{entry_index}.o"));
            let result = handoff.emit(entry, &object);
            assert_eq!(
                conversions.get(),
                1,
                "matching={matching} entry={entry_index}"
            );
            if matching && entry_index == 0 {
                result.expect("custom entry conversion emits");
                crate::test_support::assert_native_object(&fs::read(&object).unwrap());
            } else {
                let error = result.expect_err("identity or entry shape rejects after conversion");
                if matching {
                    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
                } else {
                    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
                    assert_eq!(error.span(), None);
                    assert_eq!(
                        error.to_string(),
                        mismatch_display(LoweringErrorKind::MismatchedSource)
                    );
                }
                assert!(!object.exists());
            }
            assert_no_sibling_temporary(&directory.0);
        }
    }
}

#[test]
fn identity_rejection_never_reserves_sibling_output() {
    if std::env::var_os(COUNTER_CHILD).as_deref() == Some(std::ffi::OsStr::new(COUNTER_TEST)) {
        assert_eq!(NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed), 0);
        for replacement in REPLACEMENTS {
            check_replacement(replacement, true);
        }
        // 每个维度的正控 reserve 一次；10 个负交接 × 5 种 native 请求均 reserve 零次。
        assert_eq!(
            NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed),
            REPLACEMENTS.len() as u64
        );
        return;
    }
    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .args([COUNTER_TEST, "--exact", "--nocapture"])
        .env(COUNTER_CHILD, COUNTER_TEST)
        .output()
        .expect("isolated libtest child starts");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("libtest stdout");
    assert!(stdout.contains("1 passed; 0 failed; 0 ignored"), "{stdout}");
}
