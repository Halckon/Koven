//! Verified exclusive root exchange: move storage without invoking resource glue.

use inkwell::types::BasicTypeEnum;

use crate::ssa::model::{SsaTypeId, SsaTypeKind};

use super::{
    FunctionLowerer, LlvmAdapterError, LoanId, PlaceAccess, ValueId, invalid_result_count,
    value_name, value_type,
};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    /// Unit has local zero-sized storage for root exchange and CFG values. Function signatures
    /// still use TypeMap directly, so this does not introduce a non-void Unit ABI.
    pub(super) fn local_storage_type(
        &self,
        ty: SsaTypeId,
    ) -> Result<BasicTypeEnum<'ctx>, LlvmAdapterError> {
        if matches!(self.module.type_kind(ty), Some(SsaTypeKind::Unit)) {
            Ok(self.context.struct_type(&[], false).into())
        } else {
            self.dependencies.type_map.basic_type(ty)
        }
    }

    pub(super) fn lower_root_replace(
        &mut self,
        owner: ValueId,
        loan: LoanId,
        replacement: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [new_root, old] = results else {
            return Err(invalid_result_count("root replace", 2, results.len()));
        };
        let pointer = self.access(PlaceAccess::Loan(loan))?;
        let replacement = self.value(replacement)?;
        let storage = self.local_storage_type(value_type(self.function, owner)?)?;
        // Read the current root storage, not the owner's original SSA payload. The verifier
        // proves this loan is active, exclusive and exactly covers the owned root.
        let old_value = self
            .builder
            .build_load(storage, pointer, &value_name(*old))?;
        self.builder.build_store(pointer, replacement)?;
        self.loans.remove(&loan);
        self.zero_sized_loans.remove(&loan);
        self.values.insert(*new_root, replacement);
        self.values.insert(*old, old_value);
        Ok(())
    }

    pub(super) fn lower_heap_field_exchange(
        &mut self,
        loan: LoanId,
        replacement: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [old] = results else {
            return Err(invalid_result_count(
                "heap field exchange",
                1,
                results.len(),
            ));
        };
        let pointer = self.access(PlaceAccess::Loan(loan))?;
        let replacement = self.value(replacement)?;
        let storage = self.local_storage_type(value_type(self.function, *old)?)?;
        // The verified loan denotes this exact direct field of the still-live parent.
        // Load before store; never call drop glue or expose an empty field in between.
        let old_value = self
            .builder
            .build_load(storage, pointer, &value_name(*old))?;
        self.builder.build_store(pointer, replacement)?;
        self.loans.remove(&loan);
        self.zero_sized_loans.remove(&loan);
        self.values.insert(*old, old_value);
        Ok(())
    }

    pub(super) fn lower_root_swap(
        &mut self,
        owners: [ValueId; 2],
        loans: [LoanId; 2],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [new_a, new_b] = results else {
            return Err(invalid_result_count("root swap", 2, results.len()));
        };
        let pointer_a = self.access(PlaceAccess::Loan(loans[0]))?;
        let pointer_b = self.access(PlaceAccess::Loan(loans[1]))?;
        let storage = self.local_storage_type(value_type(self.function, owners[0])?)?;
        // Both old values are loaded before either store. No drop, clone, retain, allocation
        // or other observable call may interrupt the ownership exchange.
        let old_a = self
            .builder
            .build_load(storage, pointer_a, &value_name(*new_b))?;
        let old_b = self
            .builder
            .build_load(storage, pointer_b, &value_name(*new_a))?;
        self.builder.build_store(pointer_a, old_b)?;
        self.builder.build_store(pointer_b, old_a)?;
        for loan in loans {
            self.loans.remove(&loan);
            self.zero_sized_loans.remove(&loan);
        }
        self.values.insert(*new_a, old_b);
        self.values.insert(*new_b, old_a);
        Ok(())
    }
}
