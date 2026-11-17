//! Temporary parents do not permit implicit moves of non-Copyable fields.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, check_compilation_unit_constant_ownership,
        check_compilation_unit_ownership,
    },
    source::{SourceId, SourceMap},
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
#[path = "ownership_temporary_field_consumption/source_identity.rs"]
mod source_identity;

fn observe(
    text: &str,
    constants: bool,
    rooted: bool,
) -> (SourceMap, SourceId, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("fields.ko", text).unwrap();
    let file = parser_test_assertions::parse_file_twice(&sources, source, "temporary field");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let input = if rooted {
        SourceUnitInput::new("root", "fields.ko", source, &file)
    } else {
        SourceUnitInput::memory_document(source, &file)
    };
    let inputs = [input];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = if constants {
        let typed = typed.validate_constants().unwrap();
        let recovery =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
        assert!(recovery.is_compatible_with(&typed));
        let owned = recovery.ownership().clone();
        if owned.diagnostics().is_empty() {
            assert!(recovery.validate().is_ok());
        } else {
            assert!(recovery.materializations().is_none());
            assert!(recovery.validate().is_err());
        }
        owned
    } else {
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        if owned.diagnostics().is_empty() {
            assert!(owned.clone().validate().is_ok());
        } else {
            assert!(owned.clone().validate().is_err());
        }
        owned
    };
    (sources, source, owned)
}

#[test]
fn temporary_string_field_consumption_is_rejected() {
    for body in [
        "fun use(): String = Holder(VALUE).text",
        "fun use(): String = (Holder(VALUE)).text",
        "fun use(): String { return Holder(VALUE).text }",
        "fun use(): Unit { val result = Holder(VALUE).text }",
        "fun take(own item: String): Unit {}\nfun use(): Unit { take(Holder(VALUE).text) }",
    ] {
        for constants in [false, true] {
            let text = format!(
                "{}class Holder(val text: String)\n{}\n",
                if constants {
                    "const val TEXT = \"hi\"\n"
                } else {
                    ""
                },
                body.replace("VALUE", if constants { "TEXT" } else { "\"hi\"" })
            );
            for rooted in [false, true] {
                let (sources, source, owned) = observe(&text, constants, rooted);
                let start = text.rfind(".text").unwrap() + 1;
                let field = text.find("text:").unwrap();
                let catalog = DiagnosticCodeCatalog::try_new(&["L0132"]).unwrap();
                let mut expected = Diagnostic::new(
                    &sources,
                    Severity::Error,
                    catalog.resolve("L0132").unwrap(),
                    "cannot move a non-Copyable component out of its owner",
                    sources.span(source, start, start + 4).unwrap(),
                )
                .unwrap();
                expected
                    .add_label(
                        &sources,
                        sources.span(source, field, field + 4).unwrap(),
                        "non-Copyable component declared here",
                    )
                    .unwrap();
                if let Some(parameter) = text.find("own item: String") {
                    expected
                        .add_label(
                            &sources,
                            sources
                                .span(source, parameter, parameter + "own item: String".len())
                                .unwrap(),
                            "selected parameter declared here",
                        )
                        .unwrap();
                }
                assert_eq!(owned.diagnostics(), [expected], "{text}; rooted={rooted}");
                assert!(owned.drops().is_empty());
                assert!(owned.loans().is_empty());
            }
        }
    }
}

#[test]
fn temporary_field_borrow_and_copy_remain_valid() {
    for text in [
        "class Holder(val text: String)\nfun view(text: String): Unit {}\nfun use(): Unit { view(Holder(\"hi\").text) }",
        "class Holder(val value: Int)\nfun use(): Int = Holder(7).value",
        "class Holder(val text: String)\nfun stop(): Nothing = stop()\nfun use(): String = Holder(stop()).text",
    ] {
        for constants in [false, true] {
            for rooted in [false, true] {
                let (_, _, owned) = observe(text, constants, rooted);
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            }
        }
    }
}

#[test]
fn erroneous_receiver_does_not_add_a_field_move_diagnostic() {
    let text = "class Holder(val text: String)\nfun make(own text: String): Holder = Holder(text)\nfun use(own text: String): String { val first = make(text); return make(text).text }";
    for constants in [false, true] {
        for rooted in [false, true] {
            let (_, _, owned) = observe(text, constants, rooted);
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0131"]
            );
            assert!(owned.drops().is_empty());
        }
    }
}
