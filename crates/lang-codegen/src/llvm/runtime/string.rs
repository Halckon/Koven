//! String byte buffer 使用的集中系统分配与 stdout ABI。

use inkwell::{
    IntPredicate,
    builder::Builder,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind,
    },
};

use super::RuntimeAbi;
use crate::llvm::LlvmAdapterError;

impl<'ctx> RuntimeAbi<'ctx> {
    pub(in crate::llvm) fn allocate_string_bytes(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        length: IntValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let malloc = self
            .malloc
            .ok_or_else(|| LlvmAdapterError::Build("malloc 未声明".to_owned()))?;
        let allocation = match builder
            .build_call(
                malloc,
                &[BasicMetadataValueEnum::from(length)],
                &format!("{name}.buffer"),
            )?
            .try_as_basic_value()
        {
            ValueKind::Basic(BasicValueEnum::PointerValue(pointer)) => pointer,
            _ => {
                return Err(LlvmAdapterError::Build(
                    "malloc 未返回 LLVM pointer".to_owned(),
                ));
            }
        };
        let failed = builder.build_is_null(allocation, &format!("{name}.oom"))?;
        self.abort_if(builder, function, failed, name)?;
        Ok(allocation)
    }

    pub(in crate::llvm) fn emit_write_bytes(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        bytes: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        name: &str,
    ) -> Result<(), LlvmAdapterError> {
        let call = builder.build_call(
            self.write
                .ok_or_else(|| LlvmAdapterError::Build("write 未声明".to_owned()))?,
            &[
                BasicMetadataValueEnum::from(self.context.i32_type().const_int(1, false)),
                BasicMetadataValueEnum::from(bytes),
                BasicMetadataValueEnum::from(length),
            ],
            &format!("{name}.written"),
        )?;
        let written = match call.try_as_basic_value() {
            ValueKind::Basic(BasicValueEnum::IntValue(value)) => value,
            _ => {
                return Err(LlvmAdapterError::Build(
                    "write 未返回整数 byte count".to_owned(),
                ));
            }
        };
        let incomplete = builder.build_int_compare(
            IntPredicate::NE,
            written,
            length,
            &format!("{name}.incomplete"),
        )?;
        self.abort_if(builder, function, incomplete, name)
    }
}
