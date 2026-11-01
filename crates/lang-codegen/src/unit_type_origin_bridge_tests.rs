//! Exercise diagnostic mapping with origins produced by real Unit lowering.
//! The backend layout failure is synthetic; this does not claim source overflow.
use lang_frontend::{diagnostic::DiagnosticDetail, name_resolution::SymbolKind};

use crate::{
    NativeObjectErrorKind,
    llvm::{
        LlvmAdapterError,
        layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
    },
    native::map_backend_error,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{SsaTypeId, SsaTypeKind},
};

#[test]
fn real_unit_type_origin_bridges_synthetic_layout_failure_to_l0145() {
    let text = "value class Huge(val item: Int)\nfun bootstrap(): Unit { Huge(1) }";
    let analysis = UnitSource::analyze("unit-layout-origin.ko", text).unwrap();
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("bootstrap", SymbolKind::Function))
        .expect("original source lowers through Unit");
    let module = program.module(entry.module()).unwrap();
    let aggregates = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            matches!(kind, SsaTypeKind::Aggregate { name, .. } if name.starts_with("value#"))
                .then_some(SsaTypeId {
                    module: entry.module(),
                    index,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(aggregates.len(), 1, "exact original value-class identity");
    let aggregate = aggregates[0];
    let origin = program
        .type_origin(aggregate)
        .expect("Unit lowering, not this test, must publish the type origin");
    let sources = analysis.sources();
    assert_eq!(sources.slice(origin.declaration).unwrap(), "Huge");
    assert_eq!(origin.declaration.start(), 12);
    assert_eq!(origin.declaration.end(), 16);
    assert_eq!(origin.primary.source_id(), origin.declaration.source_id());
    assert_eq!(
        sources.source_name(origin.primary.source_id()).unwrap(),
        "unit-layout-origin.ko"
    );
    assert!(!sources.slice(origin.primary).unwrap().is_empty());
    assert_ne!(
        origin.primary, origin.declaration,
        "retain distinct use and declaration spans"
    );
    let error = map_backend_error(
        sources,
        &program,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty: aggregate,
            quantity: LayoutQuantity::Size,
            failure: LayoutFailure::ExceedsTarget {
                value: u128::MAX,
                maximum: u64::MAX.into(),
            },
        }),
    );
    assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
    assert_eq!(error.span(), Some(origin.primary));
    let diagnostic = error.diagnostic().expect("real Unit origin enables L0145");
    assert_eq!(diagnostic.code().to_string(), "L0145");
    assert_eq!(diagnostic.primary_span(), origin.primary);
    let labels = diagnostic
        .details()
        .iter()
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label.span()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, vec![origin.declaration]);
}
