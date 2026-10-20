//! Real source controls for the unfinished runtime-global backend capability.

use super::parsed;
use crate::{
    NativeObjectErrorKind, emit_native_constant_unit_object, emit_native_unit_object,
    native_tests::TestDirectory,
    ssa::{
        LoweringErrorKind,
        unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    },
};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};
use std::fs;

fn checked_frontend(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
) -> (
    ValidatedCompilationUnitNames,
    TypeEnvironment,
    CompilationUnitTypes,
) {
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, inputs).expect("real source index");
    let names = resolve_compilation_unit_names(sources, inputs, &index, &name_environment)
        .expect("real source names")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(sources, inputs, &names, &environment)
        .expect("real source types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let complete = complete_compilation_unit_types(sources, inputs, &names, &environment, &typed)
        .expect("runtime globals remain legal at final frontend admission");
    assert!(complete.is_same_analysis(&typed));
    (names, environment, typed)
}

fn reject_basic_global(provider_text: &str, expected_root: &str, expected_end: usize) {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nval consumerState = 3\nfun entry(): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (names, environment, typed) = checked_frontend(&sources, &inputs);
    let typed = typed.validate().expect("ordinary basic source capability");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("real source ownership")
        .validate()
        .expect("valid ownership");
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .expect("real entry")
        .id();
    for ordered in [inputs, [inputs[1], inputs[0]]] {
        let error = lower_scalar_unit_with_entry(
            &sources,
            &ordered,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .expect("unused runtime global must not disappear during instance planning");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error.span.expect("whole source root span");
        assert_eq!(span.source_id(), provider_source);
        assert_eq!((span.start(), span.end()), (10, expected_end));
        assert_eq!(sources.slice(span).unwrap(), expected_root);
        for previous in [None, Some(b"previous object".as_slice())] {
            let directory = TestDirectory::create();
            let object = directory.join("program.o");
            if let Some(bytes) = previous {
                fs::write(&object, bytes).unwrap();
            }
            fs::write(directory.join("neighbor.txt"), b"ordinary neighbor").unwrap();
            let before = directory.snapshot();
            let error = emit_native_unit_object(
                &sources,
                &ordered,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .expect_err("runtime-global capability failure precedes object publication");
            assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
            assert_eq!(error.span(), Some(span));
            assert!(error.diagnostic().is_none(), "not a frontend language ban");
            assert_eq!(directory.snapshot(), before, "whole directory bytes");
            match previous {
                Some(bytes) => assert_eq!(fs::read(&object).unwrap(), bytes),
                None => assert!(!object.exists()),
            }
        }
    }
}

#[test]
fn runtime_global_roots_reject_in_canonical_source_order() {
    reject_basic_global(
        "package p\nval first = 1\nvar later = 2",
        "val first = 1",
        23,
    );
    reject_basic_global(
        "package p\nvar first = 1\nval later = 2",
        "var first = 1",
        23,
    );
}

#[test]
fn modified_global_initializer_calls_remain_unsupported() {
    reject_basic_global(
        "package p\nprivate val state = initialize()\nfun initialize(): Int { println(\"initializer\"); return 7 }",
        "private val state = initialize()",
        42,
    );
}

#[test]
fn constant_capability_rejects_runtime_globals_and_preserves_declarations() {
    const DECLARATIONS: &str = "package p\nconst val VALUE = 7\nobject Config { const val EXTRA = 1 }\nclass Holder(val item: Int)\ninterface Marker\nenum class Choice { One }";
    let runtime_global = format!(
        "{DECLARATIONS}\nprivate var state = initialize()\nfun initialize(): Int {{ println(\"initializer\"); return VALUE }}"
    );
    for (provider_text, unsupported) in [(DECLARATIONS, false), (runtime_global.as_str(), true)] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Unit { val number = p.VALUE + p.Config.EXTRA }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (names, environment, typed) = checked_frontend(&sources, &inputs);
        let typed = typed
            .validate_constants()
            .expect("actual constant capability");
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .expect("real constant ownership")
        .validate()
        .expect("valid constant ownership");
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .expect("real entry")
            .id();
        for ordered in [inputs, [inputs[1], inputs[0]]] {
            if unsupported {
                let error = lower_constant_unit_with_entry(
                    &sources,
                    &ordered,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                )
                .err()
                .expect("constant ownership cannot hide a runtime global");
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                let span = error.span.expect("complete modified source root span");
                assert_eq!(span.source_id(), provider_source);
                assert_eq!((span.start(), span.end()), (139, 171));
                assert_eq!(
                    sources.slice(span).unwrap(),
                    "private var state = initialize()"
                );
                for previous in [None, Some(b"previous object".as_slice())] {
                    let directory = TestDirectory::create();
                    let object = directory.join("program.o");
                    if let Some(bytes) = previous {
                        fs::write(&object, bytes).unwrap();
                    }
                    fs::write(directory.join("neighbor.txt"), b"ordinary neighbor").unwrap();
                    let before = directory.snapshot();
                    let error = emit_native_constant_unit_object(
                        &sources,
                        &ordered,
                        &names,
                        &environment,
                        &typed,
                        &owned,
                        entry,
                        &object,
                    )
                    .expect_err("constant native also rejects before object publication");
                    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
                    assert_eq!(error.span(), Some(span));
                    assert!(error.diagnostic().is_none(), "not a frontend language ban");
                    assert_eq!(directory.snapshot(), before, "whole directory bytes");
                    match previous {
                        Some(bytes) => assert_eq!(fs::read(&object).unwrap(), bytes),
                        None => assert!(!object.exists()),
                    }
                }
            } else {
                let directory = TestDirectory::create();
                let object = directory.join("program.o");
                emit_native_constant_unit_object(
                    &sources,
                    &ordered,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    &object,
                )
                .expect("constant and declarative roots remain supported");
                crate::test_support::assert_native_object(&fs::read(&object).unwrap());
                assert_eq!(
                    directory.snapshot().len(),
                    1,
                    "no sibling temporary remains"
                );
            }
        }
    }
}
