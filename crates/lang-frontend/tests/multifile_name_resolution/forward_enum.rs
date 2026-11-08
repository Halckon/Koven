//! Local enum type paths retain lexical roots across declaration order.
use super::*;
use lang_frontend::{
    name_resolution::{SymbolKind, resolve_names},
    type_checking::check_compilation_unit_types,
};

fn names_for(text: &str, rooted: bool) -> (SourceMap, CompilationUnitNames) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "forward.ko", text);
    let input = if rooted {
        SourceUnitInput::new("root", "forward.ko", source, &file)
    } else {
        SourceUnitInput::memory_document(source, &file)
    };
    let (environment, _) = standard_environments();
    let local = resolve_names(&sources, &file, &environment).expect("local names");
    let names = resolve(&sources, &[input], &environment);
    let retained = names.source_units()[0].resolution();
    assert_eq!(retained.symbols(), local.symbols());
    assert_eq!(retained.scopes(), local.scopes());
    assert_eq!(retained.enum_cases(), local.enum_cases());
    (sources, names)
}

fn assert_case_path(names: &CompilationUnitNames, text: &str, root_name: &str) {
    let source = &names.source_units()[0];
    let resolution = source.resolution();
    let declaration = names
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == root_name)
        .expect("root enum declaration");
    let root = names
        .declaration_symbol(declaration.id())
        .expect("root symbol");
    let case = resolution
        .enum_cases()
        .iter()
        .find(|case| case.root() == root.symbol())
        .expect("enum case");
    let path = format!("{root_name}.Circle");
    let mut paths = 0;
    for (start, _) in text.match_indices(&path) {
        paths += 1;
        for offset in [0, root_name.len() + 1] {
            let references = names
                .references()
                .iter()
                .filter(|reference| {
                    reference.span().start() == start + offset
                        && reference.namespace() == Some(Namespace::Type)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                references.len(),
                1,
                "one reference per type segment: {text}"
            );
            if offset == 0 {
                assert_eq!(
                    references[0].target(),
                    &UnitReferenceTarget::Declaration(declaration.id()),
                    "{text}"
                );
            } else {
                let UnitReferenceTarget::Symbol(symbol) = references[0].target() else {
                    panic!("case type symbol: {text}")
                };
                assert_eq!(symbol.symbol(), case.type_symbol());
                assert_eq!(symbol.source_unit(), source.source_unit());
            }
            let width = if offset == 0 {
                root_name.len()
            } else {
                "Circle".len()
            };
            assert_eq!(references[0].span().end(), start + offset + width);
            assert_eq!(references[0].source_unit(), source.source_unit());
            assert_eq!(references[0].span().source_id(), resolution.source_id());
        }
    }
    assert!(paths > 0);
}

#[test]
fn local_enum_type_paths_preserve_root_and_case_identity_in_both_orders() {
    let declaration = "enum class Shape { Circle(radius: Int), Point }";
    let uses = "class Holder(val value: Shape.Circle)\nfun inspect(value: Shape): Boolean = value is Shape.Circle";
    for text in [
        format!("{declaration}\n{uses}"),
        format!("{uses}\n{declaration}"),
    ] {
        for rooted in [false, true] {
            let (_, names) = names_for(&text, rooted);
            assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
            assert_case_path(&names, &text, "Shape");
            assert!(names.validate().is_ok());
        }
    }
}

#[test]
fn equally_named_cases_are_selected_by_the_bound_root() {
    let text = "fun inspect(left: Shape.Circle, right: Other.Circle): Unit\n        enum class Shape { Circle }\nenum class Other { Circle }";
    for rooted in [false, true] {
        let (_, names) = names_for(text, rooted);
        assert!(names.diagnostics().is_empty());
        assert_case_path(&names, text, "Shape");
        assert_case_path(&names, text, "Other");
    }
}

