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
    ssa::model::{
        EntityId, EntityType, LoanKind, MapContainerKind, SequentialContainerKind, SsaTypeId,
        SsaTypeKind, TypeOrigin,
    },
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

#[test]
fn view_only_unit_parameter_supplies_its_synthetic_list_layout_origin() {
    for (definition, element_name) in [("", "Int"), ("class Resource {}", "Resource")] {
        // There is no List annotation or factory to fill in a missing origin later.
        let text = format!("{definition}\nfun entry(view: View<{element_name}>): Unit {{}}");
        let unit = UnitSource::analyze("view-only-origin.ko", &text)
            .expect("strict original View parameter admission");
        unit.assert_frontend_clean();
        let declaration = unit.declaration("entry", SymbolKind::Function);
        let signature = unit
            .types()
            .signatures()
            .declaration(declaration)
            .unwrap()
            .callable()
            .unwrap();
        let [parameter] = signature.parameters() else {
            panic!("one original View parameter");
        };
        let expected = TypeOrigin {
            primary: parameter.span(),
            declaration: parameter.span(),
        };
        let sources = unit.sources();
        assert_eq!(expected.primary.source_id(), unit.parsed().source_id());
        assert_eq!(
            sources.source_text(expected.primary.source_id()),
            Ok(text.as_str())
        );
        assert!(!sources.slice(expected.primary).unwrap().is_empty());
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("real View-only Unit signature lowers to verified SSA");
        let module = program.module(entry.module()).unwrap();
        let views = module
            .types
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| match kind {
                SsaTypeKind::RangeView { source } => Some((
                    SsaTypeId {
                        module: entry.module(),
                        index,
                    },
                    *source,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(views.len(), 1, "one real parameter storage identity");
        let (view, source) = views[0];
        let function = module.function(entry).unwrap();
        let [loan @ EntityId::Loan(_)] = function.blocks[0].parameters.as_slice() else {
            panic!("View remains a Borrow parameter, not a newly owned value");
        };
        assert_eq!(
            function.entity(*loan).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: view,
            }
        );
        let Some(SsaTypeKind::SequentialContainer {
            kind: SequentialContainerKind::List,
            element,
        }) = module.type_kind(source)
        else {
            panic!("View source is its existing synthetic List storage");
        };
        assert_eq!(program.type_origin(source), Some(expected));
        assert_eq!(
            program.type_origin(view),
            None,
            "no RangeView-origin expansion"
        );
        if element_name == "Resource" {
            let nominal = unit
                .names()
                .names()
                .index()
                .declarations()
                .iter()
                .find(|item| item.name() == "Resource")
                .unwrap()
                .name_span();
            assert_eq!(program.type_origin(*element).unwrap().declaration, nominal);
            assert_ne!(nominal, expected.declaration);
        }
        let failure = |ty| {
            LlvmAdapterError::InvalidLayout(TargetLayoutError {
                ty,
                quantity: LayoutQuantity::Stride,
                failure: LayoutFailure::ExceedsTarget {
                    value: u128::MAX,
                    maximum: u64::MAX.into(),
                },
            })
        };
        let error = map_backend_error(sources, &program, failure(source));
        assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
        assert_eq!(error.span(), Some(expected.primary));
        let diagnostic = error
            .diagnostic()
            .expect("synthetic List has an L0145 origin");
        assert_eq!(diagnostic.code().to_string(), "L0145");
        assert_eq!(diagnostic.primary_span(), expected.primary);
        assert!(
            diagnostic
                .details()
                .iter()
                .all(|detail| !matches!(detail, DiagnosticDetail::Label(_))),
            "do not label the View or its element as a separate List declaration"
        );
        let unchanged = map_backend_error(sources, &program, failure(view));
        assert_eq!(unchanged.kind(), NativeObjectErrorKind::Backend);
        assert_eq!(unchanged.span(), None);
        assert!(unchanged.diagnostic().is_none());
    }
}
