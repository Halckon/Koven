//! Public non-test-library split preparation and original declaration selection.
#![cfg(feature = "d10-catalog-experiment")]
use experiment::{NamedEntryOrder, PrepareNamedEntryError};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{FinalTypeAdmissionError, check_compilation_unit_types, standard_environments},
};
const BEFORE: &str = r#"fun run(count: Int, childCount: Int) { var a = "first"; var b = "kept"; var outer = count; while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount; while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 }; println(a); println(b) }"#;
const ENTRY: &str = "fun entry() { val p = 1; val c = 2; run(p,c) }";
#[derive(Clone, Copy)]
enum Expected {
    Bound(NamedEntryOrder),
    Unsupported,
    EntryMismatch,
    Ownership,
    FinalType,
}
fn check(texts: &[(&str, &str)], selector: Option<&str>, memory: bool, expected: Expected) {
    let mut sources = SourceMap::new();
    let parsed: Vec<_> = texts
        .iter()
        .map(|(path, text)| {
            let source = sources.add_source(*path, *text).unwrap();
            parse_file(&sources, &lex(&sources, source).unwrap()).unwrap()
        })
        .collect();
    let inputs: Vec<_> = texts
        .iter()
        .zip(&parsed)
        .map(|((path, _), p)| {
            if memory {
                SourceUnitInput::memory_document(p.source_id(), p)
            } else {
                SourceUnitInput::new("root", path, p.source_id(), p)
            }
        })
        .collect();
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let result = if let Some(name) = selector {
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap();
        experiment::prepare_selected_split_named_entry(
            &sources,
            &inputs,
            &names,
            &types,
            typed,
            declaration,
        )
    } else {
        experiment::prepare_split_named_entry(&sources, &inputs, &names, &types, typed)
    };
    match expected {
        Expected::Bound(order) => {
            let prepared = result.unwrap();
            assert_eq!(prepared.bound().order(), order);
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
            assert_ne!(
                prepared.bound().entry().source_unit(),
                prepared.bound().callee().source_unit()
            );
        }
        Expected::Unsupported => {
            assert!(matches!(result, Err(PrepareNamedEntryError::Unsupported)))
        }
        Expected::EntryMismatch => {
            assert!(matches!(result, Err(PrepareNamedEntryError::EntryMismatch)))
        }
        Expected::Ownership => match result {
            Err(PrepareNamedEntryError::OwnershipAdmission { diagnostics, .. }) => {
                assert!(!diagnostics.is_empty());
                assert!(
                    diagnostics
                        .iter()
                        .all(|d| sources.slice(d.primary_span()).is_ok())
                );
            }
            Err(e) => panic!("wrong error {e:?}"),
            Ok(_) => panic!("invalid ownership admitted"),
        },
        Expected::FinalType => match result {
            Err(PrepareNamedEntryError::FinalTypeAdmission(
                FinalTypeAdmissionError::Diagnostics(d),
            )) => {
                assert_eq!(d.len(), 1);
                assert_eq!(d[0].code().to_string(), "L0080");
                assert_eq!(sources.slice(d[0].primary_span()).unwrap(), "fooBar");
            }
            Err(e) => panic!("wrong error {e:?}"),
            Ok(_) => panic!("invalid final type admitted"),
        },
    }
}
#[test]
fn split_prepare_retains_full_admission_and_original_selector() {
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (callee, order) in [
        (BEFORE, NamedEntryOrder::BeforeChild),
        (after.as_str(), NamedEntryOrder::AfterChild),
    ] {
        for callee_first in [false, true] {
            let (cp, wp) = if callee_first {
                ("a.ko", "z.ko")
            } else {
                ("z.ko", "a.ko")
            };
            for reverse in [false, true] {
                let mut texts = [(cp, callee), (wp, ENTRY)];
                if reverse {
                    texts.reverse();
                }
                check(&texts, Some("entry"), false, Expected::Bound(order));
                check(&texts, Some("run"), false, Expected::EntryMismatch);
            }
        }
    }
}
#[test]
fn split_prepare_preserves_language_errors_before_scope_and_selector() {
    {
        let memory = false;
        check(
            &[
                ("a.ko", BEFORE),
                ("z.ko", &ENTRY.replace("run(p,c)", "run(1,2)")),
            ],
            Some("run"),
            memory,
            Expected::Unsupported,
        );
        check(
            &[
                ("a.ko", BEFORE),
                ("z.ko", ENTRY),
                ("third.ko", "fun extra() {}"),
            ],
            None,
            memory,
            Expected::Unsupported,
        );
        check(
            &[
                ("a.ko", BEFORE),
                (
                    "z.ko",
                    "fun entry() { val a = \"a\"; val b = a; println(a); println(b) }",
                ),
            ],
            Some("run"),
            memory,
            Expected::Ownership,
        );
        check(
            &[(
                "bad.ko",
                include_str!("fixtures/final-type-admission-0319/r01_bogus_member.ko"),
            )],
            None,
            memory,
            Expected::FinalType,
        );
    }
}
#[test]
#[ignore = "CI-only cloned and foreign split selector identity"]
fn ci_only_split_selector_rejects_cloned_and_foreign_declarations() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let mut sources = SourceMap::new();
    let texts = [("a.ko", BEFORE), ("z.ko", ENTRY)];
    let parsed: Vec<_> = texts
        .iter()
        .map(|(p, t)| {
            let s = sources.add_source(*p, *t).unwrap();
            parse_file(&sources, &lex(&sources, s).unwrap()).unwrap()
        })
        .collect();
    let inputs: Vec<_> = texts
        .iter()
        .zip(&parsed)
        .map(|((path, _), p)| SourceUnitInput::new("root", path, p.source_id(), p))
        .collect();
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let foreign_index = index_compilation_unit(&sources, &inputs).unwrap();
    let foreign = resolve_compilation_unit_names(&sources, &inputs, &foreign_index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "entry")
        .unwrap()
        .clone();
    let other = foreign
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "entry")
        .unwrap();
    let original = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "entry")
        .unwrap();
    assert!(!std::ptr::eq(original, &entry));
    assert!(!std::ptr::eq(original, other));
    assert_eq!(
        (original.id(), original.source_unit(), original.root()),
        (other.id(), other.source_unit(), other.root())
    );
    for selector in [&entry, other] {
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        assert!(matches!(
            experiment::prepare_selected_split_named_entry(
                &sources, &inputs, &names, &types, typed, selector
            ),
            Err(PrepareNamedEntryError::ForeignEntry)
        ));
    }
}

#[test]
fn split_memory_documents_remain_rejected_by_original_source_contract() {
    let mut sources = SourceMap::new();
    let parsed: Vec<_> = [("a.ko", BEFORE), ("z.ko", ENTRY)]
        .iter()
        .map(|(path, text)| {
            let source = sources.add_source(*path, *text).unwrap();
            parse_file(&sources, &lex(&sources, source).unwrap()).unwrap()
        })
        .collect();
    let inputs: Vec<_> = parsed
        .iter()
        .map(|p| SourceUnitInput::memory_document(p.source_id(), p))
        .collect();
    assert!(matches!(index_compilation_unit(&sources, &inputs), Err(lang_frontend::name_resolution::CompilationUnitInputError::MemoryDocumentRequiresSingleSource)));
}
