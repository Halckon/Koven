//! Composition evidence from #428's repository fixture, not the unpublished CLI probe.
use super::*;
#[test]
fn main428_elvis_and_explicit_if_lowering_boundaries_remain_closed() {
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_ownership, d10_catalog_experiment as experiment,
            owned_compilation_unit_view,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let body = "if (other || (write(&x) && false)) { break }; return 0";
    for (label, control) in [
        (
            "repository-elvis",
            format!("left ?: if (flag) {{ {body} }} else {{ break }}"),
        ),
        (
            "local-explicit-if",
            format!("if (left == null) {{ if (flag) {{ {body} }} else {{ break }} }}"),
        ),
    ] {
        let text = format!(
            "fun write(inout target: Int?): Boolean {{ target = null; return true }}\nfun take(first: Boolean, second: Int): Boolean {{ return first }}\nfun run(left: Boolean?, flag: Boolean, other: Boolean): Int {{ var x: Int? = 7; if (x != null && (if (flag) {{ loop {{ {control}; break }}; true }} else {{ true }})) {{ return x }}; return 0 }}"
        );
        let mut sources = SourceMap::new();
        let (source, ast) =
            super::unit_lower_test_support::parsed(&mut sources, "main428.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "main428.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
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
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
            assert!(
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            let unit =
                owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
                    .unwrap();
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let error = match unit_lower::lower_owned_unit_with_entry(&unit, entry) {
                Ok(_) => panic!("{label}: unsupported boundary unexpectedly admitted"),
                Err(error) => error,
            };
            let (kind, expected_span) = if label == "repository-elvis" {
                (LoweringErrorKind::UnsupportedNode, control.as_str())
            } else {
                (LoweringErrorKind::MissingFact, "x")
            };
            assert_eq!(error.kind, kind, "{label}");
            assert_eq!(
                error.span.and_then(|span| sources.slice(span).ok()),
                Some(expected_span),
                "{label}"
            );
        }
    }
}
