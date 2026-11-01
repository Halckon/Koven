//! Real admitted types exercise mapper cache order, not whole-program nullable lowering.
use lang_frontend::{
    name_resolution::SymbolKind,
    source::Span,
    type_checking::{IntrinsicTypeConstructor, UnitTypeId, UnitTypeKind},
};

use super::type_lower::UnitTypeLowering;
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        MapContainerKind, Module, Program, SequentialContainerKind, SsaTypeId, SsaTypeKind,
        TypeOrigin,
    },
};

#[test]
fn nullable_first_map_origin_survives_inner_cache_hits_and_repeated_requests() {
    for (name, factory, expected_kind) in [
        ("Map", "mapOf", MapContainerKind::Map),
        ("MutableMap", "mutableMapOf", MapContainerKind::MutableMap),
    ] {
        let call = format!("{factory}<Int, Resource>()");
        let text = format!(
            "class Resource {{}}\nfun entry(): Unit {{\n  val absent: {name}<Int, Resource>? = null\n  val later = {call}\n}}"
        );
        let unit = UnitSource::analyze("map-origin-cache.ko", &text).unwrap();
        unit.assert_frontend_clean();
        let typed = unit.types();
        let nullable = typed
            .type_ref_types()
            .iter()
            .filter_map(|(reference, &ty)| match typed.types().get(ty) {
                Some(UnitTypeKind::Nullable(inner)) => Some((*reference, ty, *inner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(nullable.len(), 1, "one original nullable annotation");
        let (reference, nullable_ty, inner_ty) = nullable[0];
        let first = unit
            .parsed()
            .ast()
            .type_refs()
            .get(reference.type_ref())
            .unwrap()
            .span();
        assert_eq!(
            unit.sources().slice(first).unwrap(),
            format!("{name}<Int, Resource>?")
        );
        let later = unit
            .parsed()
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| {
                (unit.sources().slice(node.span()).unwrap() == call).then_some(node.span())
            })
            .collect::<Vec<_>>();
        assert_eq!(later.len(), 1, "one original factory call");
        let later = later[0];
        assert_ne!(first, later);

        let mut program = Program::default();
        let module_id = program.add_module("map-origin-cache");
        let module = program.module_mut(module_id).unwrap();
        let mut lowering = UnitTypeLowering::new(unit.names());
        assert!(!lowering.type_ids().contains_key(&inner_ty));
        let wrapper = lowering.intern(module, typed, nullable_ty, first).unwrap();
        let inner = lowering.type_ids()[&inner_ty];
        assert_eq!(module.nullable_map_inner(wrapper), Some(inner));
        let expected = TypeOrigin {
            primary: first,
            declaration: first,
        };
        assert_eq!(module.type_origin(inner), Some(expected));
        assert_eq!(
            module.type_origin(wrapper),
            None,
            "no new wrapper-origin contract"
        );
        let Some(SsaTypeKind::MapContainer { kind, value, .. }) = module.type_kind(inner) else {
            panic!("real nullable annotation must map to MapContainer");
        };
        assert_eq!(*kind, expected_kind);
        let value = *value;
        let nominal_origin = module.type_origin(value).unwrap();
        let declaration = unit
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "Resource")
            .unwrap()
            .name_span();
        assert_eq!(nominal_origin.declaration, declaration);
        assert_ne!(nominal_origin.declaration, expected.declaration);
        let count = module.types.len();

        // The direct inner request is a cache hit; nullable requests re-enter intern_map.
        assert_eq!(
            lowering.intern(module, typed, inner_ty, later).unwrap(),
            inner
        );
        assert_eq!(
            lowering.intern(module, typed, nullable_ty, later).unwrap(),
            wrapper
        );
        assert_eq!(
            lowering.intern(module, typed, inner_ty, later).unwrap(),
            inner
        );
        assert_eq!(module.types.len(), count, "no duplicate storage identities");
        assert_eq!(
            module.type_origin(inner),
            Some(expected),
            "first demand wins"
        );
        assert_eq!(
            module.type_origin(value),
            Some(nominal_origin),
            "nominal origin is untouched"
        );
        assert_eq!(module.type_origin(wrapper), None);
    }
}

fn parameter_demand(unit: &UnitSource, name: &str) -> (UnitTypeId, Span) {
    let signature = unit
        .types()
        .signatures()
        .declaration(unit.declaration(name, SymbolKind::Function))
        .unwrap()
        .callable()
        .unwrap();
    let [parameter] = signature.parameters() else {
        panic!("one original parameter demand");
    };
    let span = parameter.span();
    assert_eq!(span.source_id(), unit.parsed().source_id());
    assert!(!unit.sources().slice(span).unwrap().is_empty());
    (parameter.ty(), span)
}

fn view_source(module: &Module, view: SsaTypeId) -> SsaTypeId {
    let Some(SsaTypeKind::RangeView { source }) = module.type_kind(view) else {
        panic!("View maps to its existing RangeView identity");
    };
    *source
}

#[test]
fn view_and_list_demands_preserve_first_origin_and_shared_storage_identity() {
    for (definition, element_name) in [("", "Int"), ("class Resource {}", "Resource")] {
        let text = format!(
            "{definition}\nfun view(input: View<{element_name}>): Unit {{}}\n\
             fun list(input: List<{element_name}>): Unit {{}}\n\
             fun viewAgain(other: View<{element_name}>): Unit {{}}\n\
             fun listAgain(other: List<{element_name}>): Unit {{}}"
        );
        let unit = UnitSource::analyze("view-origin-cache.ko", &text).unwrap();
        unit.assert_frontend_clean();
        let typed = unit.types();
        let (view_ty, view_span) = parameter_demand(&unit, "view");
        let (list_ty, list_span) = parameter_demand(&unit, "list");
        let (view_again_ty, view_again_span) = parameter_demand(&unit, "viewAgain");
        let (list_again_ty, list_again_span) = parameter_demand(&unit, "listAgain");
        assert_eq!(view_again_ty, view_ty);
        assert_eq!(list_again_ty, list_ty);
        assert_ne!(view_span, list_span);
        assert_ne!(view_span, view_again_span);
        assert_ne!(list_span, list_again_span);
        assert!(matches!(
            typed.types().get(view_ty),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                ..
            })
        ));
        assert!(matches!(
            typed.types().get(list_ty),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                ..
            })
        ));

        for view_first in [true, false] {
            let mut program = Program::default();
            let module_id = program.add_module("view-origin-cache");
            let module = program.module_mut(module_id).unwrap();
            let mut lowering = UnitTypeLowering::new(unit.names());
            let (first_ty, first_span, second_ty, second_span) = if view_first {
                (view_ty, view_span, list_ty, list_span)
            } else {
                (list_ty, list_span, view_ty, view_span)
            };
            let first = lowering
                .intern(module, typed, first_ty, first_span)
                .unwrap();
            let source = if view_first {
                view_source(module, first)
            } else {
                first
            };
            let expected = TypeOrigin {
                primary: first_span,
                declaration: first_span,
            };
            // Observe View-only creation before a later List request could fill the gap.
            assert_eq!(module.type_origin(source), Some(expected));
            let Some(SsaTypeKind::SequentialContainer {
                kind: SequentialContainerKind::List,
                element,
            }) = module.type_kind(source)
            else {
                panic!("View uses the same List storage as a direct List demand");
            };
            let element = *element;
            let element_origin = module.type_origin(element);
            if element_name == "Resource" {
                let declaration = unit
                    .names()
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|item| item.name() == "Resource")
                    .unwrap()
                    .name_span();
                assert_eq!(element_origin.unwrap().declaration, declaration);
                assert_ne!(declaration, first_span);
            } else {
                assert_eq!(element_origin, None, "no new primitive-origin contract");
            }
            let first_count = module.types.len();
            let second = lowering
                .intern(module, typed, second_ty, second_span)
                .unwrap();
            let view = if view_first {
                assert_eq!(second, source);
                first
            } else {
                assert_eq!(view_source(module, second), source);
                second
            };
            assert_eq!(
                module.types.len(),
                first_count + usize::from(!view_first),
                "only a first View request adds its RangeView, never another List"
            );
            let count = module.types.len();
            assert_eq!(
                lowering
                    .intern(module, typed, view_again_ty, view_again_span)
                    .unwrap(),
                view
            );
            assert_eq!(
                lowering
                    .intern(module, typed, list_again_ty, list_again_span)
                    .unwrap(),
                source
            );
            assert_eq!(module.types.len(), count);
            assert_eq!(module.type_origin(source), Some(expected));
            assert_eq!(module.type_origin(element), element_origin);
            assert_eq!(module.type_origin(view), None, "RangeView is unchanged");
        }
    }
}
