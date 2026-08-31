use inkwell::values::{BasicValueEnum, PointerValue};

use crate::ssa::model::{EntityId, EntityType, LoanId, PlaceAccess, PlaceId, ValueId};

use super::{FunctionLowerer, LlvmAdapterError};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
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

    /// Resolve a receiver loan to the same heap handle stored by the caller, then project a
    /// payload field. The returned pointer never aliases the receiver binding storage itself.
    pub(super) fn heap_field_pointer(
        &self,
        receiver: LoanId,
        field: usize,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let EntityType::Loan { target: owner, .. } = self
            .function
            .entity(EntityId::Loan(receiver))
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("heap receiver loan 不存在".to_owned()))?
            .ty
        else {
            return Err(LlvmAdapterError::InvalidSsa(
                "heap receiver operand 不是 loan".to_owned(),
            ));
        };
        let payload = self.module.heap_payload(owner).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("heap receiver loan target 不是 heap owner".to_owned())
        })?;
        let handle = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(owner)?,
                self.access(PlaceAccess::Loan(receiver))?,
                &format!("{name}.handle"),
            )?
            .into_pointer_value();
        self.builder
            .build_struct_gep(
                self.dependencies.type_map.aggregate_type(payload)?,
                handle,
                field as u32,
                name,
            )
            .map_err(Into::into)
    }
}
