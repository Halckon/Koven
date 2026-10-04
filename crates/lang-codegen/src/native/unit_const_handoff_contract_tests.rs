//! const unit 六输入交接合同；在已冻结的旧 native/lower oracle 上覆盖 view 路径。

use std::{cell::Cell, ffi::OsString, path::Path, sync::atomic::Ordering};

use lang_frontend::ownership_checking::{
    OwnedCompilationUnitViewError, check_compilation_unit_constant_ownership,
};

use super::super::{
    Command, SourceUnitInput, TestDirectory, assert_no_sibling_temporary,
    check_compilation_unit_types, fs, index_compilation_unit, lex, parse_file,
    resolve_compilation_unit_names, standard_environments,
};
use crate::{
    NativeObjectErrorKind, NativeUnitEntry,
    native::NEXT_UNIT_OBJECT_TEMPORARY,
    ssa::{LoweringError, LoweringErrorKind},
};

#[path = "unit_const_handoff_test_support.rs"]
mod support;
use support::{Handoff, PATHWAYS, Pathway, fixture};

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
    for pathway in PATHWAYS {
        assert_matching_control_path(pathway, original, entries, isolated);
    }
}

fn assert_matching_control_path(
    pathway: Pathway,
    original: Handoff<'_>,
    entries: [NativeUnitEntry; 2],
    isolated: bool,
) {
    let directory = TestDirectory::create();
    let object = directory.join("control.o");
    let before = reservation_count(isolated);
    original
        .emit(pathway, entries[0], &object)
        .expect("matching chain emits");
    assert_reservation_delta(before, 1);
    crate::test_support::assert_native_object(&fs::read(&object).expect("control object"));
    assert_no_sibling_temporary(&directory.0);

    let invalid = directory.join("invalid.o");
    let before = reservation_count(isolated);
    let error = original
        .emit(pathway, entries[1], &invalid)
        .expect_err("matching chain reaches entry-shape validation");
    assert_reservation_delta(before, 0);
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
    assert!(error.diagnostic().is_none());
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
    let before = reservation_count(isolated);
    let factory_error = match expected {
        LoweringErrorKind::MismatchedSource => OwnedCompilationUnitViewError::MismatchedSource,
        LoweringErrorKind::MismatchedAnalysis => OwnedCompilationUnitViewError::MismatchedAnalysis,
        _ => panic!("only identity errors belong in this oracle"),
    };
    assert_eq!(
        handoff.view().err().expect("factory rejects mismatch"),
        factory_error
    );
    assert_reservation_delta(before, 0);
    for entry in entries {
        let before = reservation_count(isolated);
        assert_eq!(
            handoff
                .lower_legacy(entry)
                .err()
                .expect("legacy lower rejects mismatch"),
            LoweringError {
                kind: expected,
                span: None
            }
        );
        assert_reservation_delta(before, 0);
    }
    for pathway in PATHWAYS {
        assert_rejected_native(pathway, handoff, entries, expected, isolated);
    }
}

fn assert_rejected_native(
    pathway: Pathway,
    handoff: Handoff<'_>,
    entries: [NativeUnitEntry; 2],
    expected: LoweringErrorKind,
    isolated: bool,
) {
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
                .emit(pathway, entry, &object)
                .expect_err("identity rejects before entry");
            assert_reservation_delta(before, 0);
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert!(error.diagnostic().is_none());
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
        .emit(pathway, entries[0], &missing_parent.join("output.o"))
        .unwrap_err();
    assert_reservation_delta(before, 0);
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert!(error.diagnostic().is_none());
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
    let expected_ssa = original.verified_ssa(Pathway::Legacy, entry);
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
        for pathway in PATHWAYS {
            assert_eq!(
                handoff.verified_ssa(pathway, entry),
                expected_ssa,
                "{label}"
            );
            let directory = TestDirectory::create();
            let object = directory.join("program.o");
            handoff.emit(pathway, entry, &object).expect(label);
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
            let result = handoff.emit_legacy(entry, &object);
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
                    assert!(error.diagnostic().is_none());
                    assert_eq!(
                        error.span(),
                        Some(
                            original.names.names().index().declarations()
                                [analysis.declaration("invalid").index()]
                            .name_span()
                        )
                    );
                    assert_eq!(
                        error.to_string(),
                        "native object InvalidEntry: entry must match its declared native process shape and return Unit"
                    );
                } else {
                    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
                    assert!(error.diagnostic().is_none());
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
        // 每维每路径正控 reserve 一次；10 负交接 × 5 native 请求 × 2 路径均 reserve 零次。
        assert_eq!(
            NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed),
            (REPLACEMENTS.len() * PATHWAYS.len()) as u64
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

#[test]
fn llvm_emission_failure_is_atomic_and_tls_recovers() {
    let analysis = fixture();
    let inputs = analysis.inputs();
    let handoff = Handoff::new(&analysis, &inputs);
    let entry = analysis.entries()[0];
    for pathway in PATHWAYS {
        for present in [false, true] {
            let directory = TestDirectory::create();
            let blocker = directory.join("regular-file");
            fs::write(&blocker, b"blocker bytes").unwrap();
            let object = directory.join("program.o");
            if present {
                fs::write(&object, b"old object bytes").unwrap();
            }
            let before = directory_entries(&directory.0);
            let guard = crate::llvm::emission_failure::Guard::new(blocker.join("object.o"));
            let error = handoff
                .emit(pathway, entry, &object)
                .expect_err("real LLVM ENOTDIR");
            assert_eq!(
                guard.calls(),
                1,
                "lowering and LLVM verification reached emission"
            );
            assert_eq!(error.kind(), NativeObjectErrorKind::Backend);
            assert!(error.to_string().contains("Not a directory"), "{error}");
            assert_eq!(fs::read(&blocker).unwrap(), b"blocker bytes");
            if present {
                assert_eq!(fs::read(&object).unwrap(), b"old object bytes");
            } else {
                assert!(!object.exists());
            }
            assert_eq!(directory_entries(&directory.0), before);
            assert_no_sibling_temporary(&directory.0);
            // Assert cleanup while both the injection guard and directory still live.
            drop(guard);
            handoff
                .emit(pathway, entry, &object)
                .expect("TLS restored: real object");
            crate::test_support::assert_native_object(&fs::read(&object).unwrap());
            let executable = directory.join("program");
            let linked = Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(executable).output().unwrap();
            assert_eq!(run.status.code(), Some(0));
            assert_eq!(run.stdout, "handoff-界\nhandoff-界\n".as_bytes());
            assert!(run.stderr.is_empty());
        }
    }
}
