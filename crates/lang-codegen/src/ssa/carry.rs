//! Ordered temporary aliases share a physical carry column and its current Value.
use std::collections::BTreeMap;

use lang_frontend::source::Span;

use super::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityId, EntityType, Function, ValueId},
};

/// Storage projections only; the shared algorithm owns lookup, append and alias fan-out.
pub(in crate::ssa) trait TemporarySlot<K> {
    fn source(&self) -> EntityId;
    fn temporary_keys_mut(&mut self) -> &mut Vec<K>;
    fn from_temporary(source: ValueId, ty: EntityType) -> Self;
}

pub(in crate::ssa) fn collect_temporaries<K: Copy + Ord, S: TemporarySlot<K>>(
    function: &Function,
    temporaries: &BTreeMap<K, ValueId>,
    slots: &mut Vec<S>,
    span: Span,
) -> Result<(), LoweringError> {
    for (&key, &owner) in temporaries {
        let source = EntityId::Value(owner);
        // Keep seed columns and reuse only the first match; a hit never queries its type.
        if let Some(slot) = slots.iter_mut().find(|slot| slot.source() == source) {
            slot.temporary_keys_mut().push(key);
        } else {
            let ty = function
                .entity(source)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .ty;
            let mut slot = S::from_temporary(owner, ty);
            slot.temporary_keys_mut().push(key);
            slots.push(slot);
        }
    }
    Ok(())
}

/// Apply one column in the caller's original role order, without searching by old source.
pub(in crate::ssa) fn apply_temporary_column<K: Copy + Ord>(
    keys: &[K],
    current: ValueId,
    temporaries: &mut BTreeMap<K, ValueId>,
) {
    for &key in keys {
        temporaries.insert(key, current);
    }
}
