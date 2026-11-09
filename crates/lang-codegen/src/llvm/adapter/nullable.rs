//! ADR-0033 inline nullable values: `{present, payload}` or a niche-carrying owner.
use inkwell::values::{BasicValue, BasicValueEnum, IntValue};

use super::{FunctionLowerer, LlvmAdapterError, access_type, value_name, value_type};
use crate::ssa::model::{LoanId, PlaceAccess, SsaTypeId, ValueId};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    /// `Some` for an inline nullable result: the niche form is the inner value itself.
    pub(super) fn inline_nullable_wrap(
        &self,
        result: ValueId,
        owner: ValueId,
    ) -> Result<Option<BasicValueEnum<'ctx>>, LlvmAdapterError> {
        let ty = value_type(self.function, result)?;
        let Some((_, niche)) = self.module.inline_nullable(ty) else {
            return Ok(None);
        };
        let value = self.value(owner)?;
        if niche {
            return Ok(Some(value));
        }
        let header = self
            .dependencies
            .type_map
            .basic_type(ty)?
            .into_struct_type()
            .const_zero();
        let present = self.context.bool_type().const_int(1, false);
        let header = self
            .builder
            .build_insert_value(header, present, 0, "nullable.present")?;
        Ok(Some(
            self.builder
                .build_insert_value(header, value, 1, &value_name(result))?
                .as_basic_value_enum(),
        ))
    }

    /// `Some((inner, payload))` for an inline nullable operand on a proven non-null edge.
    pub(super) fn inline_nullable_payload(
        &self,
        owner: ValueId,
    ) -> Result<Option<(SsaTypeId, BasicValueEnum<'ctx>)>, LlvmAdapterError> {
        let Some((inner, niche)) = self
            .module
            .inline_nullable(value_type(self.function, owner)?)
        else {
            return Ok(None);
        };
        let payload = if niche {
            self.value(owner)?
        } else {
            self.builder
                .build_extract_value(self.struct_value(owner)?, 1, "nullable.payload")?
        };
        Ok(Some((inner, payload)))
    }

    /// Read only the presence field (or the handle itself) through the existing loan.
    pub(super) fn nullable_loan_is_null(
        &self,
        source: LoanId,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        let source = PlaceAccess::Loan(source);
        let ty = access_type(self.function, source)?;
        let storage = self.access(source)?;
        let llvm_type = self.dependencies.type_map.basic_type(ty)?;
        let presence_niche = self
            .module
            .inline_nullable(ty)
            .map(|(_, niche)| niche)
            .or_else(|| self.module.map_result_value(ty).map(|_| false));
        let Some(niche) = presence_niche else {
            let handle = self
                .builder
                .build_load(llvm_type, storage, "nullable.borrowed.handle")?;
            return Ok(self
                .builder
                .build_is_null(handle.into_pointer_value(), name)?);
        };
        let wrapper = llvm_type.into_struct_type();
        let presence_type = wrapper.get_field_type_at_index(0).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("inline nullable requires a presence field".into())
        })?;
        let presence_slot = self.builder.build_struct_gep(
            wrapper,
            storage,
            0,
            "nullable.borrowed.presence.slot",
        )?;
        let presence =
            self.builder
                .build_load(presence_type, presence_slot, "nullable.borrowed.presence")?;
        if niche {
            Ok(self
                .builder
                .build_is_null(presence.into_pointer_value(), name)?)
        } else {
            Ok(self.builder.build_not(presence.into_int_value(), name)?)
        }
    }

    pub(super) fn inline_nullable_is_null(
        &self,
        owner: ValueId,
        name: &str,
    ) -> Result<Option<IntValue<'ctx>>, LlvmAdapterError> {
        let Some((_, niche)) = self
            .module
            .inline_nullable(value_type(self.function, owner)?)
        else {
            return Ok(None);
        };
        let wrapper = self.struct_value(owner)?;
        let is_null = if niche {
            // String and container owners keep a non-null data pointer in field 0.
            let data = self
                .builder
                .build_extract_value(wrapper, 0, "nullable.data")?
                .into_pointer_value();
            self.builder.build_is_null(data, name)?
        } else {
            let present = self
                .builder
                .build_extract_value(wrapper, 0, "nullable.present")?
                .into_int_value();
            self.builder.build_not(present, name)?
        };
        Ok(Some(is_null))
    }
}
