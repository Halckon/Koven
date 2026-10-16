//! Temporary Phase 4 boundary for real numeric Range facts before its value ABI exists.
//! Inspect actual source uses, never the canonical type table (which seeds unused builtins).
use super::lowering_support::{LoweringError, LoweringErrorKind, error};
use lang_frontend::{
    name_resolution::SourceUnitId,
    parser::ParsedFile,
    type_checking::{
        BuiltinType, CompilationUnitTypes, TypeId, TypeKind, TypedFile, UnitExpressionId,
        UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

fn range(builtin: BuiltinType) -> bool {
    matches!(builtin, BuiltinType::IntRange | BuiltinType::LongRange)
}

fn file_range(typed: &TypedFile, mut ty: TypeId) -> bool {
    for _ in 0..typed.types().len() {
        let Some(kind) = typed.types().get(ty) else {
            return false;
        };
        match kind {
            TypeKind::Builtin(builtin) => return range(*builtin),
            TypeKind::Nullable(inner) => ty = *inner,
            _ => return false,
        }
    }
    false
}

fn unit_range(typed: &CompilationUnitTypes, mut ty: UnitTypeId) -> bool {
    for _ in 0..typed.types().len() {
        let Some(kind) = typed.types().get(ty) else {
            return false;
        };
        match kind {
            UnitTypeKind::Builtin(builtin) => return range(*builtin),
            UnitTypeKind::Nullable(inner) => ty = *inner,
            _ => return false,
        }
    }
    false
}

pub(super) fn file(parsed: &ParsedFile, typed: &TypedFile) -> Result<(), LoweringError> {
    for (id, node) in parsed.ast().type_refs().iter() {
        if typed
            .type_ref_type(id)
            .is_some_and(|ty| file_range(typed, ty))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    for (id, node) in parsed.ast().expressions().iter() {
        if typed.integer_range_operation(id).is_some()
            || typed
                .expression_type(id)
                .is_some_and(|ty| file_range(typed, ty))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}

pub(super) fn unit(
    source: SourceUnitId,
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
) -> Result<(), LoweringError> {
    // Container/function syntax has child TypeRefs; nullable is part of its own TypeRef.
    for (id, node) in parsed.ast().type_refs().iter() {
        if typed
            .type_ref_type(UnitTypeRefId::new(source, id))
            .is_some_and(|ty| unit_range(typed, ty))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    for (id, node) in parsed.ast().expressions().iter() {
        let id = UnitExpressionId::new(source, id);
        if typed.integer_range_operation(id).is_some()
            || typed
                .expression_type(id)
                .is_some_and(|ty| unit_range(typed, ty))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}
