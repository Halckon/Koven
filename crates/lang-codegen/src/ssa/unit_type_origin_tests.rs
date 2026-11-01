//! Real Unit lowering origins; no manually populated type-origin table or native execution.
use lang_frontend::{
    name_resolution::{SourceUnitInput, SymbolKind},
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::standard_environments,
};

use super::{
    model::{Program, SsaTypeId, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

fn name(ty: &SsaTypeKind) -> Option<&str> {
    match ty {
        SsaTypeKind::Aggregate { name, .. }
        | SsaTypeKind::TaggedUnion { name, .. }
        | SsaTypeKind::HeapOwner { name, .. }
        | SsaTypeKind::SharedOwner { name, .. } => Some(name),
        _ => None,
    }
}

fn assert_real_primary(sources: &SourceMap, parsed: &ParsedFile, span: Span) {
    assert_eq!(span.source_id(), parsed.source_id());
    assert!(!sources.slice(span).expect("valid source span").is_empty());
    // The origin must be an actual AST use/declaration span, not a manufactured range.
    assert!(
        parsed
            .ast()
            .expressions()
            .iter()
            .any(|(_, node)| node.span() == span)
            || parsed
                .ast()
                .items()
                .iter()
                .any(|(_, node)| node.span() == span)
            || parsed
                .ast()
                .type_refs()
                .iter()
                .any(|(_, node)| node.span() == span),
        "primary must preserve an original AST span: {span:?}"
    );
}

#[test]
fn unit_storage_origins_preserve_real_nominal_and_wrapper_declarations() {
    for (definition, expression, storage_count) in [
        ("class Item(val n: Int)", "Item(1)", 2),
        ("value class Item(val n: Int)", "Item(1)", 1),
        ("enum class Item { Full(n: Int), Empty }", "Item.Full(1)", 3),
        ("value class Item(val n: Int)", "Box(Item(1))", 2),
        ("class Item(val n: Int)", "Rc(Item(1))", 3),
    ] {
        let text = format!("{definition}\nfun entry(): Unit {{ val item = {expression} }}");
        let unit =
            UnitSource::analyze("origins.ko", &text).expect("strict original Unit admission");
        unit.assert_frontend_clean();
        let (program, _) = unit
            .lower_entry(unit.declaration("entry", SymbolKind::Function))
            .expect("real Unit source lowers and verifies");
        let declaration = unit
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "Item")
            .expect("real nominal declaration")
            .name_span();
        assert_eq!(unit.sources().slice(declaration).unwrap(), "Item");
        let module = &program.modules[0];
        let mut count = 0;
        for (index, kind) in module.types.iter().enumerate() {
            let Some(storage_name) = name(kind) else {
                continue;
            };
            let ty = SsaTypeId {
                module: module.id,
                index,
            };
            let origin = program
                .type_origin(ty)
                .expect("every lowered storage type has an origin");
            assert_real_primary(unit.sources(), unit.parsed(), origin.primary);
            let expected = if storage_name.contains(".case0.payload") {
                let start = text.find("Full(n:").unwrap();
                unit.sources()
                    .span(unit.parsed().source_id(), start, start + 4)
                    .unwrap()
            } else if storage_name.contains(".case1.payload") {
                let start = text.find("Empty }").unwrap();
                unit.sources()
                    .span(unit.parsed().source_id(), start, start + 5)
                    .unwrap()
            } else {
                declaration
            };
            assert_eq!(origin.declaration, expected, "{storage_name}");
            if let SsaTypeKind::HeapOwner {
                payload: Some(payload),
                ..
            } = kind
                && storage_name.starts_with("class#")
            {
                assert_eq!(program.type_origin(*payload), Some(origin));
            }
            count += 1;
        }
        assert_eq!(count, storage_count, "{text}");
    }
}

fn origins(program: &Program) -> Vec<(String, Span, Span)> {
    let module = &program.modules[0];
    module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            let name = name(kind)?;
            let origin = program
                .type_origin(SsaTypeId {
                    module: module.id,
                    index,
                })
                .expect("real nominal origin");
            Some((name.to_owned(), origin.primary, origin.declaration))
        })
        .collect()
}

#[test]
fn same_named_classes_keep_source_qualified_origins_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(
        &mut sources,
        "left/origins.ko",
        "package left\nclass Item(val n: Int)\nfun create(): Item = Item(1)",
    );
    let (right_id, right) = parsed(
        &mut sources,
        "right/origins.ko",
        "package right\nimport left.create\nclass Item(val n: Int)\nfun entry(): Unit { val a = create(); val b = Item(2) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "left/origins.ko", left_id, &left),
        SourceUnitInput::new("root", "right/origins.ko", right_id, &right),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (ne, te) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
    let entry = declaration(&names, "right", "entry");
    let (forward, _) =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
            .expect("two source same-name classes lower");
    let (backward, _) =
        lower_scalar_unit_with_entry(&sources, &reversed, &names, &te, &typed, &owned, entry)
            .expect("input permutation lowers");
    assert_eq!(render_program(&forward), render_program(&backward));
    assert_eq!(origins(&forward), origins(&backward));
    let module = &forward.modules[0];
    let mut owners = 0;
    for package in ["left", "right"] {
        let id = declaration(&names, package, "Item");
        let expected = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|candidate| candidate.id() == id)
            .unwrap()
            .name_span();
        assert_eq!(sources.slice(expected).unwrap(), "Item");
        assert_eq!(
            expected.source_id(),
            if package == "left" { left_id } else { right_id }
        );
        let prefix = format!("class#d{}.", id.index());
        let matching = module.types.iter().enumerate().filter(|(_, kind)| {
            matches!(kind, SsaTypeKind::HeapOwner { name, .. } if name.starts_with(&prefix))
        }).collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        let (
            index,
            SsaTypeKind::HeapOwner {
                payload: Some(payload),
                ..
            },
        ) = matching[0]
        else {
            panic!("defined class storage")
        };
        let origin = forward
            .type_origin(SsaTypeId {
                module: module.id,
                index,
            })
            .unwrap();
        assert_eq!(origin.declaration, expected);
        assert_eq!(forward.type_origin(*payload), Some(origin));
        let source_file = if origin.primary.source_id() == left_id {
            &left
        } else {
            &right
        };
        assert_real_primary(&sources, source_file, origin.primary);
        owners += 1;
    }
    assert_eq!(owners, 2);
}
