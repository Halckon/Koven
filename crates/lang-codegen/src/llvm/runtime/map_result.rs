//! Owned Map result destroys its payload only on the present branch.
use super::{LlvmAdapterError, RuntimeAbi};
use crate::ssa::model::{Module, SsaTypeId};
use inkwell::{
    builder::Builder,
    values::{BasicValueEnum, FunctionValue},
};

impl<'ctx> RuntimeAbi<'ctx> {
    pub(super) fn define_map_result_drop(
        &self,
        module: &Module,
        ty: SsaTypeId,
        function: FunctionValue<'ctx>,
        builder: &Builder<'ctx>,
        value: BasicValueEnum<'ctx>,
    ) -> Result<(), LlvmAdapterError> {
        let inner = module
            .map_result_value(ty)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("Map result payload missing".into()))?;
        let header = value.into_struct_value();
        let present = builder
            .build_extract_value(header, 0, "present")?
            .into_int_value();
        let occupied = self.context.append_basic_block(function, "present");
        let done = self.context.append_basic_block(function, "done");
        builder.build_conditional_branch(present, occupied, done)?;
        builder.position_at_end(occupied);
        let payload = builder.build_extract_value(header, 1, "payload")?;
        self.emit_drop(builder, inner, payload)?;
        builder.build_unconditional_branch(done)?;
        builder.position_at_end(done);
        Ok(())
    }
}
