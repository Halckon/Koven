//! Real admitted types exercise mapper cache order, not whole-program nullable lowering.
use lang_frontend::type_checking::UnitTypeKind;

use super::type_lower::UnitTypeLowering;
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{MapContainerKind, Program, SsaTypeKind, TypeOrigin},
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
