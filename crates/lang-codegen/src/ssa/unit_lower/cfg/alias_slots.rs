//! Pure storage and incoming-entity projections for ordered carry roles.
use lang_frontend::{name_resolution::UnitSymbolId, source::Span, type_checking::UnitExpressionId};

use super::{
    BranchExit, CarriedAccess, CarriedBinding, LoweredValue, LoweringError, LoweringErrorKind,
    lowering_error,
};
use crate::ssa::{
    carry::{
        AliasRole, AliasSlot, CaptureLoan, IncomingRole, ResultSourceLoan, SourceSlot, Temporary,
    },
    model::{EntityId, EntityType, LoanId, ValueId},
};

impl SourceSlot for CarriedBinding {
    type Source = ValueId;

    fn source(&self) -> ValueId {
        self.source
    }

    fn entity(source: ValueId) -> EntityId {
        EntityId::Value(source)
    }

    fn ty(&self) -> EntityType {
        self.ty
    }

    fn empty(source: ValueId, ty: EntityType) -> Self {
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

impl SourceSlot for CarriedAccess {
    type Source = EntityId;

    fn source(&self) -> EntityId {
        self.source
    }

    fn entity(source: EntityId) -> EntityId {
        source
    }

    fn ty(&self) -> EntityType {
        self.ty
    }

    fn empty(source: EntityId, ty: EntityType) -> Self {
        Self {
            nullable_proofs: Vec::new(),
            nullable_children: Vec::new(),
            range_sources: Vec::new(),
            captures: Vec::new(),
            symbol: None,
            receiver: None,
            source,
            ty,
            pending: Vec::new(),
        }
    }
}

impl AliasSlot<Temporary, UnitExpressionId> for CarriedBinding {
    fn alias_source(source: ValueId) -> ValueId {
        source
    }

    fn keys_mut(&mut self) -> &mut Vec<UnitExpressionId> {
        &mut self.temporaries
    }
}

impl AliasSlot<CaptureLoan, (UnitExpressionId, usize)> for CarriedAccess {
    fn alias_source(source: LoanId) -> EntityId {
        CaptureLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(UnitExpressionId, usize)> {
        &mut self.captures
    }
}

impl AliasSlot<ResultSourceLoan, (UnitSymbolId, usize)> for CarriedAccess {
    fn alias_source(source: LoanId) -> EntityId {
        ResultSourceLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(UnitSymbolId, usize)> {
        &mut self.range_sources
    }
}

#[derive(Clone, Copy)]
pub(super) enum MergeRole {
    Result,
    Receiver,
    NamedValue(UnitSymbolId),
    NamedBorrow(UnitSymbolId),
    Pending(usize),
    Temporary(UnitExpressionId),
    Capture(UnitExpressionId, usize),
    ResultSource(UnitSymbolId, usize),
    NullableProof(UnitSymbolId),
}

pub(super) struct MergeProjection {
    pub(super) role: MergeRole,
    pub(super) span: Span,
}

impl IncomingRole<BranchExit> for MergeProjection {
    fn current(&self, exit: &BranchExit) -> Result<EntityId, LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, self.span);
        let entity = match self.role {
            MergeRole::Result => match exit.result {
                LoweredValue::Value(value) => EntityId::Value(value),
                LoweredValue::Unit | LoweredValue::Diverged => return Err(missing()),
            },
            MergeRole::Receiver => exit.receiver.ok_or_else(missing)?.entity,
            MergeRole::NamedValue(symbol) => match exit.bindings[&symbol] {
                LoweredValue::Value(value) => EntityId::Value(value),
                LoweredValue::Unit | LoweredValue::Diverged => return Err(missing()),
            },
            MergeRole::NamedBorrow(symbol) => EntityId::Loan(exit.borrow_bindings[&symbol]),
            MergeRole::Pending(index) => exit.pending_operands[index],
            MergeRole::Temporary(key) => EntityId::Value(exit.temporaries[&key]),
            MergeRole::Capture(call, index) => EntityId::Loan(exit.capture_loans[&(call, index)]),
            MergeRole::ResultSource(symbol, index) => {
                EntityId::Loan(exit.result_source_loans[&symbol][index])
            }
            MergeRole::NullableProof(symbol) => EntityId::Loan(exit.nullable_proofs[&symbol].loan),
        };
        Ok(entity)
    }
}
