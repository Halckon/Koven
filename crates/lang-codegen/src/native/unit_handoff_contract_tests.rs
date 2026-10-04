//! 普通 unit 的六输入交接合同；不把结构相等误当同一次分析。

use std::{cell::Cell, ffi::OsString, path::Path, sync::atomic::Ordering};

use lang_frontend::ownership_checking::{
    OwnedCompilationUnitView, OwnedCompilationUnitViewError, owned_compilation_unit_view,
};

use super::{
    Command, SourceMap, SourceUnitInput, TestDirectory, TypeEnvironment, UnitAnalysis,
    ValidatedCompilationUnitNames, ValidatedCompilationUnitOwnership,
    ValidatedCompilationUnitTypes, analyze_sources, assert_no_sibling_temporary,
    check_compilation_unit_ownership, check_compilation_unit_types, fs, index_compilation_unit,
    lower_scalar_unit_with_entry, resolve_compilation_unit_names, standard_environments,
};
use crate::{
    NativeObjectError, NativeObjectErrorKind, NativeUnitEntry, emit_native_owned_unit_object,
    emit_native_unit_object,
    native::NEXT_UNIT_OBJECT_TEMPORARY,
    ssa::{render_program, unit_lower::lower_owned_unit_with_entry, verify::verify_program},
};

const COUNTER_CHILD: &str = "KOVEN_UNIT_HANDOFF_COUNTER_CHILD";
const COUNTER_TEST: &str =
    "native::unit_tests::handoff_contracts::identity_rejection_never_reserves_sibling_output";

#[derive(Clone, Copy, Debug)]
enum Replacement {
    Sources,
    Inputs,
    Names,
    Environment,
    Typed,
    Owned,
}

#[derive(Clone, Copy, Debug)]
enum Pathway {
    Legacy,
    View,
}

const PATHWAYS: [Pathway; 2] = [Pathway::Legacy, Pathway::View];

#[derive(Clone, Copy)]
struct Handoff<'a> {
    sources: &'a SourceMap,
    inputs: &'a [SourceUnitInput<'a>],
    names: &'a ValidatedCompilationUnitNames,
    environment: &'a TypeEnvironment,
    typed: &'a ValidatedCompilationUnitTypes,
    owned: &'a ValidatedCompilationUnitOwnership,
}

