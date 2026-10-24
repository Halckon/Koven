//! Normal source negative controls; no malformed facts or IR.
use super::unit_lower_test_support::parsed;
use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_parsed_unit_names, analyze_unit_recovery},
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, TypeEnvironment,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

fn diagnostics(source: &str, unit: bool) -> String {
    if !unit {
        let mut sources = lang_frontend::source::SourceMap::new();
        let (id, file) = parsed(&mut sources, "negative.ko", source);
        let (environment, types) = standard_environments();
        let snapshot = analyze_parsed_unit_names(
            sources,
            vec![(UnitSourceDescriptor::memory_document(id), file)],
            environment,
        )
        .unwrap();
        assert!(
            snapshot.names().diagnostics().is_empty(),
            "{:?}",
            snapshot.names().diagnostics()
        );
        let names = snapshot.validated_names().unwrap();
        let inputs = snapshot.inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].source_id(), id);
        assert!(std::ptr::eq(
            inputs[0].parsed(),
            &snapshot.parsed_files()[0]
        ));
        assert_eq!(snapshot.sources().source_text(id), Ok(source));
        // Preserve the original parallel type/ownership observation even when
        // raw types reject. This public producer continues these exact parsed
        // and name products; it does not reparse or fabricate a qualification.
        let (raw, owned) = analyze_unit_recovery(&snapshot, &types).unwrap();
        observe_complete_types(snapshot.sources(), &inputs, names, &types, &raw);
        let diagnostics = format!(
            "types={:?}; ownership={:?}",
            raw.diagnostics(),
            owned.diagnostics()
        );
        match raw.validate() {
            Ok(typed) => {
                assert!(owned.is_compatible_with(&typed));
            }
            Err(raw) => {
                eprintln!(
                    "MemoryDocument basic qualification rejected: {:?}",
                    raw.diagnostics()
                );
            }
        }
        return diagnostics;
    }
    let mut sources = lang_frontend::source::SourceMap::new();
    let (provider, entry) = source.split_once("fun entry()").unwrap();
    let (p, provider) = parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{provider}"),
    );
    let (q, entry) = parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\nimport p.*\nfun entry(){entry}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &entry),
    ];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    observe_complete_types(&sources, &inputs, &names, &types, &typed);
    if !typed.diagnostics().is_empty() {
        return format!("types={:?}", typed.diagnostics());
    }
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    format!("ownership={:?}", owned.diagnostics())
}

// Observe complete admission without replacing the original raw diagnostics
// or granting a capability. Basic and complete are distinct qualifications:
// recovery diagnostics neither require complete success nor imply its failure.
fn observe_complete_types(
    sources: &lang_frontend::source::SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &lang_frontend::name_resolution::ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    raw: &CompilationUnitTypes,
) {
    assert!(raw.is_compatible_with(sources, inputs, names, environment));
    match complete_compilation_unit_types(sources, inputs, names, environment, raw) {
        Ok(complete) => {
            assert!(complete.is_same_analysis(raw));
            assert!(std::ptr::eq(complete.types(), raw));
        }
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
            eprintln!("Unit complete admission diagnostics (not the raw oracle): {diagnostics:?}");
        }
        Err(error) => panic!("original Unit complete admission failed internally: {error:?}"),
    }
}

#[test]
fn owned_map_borrow_frontend_negative_controls() {
    let prefix = "fun observe(value: String) { println(value) }\nfun entry() { var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1); ";
    for (label, source) in [
        ("get-move-only", "fun entry() { val values = mapOf<Int, String>(); val item = values.get(1) }".into()),
        ("no-proof", format!("{prefix} observe(removed) }}")),
        ("wrong-edge", format!("{prefix} if (removed == null) {{ observe(removed) }} }}")),
        ("proof-ended", format!("{prefix} if (removed != null) {{ observe(removed) }}; observe(removed) }}")),
        ("other-symbol", format!("{prefix} val other = values.remove(2); if (removed != null) {{ observe(other) }} }}")),
        ("consumed-owner", format!("{prefix} val taken = removed!!; observe(taken); val again = removed!!; observe(again) }}")),
        ("borrow-then-move", "fun conflict(first: String, own second: String) { println(first); println(second) }\nfun entry() { var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1); if (removed != null) { conflict(removed, removed) } }".into()),
        ("callee-consumes", "fun consume(own value: String) {}\nfun observe(value: String) { consume(value) }\nfun entry() {}".into()),
    ] {
        for unit in [false, true] {
            let diagnostics = diagnostics(&source, unit);
            eprintln!("{label}/unit={unit}: {diagnostics}");
            assert!(diagnostics.contains("Diagnostic {"), "must reject {label}/unit={unit}");
            let expected = match label {
                "get-move-only" => "L0136", "consumed-owner" => "L0131",
                "borrow-then-move" => "L0135", "callee-consumes" => "L0133", _ => "L0084",
            };
            assert!(diagnostics.contains(expected), "{label}/unit={unit}: expected {expected}");
        }
    }
}

#[test]
fn owned_map_borrow_unit_bounded_result_and_nested_positions() {
    let setup = "var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1);";
    for (label, source) in [
        (
            "borrow-return",
            format!(
                "fun view(value: String): borrow String from value = value\nfun entry() {{ {setup} if (removed != null) {{ borrow val item = view(removed); println(item) }} }}"
            ),
        ),
        (
            "require-key",
            format!(
                "fun key(value: String): String = value.clone()\nfun entry() {{ {setup} val other = mapOf<String, String>(); if (removed != null) {{ borrow val item = other.requireValue(key(removed)); println(item) }} }}"
            ),
        ),
        (
            "container-index",
            format!(
                "fun index(value: String): Int {{ println(value); return 0 }}\nfun entry() {{ {setup} val array = arrayOf(7); if (removed != null) {{ val selected = array[index(removed)]; if (selected == 7) {{ println(\"found\") }} }} }}"
            ),
        ),
    ] {
        let error = super::borrow_storage_contract_tests::unit(&source)
            .err()
            .expect("bounded source must reject");
        eprintln!("{label}: {error:?}; source={source}");
        assert_eq!(
            error.kind,
            super::LoweringErrorKind::UnsupportedNode,
            "{label}"
        );
        assert!(error.span.is_some(), "source-located boundary {label}");
    }
}
