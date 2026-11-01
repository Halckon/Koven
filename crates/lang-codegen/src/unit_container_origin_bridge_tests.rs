//! Real container Unit origins with synthetic backend failures; no source overflow claim.
use lang_frontend::{diagnostic::DiagnosticDetail, name_resolution::SymbolKind};

use crate::{
    NativeObjectErrorKind,
    llvm::{
        LlvmAdapterError,
        layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
    },
    native::map_backend_error,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{MapContainerKind, SequentialContainerKind, SsaTypeId, SsaTypeKind},
};

#[derive(Clone, Copy)]
enum Container {
    Sequential(SequentialContainerKind),
    Map(MapContainerKind),
}

impl Container {
    fn matches(self, ty: &SsaTypeKind) -> bool {
        match (self, ty) {
            (Self::Sequential(expected), SsaTypeKind::SequentialContainer { kind, .. }) => {
                expected == *kind
            }
            (Self::Map(expected), SsaTypeKind::MapContainer { kind, .. }) => expected == *kind,
            _ => false,
        }
    }
}

fn assert_bridge(text: &str, expected: Container, nominal: Option<&str>) {
    let unit = UnitSource::analyze("container-origin.ko", text)
        .expect("original factory source passes strict Unit admission");
    unit.assert_frontend_clean();
    let (program, entry) = unit
        .lower_entry(unit.declaration("entry", SymbolKind::Function))
        .expect("real Unit container lowers to verified SSA");
    let module = program.module(entry.module()).unwrap();
    let containers = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            expected.matches(kind).then_some(SsaTypeId {
                module: entry.module(),
                index,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(containers.len(), 1, "one exact container storage identity");
    let ty = containers[0];
    let origin = program
        .type_origin(ty)
        .expect("lowering must supply the container's own origin");
    let sources = unit.sources();
    assert_eq!(origin.primary, origin.declaration);
    assert_eq!(origin.primary.source_id(), unit.parsed().source_id());
    assert_eq!(sources.source_text(origin.primary.source_id()), Ok(text));
    assert!(!sources.slice(origin.primary).unwrap().is_empty());
    assert!(
        unit.parsed()
            .ast()
            .expressions()
            .iter()
            .any(|(_, node)| node.span() == origin.primary)
            || unit
                .parsed()
                .ast()
                .items()
                .iter()
                .any(|(_, node)| node.span() == origin.primary)
            || unit
                .parsed()
                .ast()
                .type_refs()
                .iter()
                .any(|(_, node)| node.span() == origin.primary),
        "container origin must retain a real source demand span"
    );
    if let Some(nominal) = nominal {
        let declaration = unit
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .find(|item| item.name() == nominal)
            .unwrap()
            .name_span();
        assert_eq!(sources.slice(declaration).unwrap(), nominal);
        assert_ne!(
            origin.declaration, declaration,
            "do not borrow the element's nominal label"
        );
        let SsaTypeKind::SequentialContainer { element, .. } = &module.types[ty.index()] else {
            panic!("nominal-element fixture uses List");
        };
        assert_eq!(
            program.type_origin(*element).unwrap().declaration,
            declaration
        );
    }
    let error = map_backend_error(
        sources,
        &program,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty,
            quantity: LayoutQuantity::Stride,
            failure: LayoutFailure::ExceedsTarget {
                value: u128::MAX,
                maximum: u64::MAX.into(),
            },
        }),
    );
    assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
    assert_eq!(error.span(), Some(origin.primary));
    let diagnostic = error.diagnostic().expect("container origin enables L0145");
    assert_eq!(diagnostic.code().to_string(), "L0145");
    assert_eq!(diagnostic.primary_span(), origin.primary);
    assert!(
        diagnostic
            .details()
            .iter()
            .all(|detail| !matches!(detail, DiagnosticDetail::Label(_))),
        "an intrinsic container has no separate source declaration label"
    );
}

#[test]
fn real_unit_container_origins_bridge_stride_failures_without_nominal_labels() {
    for (factory, kind) in [
        (
            "arrayOf(1)",
            Container::Sequential(SequentialContainerKind::Array),
        ),
        (
            "listOf(1)",
            Container::Sequential(SequentialContainerKind::List),
        ),
        (
            "mutableListOf(1)",
            Container::Sequential(SequentialContainerKind::MutableList),
        ),
        ("mapOf<Int, Int>()", Container::Map(MapContainerKind::Map)),
        (
            "mutableMapOf<Int, Int>()",
            Container::Map(MapContainerKind::MutableMap),
        ),
    ] {
        let text = format!("fun entry(): Unit {{ val items = {factory} }}");
        assert_bridge(&text, kind, None);
    }
}

#[test]
fn container_origin_does_not_steal_its_nominal_element_declaration() {
    assert_bridge(
        "class Resource {}\nfun entry(): Unit { val items = listOf(Resource()) }",
        Container::Sequential(SequentialContainerKind::List),
        Some("Resource"),
    );
}
