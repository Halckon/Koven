use inkwell::values::{BasicValueEnum, PointerValue};

use crate::ssa::model::{PlaceAccess, PlaceId, ValueId};

use super::{FunctionLowerer, LlvmAdapterError};

impl<'ctx, 'llvm, 'ssa, 'functions> FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions> {
    pub(super) fn pointer_value(
        &self,
        id: ValueId,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        match self.value(id)? {
            BasicValueEnum::PointerValue(value) => Ok(value),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "heap owner operand 不是 LLVM pointer".to_owned(),
            )),
        }
    }

    pub(super) fn place(&self, id: PlaceId) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        self.places
            .get(&id)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM place 映射缺失".to_owned()))
    }

    pub(super) fn access(
        &self,
        access: PlaceAccess,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        match access {
            PlaceAccess::Place(place) => self.place(place),
            PlaceAccess::Loan(loan) => self
                .loans
                .get(&loan)
                .copied()
                .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM loan 映射缺失".to_owned())),
        }
    }
}
