//! Guide v0.46 (SPEC-0343): top-level runtime values are rejected in Phase 2 with L0167,
//! so they never reach ownership or native code. Declarative roots keep compiling, and the
//! backend `UnsupportedSource` runtime-global gate stays only as a defensive assertion.

use super::parsed;
use crate::{emit_native_constant_unit_object, native_tests::TestDirectory};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types, standard_environments,
    },
};
use std::fs;

fn raw_frontend(
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
    (names, environment, typed)
}

/// Every runtime top-level value reports exactly one L0167 on its keyword, independent of
/// input order, and the unit cannot qualify for basic or constant native lowering.
fn reject_runtime_globals(provider_text: &str, expected: &[(&str, usize, &str)]) {
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
    let mut expected = expected
        .iter()
        .map(|&(file, start, keyword)| (file.to_owned(), start, start + keyword.len()))
        .chain([("q/consumer.ko".to_owned(), 10, 13)])
        .collect::<Vec<_>>();
    expected.sort();
    for ordered in [inputs, [inputs[1], inputs[0]]] {
        let (_, _, typed) = raw_frontend(&sources, &ordered);
        let mut actual = typed
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                assert_eq!(diagnostic.code().to_string(), "L0167");
                let span = diagnostic.primary_span();
                let keyword = sources.slice(span).unwrap();
                assert!(keyword == "val" || keyword == "var", "{keyword}");
                (
                    sources.source_name(span.source_id()).unwrap().to_owned(),
                    span.start(),
                    span.end(),
                )
            })
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, expected);
        assert!(typed.clone().validate().is_err());
        assert!(typed.validate_constants().is_err());
    }
}

#[test]
fn runtime_global_roots_reject_in_canonical_source_order() {
    reject_runtime_globals(
        "package p\nval first = 1\nvar later = 2",
        &[("p/provider.ko", 10, "val"), ("p/provider.ko", 24, "var")],
    );
    reject_runtime_globals(
        "package p\nvar first = 1\nval later = 2",
        &[("p/provider.ko", 10, "var"), ("p/provider.ko", 24, "val")],
    );
}

#[test]
fn modified_global_initializer_calls_remain_unsupported() {
    reject_runtime_globals(
        "package p\nprivate val state = initialize()\nfun initialize(): Int { println(\"initializer\"); return 7 }",
        &[("p/provider.ko", 18, "val")],
    );
}

#[test]
fn constant_capability_rejects_runtime_globals_and_preserves_declarations() {
    const DECLARATIONS: &str = "package p\nconst val VALUE = 7\nobject Config { const val EXTRA = 1 }\nclass Holder(val item: Int)\ninterface Marker\nenum class Choice { One }";
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", DECLARATIONS);
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val number = p.VALUE + p.Config.EXTRA }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    for ordered in [inputs, [inputs[1], inputs[0]]] {
        let (names, environment, typed) = raw_frontend(&sources, &ordered);
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed
            .validate_constants()
            .expect("actual constant capability");
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &ordered,
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

    let runtime_global = format!(
        "{DECLARATIONS}\nprivate var state = initialize()\nfun initialize(): Int {{ println(\"initializer\"); return VALUE }}"
    );
    let start = runtime_global.find("var state").unwrap();
    reject_runtime_globals(&runtime_global, &[("p/provider.ko", start, "var")]);
}