impl<'a> Handoff<'a> {
    fn new(analysis: &'a UnitAnalysis, inputs: &'a [SourceUnitInput<'a>]) -> Self {
        Self {
            sources: &analysis.sources,
            inputs,
            names: &analysis.names,
            environment: &analysis.environment,
            typed: &analysis.typed,
            owned: &analysis.owned,
        }
    }

    fn view(self) -> Result<OwnedCompilationUnitView<'a, 'a>, OwnedCompilationUnitViewError> {
        owned_compilation_unit_view(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
        )
    }

    fn emit(
        self,
        pathway: Pathway,
        entry: NativeUnitEntry,
        output: &Path,
    ) -> Result<(), NativeObjectError> {
        match pathway {
            Pathway::Legacy => self.emit_legacy(entry, output),
            Pathway::View => {
                let view = self.view().map_err(NativeObjectError::from)?;
                emit_native_owned_unit_object(&view, entry, output)
            }
        }
    }

    fn emit_legacy(
        self,
        entry: impl Into<NativeUnitEntry>,
        output: &Path,
    ) -> Result<(), NativeObjectError> {
        emit_native_unit_object(
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

    fn verified_ssa(self, pathway: Pathway, entry: NativeUnitEntry) -> String {
        let (program, function) = match pathway {
            Pathway::Legacy => lower_scalar_unit_with_entry(
                self.sources,
                self.inputs,
                self.names,
                self.environment,
                self.typed,
                self.owned,
                entry.declaration(),
            ),
            Pathway::View => {
                let view = self.view().expect("matching handoff constructs a view");
                assert!(std::ptr::eq(view.sources(), self.sources));
                assert!(std::ptr::eq(view.inputs(), self.inputs));
                assert_eq!(view.names(), self.names);
                assert_eq!(view.types(), self.typed.types());
                assert_eq!(view.ownership(), self.owned.ownership());
                lower_owned_unit_with_entry(&view, entry.declaration())
            }
        }
        .expect("matching handoff lowers");
        verify_program(&program).expect("lowered program verifies");
        format!("{function:?}\n{}", render_program(&program))
    }
}

fn fixture() -> UnitAnalysis {
    analyze_sources(
        "package p\nfun message(): String = \"handoff-界\"",
        "package q\nfun entry(): Unit { println(p.message()) }\nfun invalid(number: Int): Unit {}",
    )
}

fn entries(analysis: &UnitAnalysis) -> [NativeUnitEntry; 2] {
    [
        analysis.declaration("q", "entry").into(),
        analysis.declaration("q", "invalid").into(),
    ]
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

fn assert_matching_control(original: Handoff<'_>, entries: [NativeUnitEntry; 2], isolated: bool) {
    let mut expected_invalid = None;
    for pathway in PATHWAYS {
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
        assert!(error.span().is_some());
        assert!(!invalid.exists());
        if let Some(expected) = &expected_invalid {
            assert_eq!(&error, expected, "{pathway:?}");
        } else {
            expected_invalid = Some(error.clone());
        }
        assert_eq!(
            error.to_string(),
            "native object InvalidEntry: entry must match its declared native process shape and return Unit"
        );
    }
}

fn assert_rejected(
    handoff: Handoff<'_>,
    entries: [NativeUnitEntry; 2],
    expected: OwnedCompilationUnitViewError,
    isolated: bool,
) {
    let before = reservation_count(isolated);
    assert_eq!(
        handoff.view().err().expect("factory rejects mismatch"),
        expected
    );
    assert_reservation_delta(before, 0);
    for pathway in PATHWAYS {
        let directory = TestDirectory::create();
        fs::write(directory.join("unrelated"), b"keep unrelated").expect("seed unrelated file");
        for entry in entries {
            for existing in [false, true] {
                let object = directory.join(if existing { "existing.o" } else { "absent.o" });
                if existing {
                    fs::write(&object, b"preserve prior object").expect("seed output");
                }
                let before_entries = directory_entries(&directory.0);
                let before = reservation_count(isolated);
                let error = handoff
                    .emit(pathway, entry, &object)
                    .expect_err("one replaced input rejects the handoff");
                assert_reservation_delta(before, 0);
                assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
                assert_eq!(error.span(), None);
                assert_eq!(error.to_string(), mismatch_display(&expected));
                if existing {
                    assert_eq!(
                        fs::read(&object).expect("old output"),
                        b"preserve prior object"
                    );
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

        // A reserve attempt would fail as Backend here, even if its temporary were cleaned up.
        let missing_parent = directory.join("missing");
        let before_entries = directory_entries(&directory.0);
        let before = reservation_count(isolated);
        let error = handoff
            .emit(pathway, entries[0], &missing_parent.join("output.o"))
            .expect_err("identity validation precedes output reservation");
        assert_reservation_delta(before, 0);
        assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
        assert_eq!(error.span(), None);
        assert_eq!(error.to_string(), mismatch_display(&expected));
        assert!(!missing_parent.exists());
        assert_eq!(directory_entries(&directory.0), before_entries);
    }
}

fn mismatch_display(expected: &OwnedCompilationUnitViewError) -> &'static str {
    match expected {
        OwnedCompilationUnitViewError::MismatchedSource => {
            "native object MismatchedAnalysis: frontend lowering failed with MismatchedSource"
        }
        OwnedCompilationUnitViewError::MismatchedAnalysis => {
            "native object MismatchedAnalysis: frontend lowering failed with MismatchedAnalysis"
        }
    }
}

fn check_replacement(replacement: Replacement, isolated: bool) {
    let analysis = fixture();
    let inputs = analysis.inputs();
    let original = Handoff::new(&analysis, &inputs);
    let entries = entries(&analysis);
    assert!(original.typed.types().is_compatible_with(
        original.sources,
        original.inputs,
        original.names,
        original.environment
    ));
    assert!(
        original
            .owned
            .ownership()
            .is_compatible_with(original.typed)
    );
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
                OwnedCompilationUnitViewError::MismatchedSource,
                isolated,
            );
        }
        Replacement::Inputs => {
            let changed = [
                SourceUnitInput::new(
                    "different-root",
                    "p/provider.ko",
                    analysis.provider_source,
                    &analysis.provider,
                ),
                inputs[1],
            ];
            let duplicate = [inputs[0], inputs[0]];
            for (inputs, expected) in [
                (
                    &changed[..],
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
                ),
                (
                    &duplicate[..],
                    OwnedCompilationUnitViewError::MismatchedSource,
                ),
                (
                    &inputs[..1],
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
                ),
            ] {
                assert_rejected(Handoff { inputs, ..original }, entries, expected, isolated);
            }
        }
        Replacement::Names => {
            let (name_environment, _) = standard_environments();
            let index = index_compilation_unit(&analysis.sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(
                &analysis.sources,
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
                OwnedCompilationUnitViewError::MismatchedAnalysis,
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
                OwnedCompilationUnitViewError::MismatchedAnalysis,
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
            .validate()
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
                assert!(!original.owned.ownership().is_compatible_with(&typed));
                assert_rejected(
                    Handoff {
                        typed: &typed,
                        ..original
                    },
                    entries,
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
                    isolated,
                );
            } else {
                // A fresh ownership pass over T0 is legal. This O1 instead comes from fresh T1.
                let owned = check_compilation_unit_ownership(
                    original.sources,
                    &inputs,
                    original.names,
                    original.environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                assert!(owned.ownership().is_compatible_with(&typed));
                assert!(!owned.ownership().is_compatible_with(original.typed));
                assert_eq!(owned, *original.owned);
                assert_rejected(
                    Handoff {
                        owned: &owned,
                        ..original
                    },
                    entries,
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
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
    let rechecked_owned = check_compilation_unit_ownership(
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
    assert!(
        rechecked_owned
            .ownership()
            .is_compatible_with(original.typed)
    );
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
            .validate()
            .unwrap();
    let fresh_owned = check_compilation_unit_ownership(
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
    // The facts match, but T0 cannot consume the independently resolved N1 identity.
    assert!(!original.typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        original.environment,
    ));
    assert!(fresh_typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        &fresh_environment,
    ));
    assert!(!fresh_typed.types().is_compatible_with(
        original.sources,
        &inputs,
        &fresh_names,
        original.environment,
    ));
    assert!(!fresh_typed.types().is_same_analysis(original.typed.types()));
    assert!(
        !fresh_owned
            .ownership()
            .is_same_analysis(original.owned.ownership())
    );
    assert!(fresh_owned.ownership().is_compatible_with(&fresh_typed));
    assert!(!fresh_owned.ownership().is_compatible_with(original.typed));
    // SourceMap/ParsedFile are borrowed, not Clone. Rebuild the equivalent input array.
    let rebuilt = analysis.inputs();
    let reversed = [rebuilt[1], rebuilt[0]];
    let entry = entries(&analysis)[0];
    let expected_ssa = original.verified_ssa(Pathway::Legacy, entry);
    let mut expected_object = None;
    for (label, handoff) in [
        ("original", original),
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
                sources: original.sources,
                inputs: &rebuilt,
                names: &names,
                environment: &environment,
                typed: &typed,
                owned: &owned,
            },
        ),
        (
            "reordered",
            Handoff {
                sources: original.sources,
                inputs: &reversed,
                names: &names,
                environment: &environment,
                typed: &typed,
                owned: &owned,
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
        for pathway in PATHWAYS {
            assert_eq!(
                handoff.verified_ssa(pathway, entry),
                expected_ssa,
                "{label}: {pathway:?}"
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
            assert_eq!(run.stdout, "handoff-界\n".as_bytes(), "{label}");
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
        let conversions = Cell::new(0);
        let entry = CountedEntry {
            entry: entries(&analysis)[0],
            conversions: &conversions,
        };
        let object = directory.join(if matching { "matching.o" } else { "mismatch.o" });
        let result = handoff.emit_legacy(entry, &object);
        assert_eq!(conversions.get(), 1, "matching={matching}");
        if matching {
            result.expect("custom entry conversion emits");
            crate::test_support::assert_native_object(&fs::read(&object).expect("native object"));
        } else {
            let error = result.expect_err("entry is converted even when identity fails");
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert_eq!(error.span(), None);
            assert_eq!(
                error.to_string(),
                mismatch_display(&OwnedCompilationUnitViewError::MismatchedSource)
            );
            assert!(!object.exists());
        }
        assert_no_sibling_temporary(&directory.0);
    }
}

#[test]
fn identity_rejection_never_reserves_sibling_output() {
    if std::env::var_os(COUNTER_CHILD).as_deref() == Some(std::ffi::OsStr::new(COUNTER_TEST)) {
        assert_eq!(NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed), 0);
        for replacement in [
            Replacement::Sources,
            Replacement::Inputs,
            Replacement::Names,
            Replacement::Environment,
            Replacement::Typed,
            Replacement::Owned,
        ] {
            check_replacement(replacement, true);
        }
        // Six controls on each of two paths reserve once; all 80 mismatches reserve zero.
        assert_eq!(NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed), 12);
        return;
    }
    // Only this exact test executes in the child, isolating the existing private global counter.
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
    let entry = entries(&analysis)[0];
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
            assert_eq!(run.stdout, "handoff-界\n".as_bytes());
            assert!(run.stderr.is_empty());
        }
    }
}
