//! Private driver acceptance: no public constant capability is published by this slice.
use super::{CompilationUnitOwnership, UnitDropTarget, UnitLoanTarget, analysis};
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn analyze(text: &str) -> CompilationUnitOwnership {
    let mut sources = SourceMap::new();
    let source = sources.add_source("a.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "a/source.ko", source, &parsed)];
    let (ne, te) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
        .unwrap()
        .validate_constants()
        .unwrap();
    analysis::analyze(&sources, &inputs, &names, &te, typed.types()).unwrap()
}

#[test]
fn constant_recovery_cannot_publish_the_base_owned_capability() {
    let owned = analyze("package a\nconst val X = 7\nfun read(): Int = X");
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(
        owned.validate().is_err(),
        "constant analysis must not enter base native"
    );
}

#[test]
fn each_string_binary_read_has_its_own_temporary_drop() {
    for expression in ["S + S", "(S) + (S)", "A.S + A.S"] {
        let owned = analyze(&format!(
            "package a\nconst val S = \"s\"\nobject A {{ const val S = \"s\" }}\nfun read(): String = {expression}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert_eq!(owned.drops().len(), 2, "{expression}: {:?}", owned.drops());
        let targets = owned
            .drops()
            .iter()
            .map(|drop| drop.target())
            .collect::<Vec<_>>();
        assert!(
            targets
                .iter()
                .all(|target| matches!(target, UnitDropTarget::Temporary(_)))
        );
        assert_ne!(targets[0], targets[1], "reads cannot share an owner");
    }
}

#[test]
fn constant_borrows_and_value_deliveries_never_use_declaration_places() {
    let owned = analyze(
        "package a\nconst val S = \"s\"\nfun observe(s: String): Unit {}\nfun take(own s: String): Unit {}\nfun run(): Unit { val a = observe(S)\nval b = observe(S)\nval c = take(S)\nval d = take(S) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert_eq!(owned.loans().len(), 2);
    assert!(
        owned
            .loans()
            .iter()
            .all(|loan| matches!(loan.target(), UnitLoanTarget::Temporary(_)))
    );
    assert_eq!(owned.value_deliveries().len(), 2);
    assert!(
        owned
            .value_deliveries()
            .iter()
            .all(|delivery| delivery.place().is_none())
    );
}

#[test]
fn constants_do_not_capture_declarations_or_namespaces() {
    let owned = analyze(
        "package a\nconst val X = 7\nobject A { const val X = 7 }\nfun run(): Unit { val f = { X + A.X }\nf() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.captures().is_empty());
    assert_eq!(owned.closures().len(), 1);
}
