//! Pure storage projections for ordered carry alias roles.
use lang_frontend::{name_resolution::UnitSymbolId, type_checking::UnitExpressionId};

use super::{CarriedAccess, CarriedBinding};
use crate::ssa::{
    carry::{AliasSlot, CaptureLoan, ResultSourceLoan, Temporary},
    model::{EntityId, EntityType, LoanId, ValueId},
};

impl AliasSlot<Temporary, UnitExpressionId> for CarriedBinding {
    fn source(&self) -> EntityId {
        EntityId::Value(self.source)
    }

    fn keys_mut(&mut self) -> &mut Vec<UnitExpressionId> {
        &mut self.temporaries
    }

    fn from_alias(source: ValueId, ty: EntityType) -> Self {
        Self {
            symbol: None,
            receiver: None,
            source,
            ty,
            pending: Vec::new(),
            temporaries: Vec::new(),
        }
    }
}

impl AliasSlot<CaptureLoan, (UnitExpressionId, usize)> for CarriedAccess {
    fn source(&self) -> EntityId {
        self.source
    }

    fn keys_mut(&mut self) -> &mut Vec<(UnitExpressionId, usize)> {
        &mut self.captures
    }

    fn from_alias(source: LoanId, ty: EntityType) -> Self {
        Self {
            nullable_proofs: Vec::new(),
            nullable_children: Vec::new(),
            range_sources: Vec::new(),
            captures: Vec::new(),
            symbol: None,
            receiver: None,
            source: EntityId::Loan(source),
            ty,
            pending: Vec::new(),
        }
    }
}

impl AliasSlot<ResultSourceLoan, (UnitSymbolId, usize)> for CarriedAccess {
    fn source(&self) -> EntityId {
        self.source
    }

    fn keys_mut(&mut self) -> &mut Vec<(UnitSymbolId, usize)> {
        &mut self.range_sources
    }

    fn from_alias(source: LoanId, ty: EntityType) -> Self {
        Self {
            nullable_proofs: Vec::new(),
            nullable_children: Vec::new(),
            range_sources: Vec::new(),
            captures: Vec::new(),
            symbol: None,
            receiver: None,
            source: EntityId::Loan(source),
            ty,
            pending: Vec::new(),
        }
    }
}
