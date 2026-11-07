//! A retired checker must not publish success for an owning receiver it cannot model.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{UnitDropTarget, check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn inspect(text: &str, expected_codes: &[&str], unit_should_succeed: bool) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let (name_env, type_env) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &type_env).unwrap();
    assert!(
        typed.diagnostics().is_empty(),
        "{text}: {:?}",
        typed.diagnostics()
    );
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let codes: Vec<_> = owned
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(codes, expected_codes, "{text}");
    if expected_codes.contains(&"L0170") {
        assert!(owned.drops().is_empty());
        assert!(owned.constant_materializations().is_none());
        assert!(owned.construction_plans().is_empty());
        assert!(owned.loans().is_empty());
        assert!(owned.captures().is_empty());
        assert!(owned.formed_owned_locals().is_empty());
        for d in owned.diagnostics() {
            assert_eq!(sources.slice(d.primary_span()).unwrap(), "own");
            assert!(d.message().contains("compilation-unit"));
        }
    }
    if unit_should_succeed {
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
            .unwrap()
            .validate()
            .unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "{text}: {:?}",
            owned.deferred()
        );
        if !expected_codes.is_empty() {
            assert!(
                owned
                    .drops()
                    .iter()
                    .any(|d| matches!(d.target(), UnitDropTarget::This(_))),
                "Unit retains the actual receiver cleanup: {text}"
            );
        }
    }
}

#[test]
fn legacy_owning_move_only_receivers_are_explicitly_unsupported() {
    for body in [
        "",
        "val moved = this",
        "val moved = this; val again = this",
        "while (flag) { val moved = this }",
        "val moved = this; val n = count",
        "val moved = this; val f: () -> Int = { count }",
    ] {
        inspect(
            &format!("class R(val count: Int) {{ own fun run(flag: Boolean): Unit {{ {body} }} }}"),
            &["L0170"],
            body.is_empty(),
        );
    }
    inspect("class R { own fun noop(): Unit {} }", &["L0170"], true);
    inspect(
        "class R<T>(val item: T) { own fun noop(): Unit {} }",
        &["L0170"],
        false,
    );
    inspect(
        "value class R<T>(val item: T) { own fun noop(): Unit {} }",
        &["L0170"],
        false,
    );
}

#[test]
fn borrowed_move_diagnostic_is_not_replaced_by_the_legacy_capability_gate() {
    inspect(
        "class R {\n own fun noop(): Unit {}\n fun bad(): Unit { val moved = this }\n }",
        &["L0133"],
        false,
    );
}

#[test]
fn copyable_own_and_borrow_receivers_keep_their_existing_contracts() {
    inspect(
        "value class R(val count: Int) { own fun read(): Int = count }",
        &[],
        true,
    );
    inspect(
        "class R(val count: Int) { fun read(): Int = count }",
        &[],
        true,
    );
}
