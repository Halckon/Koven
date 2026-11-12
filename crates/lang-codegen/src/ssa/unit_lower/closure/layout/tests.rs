//! Synthetic invalid SSA identities only run in official CI; old witnesses remain.
use super::shared_capture_storage;
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Program, SsaTypeId, SsaTypeKind},
};
use lang_frontend::source::SourceMap;

#[test]
fn checked_shared_capture_storage_preserves_local_type_and_interning() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("capture.ko", "{ owner }").unwrap();
    let span = sources.span(source, 0, 9).unwrap();
    let mut program = Program::default();
    let module_id = program.add_module("capture");
    let module = program.module_mut(module_id).unwrap();
    let target = module.intern_type(SsaTypeKind::Boolean);
    let storage = shared_capture_storage(module, target, span).unwrap();
    assert_eq!(
        module.type_kind(storage),
        Some(&SsaTypeKind::SharedReference { target })
    );
    assert_eq!(storage.module(), module_id);
    assert_eq!(
        shared_capture_storage(module, target, span).unwrap(),
        storage
    );
    assert_eq!(module.types.len(), 2);
}

#[test]
fn invalid_shared_capture_identity_maps_to_exact_lowering_error_without_mutation() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("capture.ko", "prefix { owner }")
        .unwrap();
    let span = sources.span(source, 7, 16).unwrap();
    let mut program = Program::default();
    let module_id = program.add_module("capture");
    let foreign_module_id = program.add_module("foreign");
    let foreign_module = program
        .module_mut(foreign_module_id)
        .unwrap()
        .intern_type(SsaTypeKind::Boolean);
    let mut foreign_program = Program::default();
    let foreign_owner_id = foreign_program.add_module("capture");
    let foreign_owner = foreign_program
        .module_mut(foreign_owner_id)
        .unwrap()
        .intern_type(SsaTypeKind::Boolean);
    assert_eq!(module_id.index(), foreign_owner_id.index());
    let module = program.module_mut(module_id).unwrap();
    module.intern_type(SsaTypeKind::Boolean);
    let unknown = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    for invalid in [foreign_module, foreign_owner, unknown] {
        let before = module.types.clone();
        assert_eq!(
            shared_capture_storage(module, invalid, span),
            Err(LoweringError {
                kind: LoweringErrorKind::InvalidModel,
                span: Some(span),
            })
        );
        assert_eq!(module.types, before);
    }
}
