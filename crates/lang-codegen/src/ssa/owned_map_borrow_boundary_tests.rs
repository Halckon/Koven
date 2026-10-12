//! Normal source negative controls; no malformed facts or IR.
use super::unit_lower_test_support::parsed;
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn diagnostics(source: &str, unit: bool) -> String {
    if !unit {
        let mut sources = lang_frontend::source::SourceMap::new();
        let (id, file) = parsed(&mut sources, "negative.ko", source);
        let _ = id;
        let (environment, types) = standard_environments();
        let names =
            lang_frontend::name_resolution::resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed =
            lang_frontend::type_checking::check_types(&sources, &file, &names, &types).unwrap();
        let owned =
            lang_frontend::ownership_checking::check_ownership(&sources, &file, &names, &typed)
                .unwrap();
        return format!(
            "types={:?}; ownership={:?}",
            typed.diagnostics(),
            owned.diagnostics()
        );
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
    if !typed.diagnostics().is_empty() {
        return format!("types={:?}", typed.diagnostics());
    }
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    format!("ownership={:?}", owned.diagnostics())
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
