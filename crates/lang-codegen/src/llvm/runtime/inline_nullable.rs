//! ADR-0033: an owned inline nullable destroys its payload only when present.
use super::{LlvmAdapterError, RuntimeAbi};
use crate::ssa::model::SsaTypeId;
use inkwell::{
    builder::Builder,
    values::{BasicValueEnum, FunctionValue},
};

impl<'ctx> RuntimeAbi<'ctx> {
    pub(super) fn define_inline_nullable_drop(
        &self,
        inner: SsaTypeId,
        niche: bool,
        function: FunctionValue<'ctx>,
        builder: &Builder<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> Result<(), LlvmAdapterError> {
        let wrapper = value.into_struct_value();
        let present = if niche {
            // String and container owners keep a non-null data pointer in field 0.
            let data = builder
                .build_extract_value(wrapper, 0, "data")?
                .into_pointer_value();
            builder.build_is_not_null(data, "present")?
        } else {
            builder
                .build_extract_value(wrapper, 0, "present")?
                .into_int_value()
        };
        let occupied = self.context.append_basic_block(function, "present");
        let done = self.context.append_basic_block(function, "done");
        builder.build_conditional_branch(present, occupied, done)?;
        builder.position_at_end(occupied);
        let payload = if niche {
            value
        } else {
            builder.build_extract_value(wrapper, 1, "payload")?
        };
        self.emit_drop(builder, inner, payload)?;
        builder.build_unconditional_branch(done)?;
        builder.position_at_end(done);
        Ok(())
    }
}
