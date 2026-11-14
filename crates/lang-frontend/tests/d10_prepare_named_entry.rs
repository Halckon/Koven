//! Unified single-run selection of existing source contracts; no driver or identity fault tests.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const BEFORE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }
fun entry() { val p = 1; val c = 2; run(p, c) }"#;
#[test]
fn prepare_preserves_real_admission_and_selects_only_closed_sources() {
    use experiment::{NamedEntryOrder, PrepareNamedEntryError};
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (text, expected) in [
        (
            include_str!("fixtures/final-type-admission-0319/r01_bogus_member.ko").to_owned(),
            4,
        ),
        (BEFORE.to_owned(), 0),
        (after, 1),
        ("fun entry() {}".to_owned(), 2),
        (BEFORE.replace("run(p, c)", "run(1, 2)"), 2),
        (
            "fun entry() { val a = \"a\"; val b = a; println(a); println(b) }".to_owned(),
            3,
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("prepare.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "prepare.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let result = experiment::prepare_named_entry(&sources, &inputs, &names, &types, typed);
            match expected {
                0 | 1 => {
                    let prepared = result.unwrap();
                    assert_eq!(
                        prepared.bound().order(),
                        if expected == 0 {
                            NamedEntryOrder::BeforeChild
                        } else {
                            NamedEntryOrder::AfterChild
                        }
                    );
                    let unit = owned_compilation_unit_view(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        prepared.typed(),
                        prepared.bound().ownership(),
                    )
                    .unwrap();
                    assert!(prepared.bound().matches(&unit));
                }
                2 => assert!(
                    matches!(result, Err(PrepareNamedEntryError::Unsupported)),
                    "unexpected result for {text}"
                ),
                3 => match result {
                    Err(PrepareNamedEntryError::OwnershipAdmission { diagnostics, .. }) => {
                        assert!(!diagnostics.is_empty())
                    }
                    Err(error) => panic!("wrong admission category: {error:?}"),
                    Ok(_) => panic!("use after move was accepted"),
                },
                4 => match result {
                    Err(PrepareNamedEntryError::FinalTypeAdmission(
                        lang_frontend::type_checking::FinalTypeAdmissionError::Diagnostics(
                            diagnostics,
                        ),
                    )) => {
                        assert_eq!(diagnostics.len(), 1);
                        assert_eq!(diagnostics[0].code().to_string(), "L0080");
                        assert_eq!(
                            sources.slice(diagnostics[0].primary_span()).unwrap(),
                            "fooBar"
                        );
                    }
                    Err(error) => panic!("lost final-type diagnostic: {error:?}"),
                    Ok(_) => panic!("deferred bogus member was accepted"),
                },
                _ => unreachable!(),
            }
        }
    }
}
