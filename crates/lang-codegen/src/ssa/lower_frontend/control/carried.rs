//! Construct and restore explicit linear CFG storage slots, and retire while views on non-common if exits.
use super::*;
use crate::ssa::carry::{
    AliasRole, AliasSlot, CaptureLoan, ResultSourceLoan, SourceSlot, Temporary,
};
use crate::ssa::model::LoanId;

impl SourceSlot for LinearBindingSlot {
    type Source = EntityId;

    fn source(&self) -> EntityId {
        self.source
    }

    fn entity(source: EntityId) -> EntityId {
        source
    }

    fn empty(source: EntityId, ty: EntityType) -> Self {
        Self::new(None, source, ty)
    }
}

impl AliasSlot<Temporary, usize> for LinearBindingSlot {
    fn alias_source(source: ValueId) -> EntityId {
        Temporary::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<usize> {
        &mut self.temporaries
    }
}

impl AliasSlot<CaptureLoan, (usize, usize)> for LinearBindingSlot {
    fn alias_source(source: LoanId) -> EntityId {
        CaptureLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(usize, usize)> {
        &mut self.captures
    }
}

impl AliasSlot<ResultSourceLoan, (SymbolId, usize)> for LinearBindingSlot {
    fn alias_source(source: LoanId) -> EntityId {
        ResultSourceLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(SymbolId, usize)> {
        &mut self.result_sources
    }
}

mod enum_type_test {
    //! 普通 enum 类型测试与无 payload Copyable enum case 条件只比较已登记 case tag。
}
