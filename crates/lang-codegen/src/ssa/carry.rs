//! Ordered aliases share a physical carry column and its current typed entity.
pub(in crate::ssa) mod columns;
pub(in crate::ssa) mod source_columns;

pub(in crate::ssa) use columns::{ColumnId, ColumnMatch, IncomingColumns, IncomingRole};
pub(in crate::ssa) use source_columns::{SourceSlot, TypeAtMiss, associate_source, seed_slot};

use std::collections::BTreeMap;

#[cfg(test)]
use super::model::{EntityId, LoanId};
use super::{
    lowering_support::LoweringError,
    model::{Function, ValueId},
};

/// Fixed storage roles only; callers retain validation and role scheduling.
pub(in crate::ssa) trait AliasRole {
    type Id: Copy;

    #[cfg(test)]
    fn entity(id: Self::Id) -> EntityId;
}

pub(in crate::ssa) struct Temporary;
#[cfg(test)]
pub(in crate::ssa) struct CaptureLoan;
#[cfg(test)]
pub(in crate::ssa) struct ResultSourceLoan;

impl AliasRole for Temporary {
    type Id = ValueId;

    #[cfg(test)]
    fn entity(id: ValueId) -> EntityId {
        EntityId::Value(id)
    }
}

#[cfg(test)]
impl AliasRole for CaptureLoan {
    type Id = LoanId;

    fn entity(id: LoanId) -> EntityId {
        EntityId::Loan(id)
    }
}

#[cfg(test)]
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
