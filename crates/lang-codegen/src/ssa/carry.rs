//! Ordered aliases share a physical carry column and its current typed entity.
use std::collections::BTreeMap;

use lang_frontend::source::Span;

use super::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityId, EntityType, Function, LoanId, ValueId},
};

/// Fixed storage roles only; callers retain validation and role scheduling.
pub(in crate::ssa) trait AliasRole {
    type Id: Copy;

    fn entity(id: Self::Id) -> EntityId;
}

pub(in crate::ssa) struct Temporary;
pub(in crate::ssa) struct CaptureLoan;

impl AliasRole for Temporary {
    type Id = ValueId;

    fn entity(id: ValueId) -> EntityId {
        EntityId::Value(id)
    }
}

impl AliasRole for CaptureLoan {
    type Id = LoanId;

    fn entity(id: LoanId) -> EntityId {
        EntityId::Loan(id)
    }
}

/// Storage projections only; the shared algorithm owns lookup, append and alias fan-out.
pub(in crate::ssa) trait AliasSlot<R: AliasRole, K> {
    fn source(&self) -> EntityId;
    fn keys_mut(&mut self) -> &mut Vec<K>;
    fn from_alias(source: R::Id, ty: EntityType) -> Self;
}

pub(in crate::ssa) fn collect_aliases<R: AliasRole, K: Copy + Ord, S: AliasSlot<R, K>>(
    function: &Function,
    aliases: &BTreeMap<K, R::Id>,
    slots: &mut Vec<S>,
    span: Span,
) -> Result<(), LoweringError> {
    for (&key, &id) in aliases {
        let source = R::entity(id);
        // Keep seed columns and reuse only the first match; a hit never queries its type.
        if let Some(slot) = slots.iter_mut().find(|slot| slot.source() == source) {
            slot.keys_mut().push(key);
        } else {
            let ty = function
                .entity(source)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .ty;
            let mut slot = S::from_alias(id, ty);
            slot.keys_mut().push(key);
            slots.push(slot);
        }
    }
    Ok(())
}

/// Apply one column in the caller's original role order, without searching by old source.
pub(in crate::ssa) fn apply_alias_column<K: Copy + Ord, Id: Copy>(
    keys: &[K],
    current: Id,
    aliases: &mut BTreeMap<K, Id>,
) {
    for &key in keys {
        aliases.insert(key, current);
    }
}