#[test]
fn type_parameter_shadowing_is_not_rebound_to_the_outer_enum() {
    let declaration = "enum class Shape { Circle }";
    let uses = "class Holder<Shape>(val value: Shape.Circle)";
    for text in [
        format!("{declaration}\n{uses}"),
        format!("{uses}\n{declaration}"),
    ] {
        for rooted in [false, true] {
            let (_, names) = names_for(&text, rooted);
            assert!(names.diagnostics().is_empty());
            let start = text.find("Shape.Circle").expect("qualified type");
            let root = names
                .references()
                .iter()
                .find(|reference| reference.span().start() == start)
                .expect("root reference");
            let UnitReferenceTarget::Symbol(symbol) = root.target() else {
                panic!("lexical type parameter")
            };
            assert_eq!(
                names.source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()[symbol.symbol().index()]
                .kind(),
                SymbolKind::TypeParameter
            );
            assert!(
                !names
                    .references()
                    .iter()
                    .any(|reference| reference.span().start() == start + "Shape.".len())
            );
        }
    }
}

#[test]
fn same_spelling_value_parameter_does_not_shadow_the_enum_type() {
    let text = "fun inspect(Shape: Int, value: Shape): Boolean = value is Shape.Circle\n        enum class Shape { Circle }";
    for rooted in [false, true] {
        let (_, names) = names_for(text, rooted);
        assert!(names.diagnostics().is_empty());
        assert_case_path(&names, text, "Shape");
    }
}

#[test]
fn missing_local_case_cannot_fall_back_to_a_same_named_top_level_type() {
    for (extra_declaration, extra_use) in [
        ("", ""),
        ("\nclass Missing", "\nfun valid(value: Missing): Unit"),
    ] {
        let declaration = format!("enum class Shape {{ Circle }}{extra_declaration}");
        let uses = format!("fun invalid(value: Shape.Missing): Unit{extra_use}");
        for text in [
            format!("{declaration}\n{uses}"),
            format!("{uses}\n{declaration}"),
        ] {
            for rooted in [false, true] {
                let (sources, names) = names_for(&text, rooted);
                assert_eq!(codes(names.diagnostics()), ["L0080"], "{text}");
                let start = text.find("Shape.Missing").expect("qualified type") + "Shape.".len();
                let diagnostic = &names.diagnostics()[0];
                assert_eq!(diagnostic.primary_span().start(), start);
                assert_eq!(sources.slice(diagnostic.primary_span()), Ok("Missing"));
                let references = names
                    .references()
                    .iter()
                    .filter(|reference| reference.span().start() == start)
                    .collect::<Vec<_>>();
                assert_eq!(references.len(), 1);
                assert_eq!(references[0].namespace(), Some(Namespace::Type));
                assert_eq!(references[0].target(), &UnitReferenceTarget::Unresolved);
                if !extra_declaration.is_empty() {
                    let bare_start =
                        text.find("value: Missing").expect("valid bare type") + "value: ".len();
                    let declaration = names
                        .index()
                        .declarations()
                        .iter()
                        .find(|declaration| declaration.name() == "Missing")
                        .expect("unrelated type");
                    let reference = names
                        .references()
                        .iter()
                        .find(|reference| reference.span().start() == bare_start)
                        .expect("bare reference is preserved");
                    assert_eq!(
                        reference.target(),
                        &UnitReferenceTarget::Declaration(declaration.id())
                    );
                }
                assert!(names.validate().is_err());
            }
        }
    }
}

#[test]
fn forward_enum_type_tests_validate_and_input_permutation_preserves_names() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "forward.ko",
        "package p\n        fun inspect(value: Shape): Boolean = value is Shape.Circle\n        enum class Shape { Circle(radius: Int), Point }",
    );
    let (other_source, other) = parsed(&mut sources, "other.ko", "package p\nclass Unrelated");
    let inputs = [
        SourceUnitInput::new("root", "p/forward.ko", source, &file),
        SourceUnitInput::new("root", "p/other.ko", other_source, &other),
    ];
    let (environment, types) = standard_environments();
    let forward = resolve(&sources, &inputs, &environment);
    let reverse = resolve(&sources, &[inputs[1], inputs[0]], &environment);
    assert_eq!(forward, reverse);
    let validated = forward.validate().expect("valid names");
    let typed =
        check_compilation_unit_types(&sources, &inputs, &validated, &types).expect("typed unit");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.validate().is_ok());
}
