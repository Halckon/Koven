//! ADR-0033 inline nullable values: `{present, payload}` or a niche-carrying owner.
use inkwell::values::{BasicValue, BasicValueEnum, IntValue};

use super::{FunctionLowerer, LlvmAdapterError, value_name, value_type};
use crate::ssa::model::{SsaTypeId, ValueId};

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
