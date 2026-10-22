//! Ordered aliases share a physical carry column and its current typed entity.
mod columns;
mod source_columns;

pub(in crate::ssa) use columns::{
    ColumnId, ColumnMatch, IncomingColumns, IncomingRole, record_type,
};
pub(in crate::ssa) use source_columns::{
    SourceSlot, TypeAtMiss, associate_source, require_column_type, seed_slot,
};

use std::collections::BTreeMap;

use super::{
    lowering_support::LoweringError,
    model::{EntityId, Function, LoanId, ValueId},
};

/// Fixed storage roles only; callers retain validation and role scheduling.
pub(in crate::ssa) trait AliasRole {
    type Id: Copy;

    fn entity(id: Self::Id) -> EntityId;
}

pub(in crate::ssa) struct Temporary;
pub(in crate::ssa) struct CaptureLoan;
pub(in crate::ssa) struct ResultSourceLoan;

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

impl AliasRole for ResultSourceLoan {
    type Id = LoanId;

    fn entity(id: LoanId) -> EntityId {
        EntityId::Loan(id)
    }
}

/// Storage projections only; the shared algorithm owns lookup, append and alias fan-out.
pub(in crate::ssa) trait AliasSlot<R: AliasRole, K>: SourceSlot {
    fn alias_source(source: R::Id) -> Self::Source;
    fn keys_mut(&mut self) -> &mut Vec<K>;
}

pub(in crate::ssa) fn collect_aliases<R: AliasRole, K: Copy + Ord, S: AliasSlot<R, K>>(
    function: &Function,
    aliases: impl IntoIterator<Item = (K, R::Id)>,
    slots: &mut Vec<S>,
    missing: LoweringError,
) -> Result<(), LoweringError> {
    for (key, id) in aliases {
        let choice = associate_source(
            function,
            slots,
            S::alias_source(id),
            TypeAtMiss::Query,
            missing,
        )?;
        slots[choice.column().index()].keys_mut().push(key);
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

/// Preserve symbol order and each saved vector position without inspecting entities.
pub(in crate::ssa) fn indexed_aliases<K: Copy, Id: Copy>(
    aliases: &BTreeMap<K, Vec<Id>>,
) -> impl Iterator<Item = ((K, usize), Id)> + '_ {
    aliases.iter().flat_map(|(&key, values)| {
        values
            .iter()
            .copied()
            .enumerate()
            .map(move |(index, id)| ((key, index), id))
    })
}

/// Restore this physical column into the original symbol-qualified vector positions.
pub(in crate::ssa) fn apply_indexed_alias_column<K: Copy + Ord, Id: Copy>(
    keys: &[(K, usize)],
    current: Id,
    aliases: &mut BTreeMap<K, Vec<Id>>,
) {
    for &(key, index) in keys {
        let values = aliases.entry(key).or_default();
        if values.len() <= index {
            values.resize(index + 1, current);
        }
        values[index] = current;
    }
}
