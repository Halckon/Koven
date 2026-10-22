//! Single-frontier slots preserve independent seeds and first-source associations.
use super::{ColumnId, ColumnMatch};
use crate::ssa::{
    lowering_support::LoweringError,
    model::{EntityId, EntityType, Function},
};

/// Storage fields only; role selection and validation stay at their original call sites.
pub(in crate::ssa) trait SourceSlot {
    type Source: Copy + Eq;

    fn source(&self) -> Self::Source;
    fn entity(source: Self::Source) -> EntityId;
    fn ty(&self) -> EntityType;
    fn empty(source: Self::Source, ty: EntityType) -> Self;
}

pub(in crate::ssa) enum TypeAtMiss {
    Known(EntityType),
    Query,
}

/// Append the complete slot, including an intentional same-source snapshot.
pub(in crate::ssa) fn seed_slot<S: SourceSlot>(slots: &mut Vec<S>, slot: S) -> ColumnId {
    let column = ColumnId::from_index(slots.len());
    slots.push(slot);
    column
}

pub(in crate::ssa) fn associate_source<S: SourceSlot>(
    function: &Function,
    slots: &mut Vec<S>,
    source: S::Source,
    ty: TypeAtMiss,
    missing: LoweringError,
) -> Result<ColumnMatch, LoweringError> {
    if let Some(index) = slots.iter().position(|slot| slot.source() == source) {
        return Ok(ColumnMatch::Reused(ColumnId::from_index(index)));
    }
    let ty = match ty {
        TypeAtMiss::Known(ty) => ty,
        TypeAtMiss::Query => function.entity(S::entity(source)).ok_or(missing)?.ty,
    };
    Ok(ColumnMatch::Added(seed_slot(slots, S::empty(source, ty))))
}

/// Only callers with an existing hit-type gate request this check.
pub(in crate::ssa) fn require_column_type<S: SourceSlot>(
    slots: &[S],
    column: ColumnId,
    expected: EntityType,
    mismatch: LoweringError,
) -> Result<(), LoweringError> {
    if slots[column.index()].ty() != expected {
        return Err(mismatch);
    }
    Ok(())
}
