//! Real stable-place origins are proofs, while the executable capability gate stays closed.
use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{LoanTarget, OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

fn checked(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_origins.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        file.diagnostics().is_empty(),
        "{text}: {:?}",
        file.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(
        names.diagnostics().is_empty(),
        "{text}: {:?}",
        names.diagnostics()
    );
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(!typed.diagnostics().is_empty());
    assert!(
        typed
            .diagnostics()
            .iter()
            .all(|d| d.code().to_string() == "L0164"),
        "{text}: {:?}",
        typed.diagnostics()
    );
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    (sources, file, names, typed, owned)
}

#[test]
fn generic_nullable_move_only_and_projection_returns_prove_the_actual_parameter() {
    for (text, returned, fields) in [
        (
            "fun <T> view(source: T): borrow T from source = source",
            "source",
            0,
        ),
        (
            "fun view(source: String?): borrow String? from source = (source)",
            "(source)",
            0,
        ),
        (
            "class Record(val text: String)\nfun view(source: Record): borrow Record from source = source",
            "source",
            0,
        ),
        (
            "class Record(val text: String)\nfun view(source: Record): borrow String from source = source.text",
            "source.text",
            1,
        ),
        (
            "fun view(aux: String, source: String): borrow String from source = source",
            "source",
            0,
        ),
        (
            "fun view(source: String): borrow String from source { return source }",
            "source",
            0,
        ),
    ] {
        let (sources, file, names, typed, owned) = checked(text);
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.deferred().is_empty());
        let facts = owned.borrow_return_origins();
        assert_eq!(facts.len(), 1, "{text}");
        let fact = &facts[0];
        assert_eq!(
            sources
                .slice(
                    file.ast()
                        .expressions()
                        .get(fact.expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            returned
        );
        assert_eq!(sources.slice(fact.declaration_span()).unwrap(), "borrow");
        let LoanTarget::Place(place) = fact.origin() else {
            panic!("stable place required");
        };
        let root = names
            .symbols()
            .iter()
            .find(|s| s.id() == place.root())
            .unwrap();
        assert_eq!(sources.slice(root.span()).unwrap(), "source");
        assert_eq!(place.fields().len(), fields);
        assert!(
            typed
                .parameter_bindings()
                .iter()
                .any(|p| p.symbol() == place.root())
        );
        assert!(
            owned.drops().is_empty(),
            "borrowed payload cannot become an owner"
        );
    }
}

#[test]
fn same_type_foreign_local_and_temporary_roots_are_rejected_with_source_labels() {
    for (text, primary) in [
        (
            "fun view(source: String, other: String): borrow String from source = other",
            "other",
        ),
        (
            "fun view(source: String): borrow String from source = \"temporary\"",
            "\"temporary\"",
        ),
        (
            "fun view(source: String): borrow String from source { val local = \"local\"; return local }",
            "local",
        ),
        (
            "class Record(val text: String)\nfun view(source: Record, other: Record): borrow String from source = other.text",
            "other.text",
        ),
    ] {
        let (sources, _, _, _, owned) = checked(text);
        let error = owned
            .diagnostics()
            .iter()
            .find(|d| d.code().to_string() == "L0162")
            .unwrap();
        assert_eq!(sources.slice(error.primary_span()).unwrap(), primary);
        assert!(
            error
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(label),
                    _ => None,
                })
                .any(|label| sources.slice(label.span()).unwrap() == "source")
        );
        assert!(owned.borrow_return_origins().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.loans().is_empty());
    }
}

#[test]
fn an_error_clears_previously_proven_return_origins_atomically() {
    let (_, _, _, _, owned) = checked(
        "fun valid(source: String): borrow String from source = source\nfun invalid(source: String, other: String): borrow String from source = other",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0162")
    );
    assert!(owned.borrow_return_origins().is_empty());
}

#[test]
fn call_conditional_inout_and_bodyless_paths_do_not_gain_continuation() {
    for text in [
        "fun view(source: String): borrow String from source",
        "fun view(inout source: String): borrow String from source = source",
        "fun view(source: String, flag: Boolean): borrow String from source = (if (flag) { source } else { source })",
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)",
        "fun view(source: String): borrow String from source = source\nfun run(source: String) { view(source) }",
    ] {
        let (_, _, _, _, owned) = checked(text);
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.borrow_return_origins().is_empty());
    }
}

#[test]
fn caller_owned_delivery_is_rejected_and_does_not_publish_partial_facts() {
    let (_, _, _, _, owned) = checked(
        "fun view(source: String): borrow String from source = source\nfun run(source: String) { val item = view(source) }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0163"),
        "{:?}",
        owned.diagnostics()
    );
    assert!(owned.borrow_return_origins().is_empty());
}

#[test]
fn lambda_returns_do_not_inherit_the_enclosing_borrow_contract() {
    let (sources, file, _, _, owned) = checked(
        "fun view(source: String): borrow String from source { val local = { \"owned lambda value\" }; return source }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.borrow_return_origins().len(), 1);
    let expression = owned.borrow_return_origins()[0].expression();
    assert_eq!(
        sources
            .slice(file.ast().expressions().get(expression).unwrap().span())
            .unwrap(),
        "source"
    );
}
