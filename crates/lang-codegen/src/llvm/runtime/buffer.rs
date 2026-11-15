//! Target-sized logical length bridge and checked contiguous buffer allocation.
//! Allocation failure and ZST/empty storage keep the shared RuntimeAbi contract.

use super::{AbortMessage, LlvmAdapterError, RuntimeAbi};
use crate::llvm::container::max_logical_length;
use inkwell::{
    AddressSpace, IntPredicate,
    builder::Builder,
    intrinsics::Intrinsic,
    module::Module as LlvmModule,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind,
    },
};

impl<'ctx> RuntimeAbi<'ctx> {
    pub(in crate::llvm) fn container_int_to_size(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        length: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        let negative = builder.build_int_compare(
            IntPredicate::SLT,
            length,
            length.get_type().const_zero(),
            &format!("{name}.negative"),
        )?;
        self.abort_if(
            builder,
            function,
            negative,
            name,
            AbortMessage::Text("negative size"),
        )?;
        match length
            .get_type()
            .get_bit_width()
            .cmp(&self.size_type.get_bit_width())
        {
            std::cmp::Ordering::Less => {
                Ok(builder.build_int_z_extend(length, self.size_type, &format!("{name}.size"))?)
            }
            std::cmp::Ordering::Equal => Ok(length),
            std::cmp::Ordering::Greater => {
                let maximum = length
                    .get_type()
                    .const_int(max_logical_length(self.size_type.get_bit_width()), false);
                let too_large = builder.build_int_compare(
                    IntPredicate::UGT,
                    length,
                    maximum,
                    &format!("{name}.size.overflow"),
                )?;
                self.abort_if(
                    builder,
                    function,
                    too_large,
                    name,
                    AbortMessage::Text("size overflow"),
                )?;
                Ok(builder.build_int_truncate(length, self.size_type, &format!("{name}.size"))?)
            }
        }
    }

    pub(in crate::llvm) fn allocate_buffer(
        &self,
        llvm: &LlvmModule<'ctx>,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
        stride: u64,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        if length.get_type().get_bit_width() != self.size_type.get_bit_width() {
            return Err(LlvmAdapterError::Unsupported(
                "buffer length 必须使用目标 size_t".to_owned(),
            ));
        }
        let sentinel = self
            .zst_sentinel
            .ok_or_else(|| LlvmAdapterError::Build("容器 sentinel 未声明".to_owned()))?;
        if stride > self.max_buffer_bytes {
            return Err(LlvmAdapterError::Unsupported(
                "container element stride exceeds pointer index range".to_owned(),
            ));
        }
        let failed = self
            .context
            .append_basic_block(function, &format!("{name}.abort"));
        let valid = self
            .context
            .append_basic_block(function, &format!("{name}.valid"));
        // The unsigned logical length may use size_t's high bit. Physical storage
        // is separately bounded by the signed pointer-index maximum below.
        builder.build_unconditional_branch(valid)?;

        builder.position_at_end(failed);
        self.emit_abort_with(builder, AbortMessage::Text("allocation failed"))?;

        builder.position_at_end(valid);
        if stride == 0 {
            return Ok(sentinel);
        }
        let intrinsic = Intrinsic::find("llvm.umul.with.overflow")
            .and_then(|intrinsic| intrinsic.get_declaration(llvm, &[self.size_type.into()]))
            .ok_or_else(|| {
                LlvmAdapterError::Build(
                    "无法声明 LLVM unsigned multiply overflow intrinsic".to_owned(),
                )
            })?;
        let product = match builder
            .build_call(
                intrinsic,
                &[
                    BasicMetadataValueEnum::from(length),
                    BasicMetadataValueEnum::from(self.size_type.const_int(stride, false)),
                ],
                &format!("{name}.size"),
            )?
            .try_as_basic_value()
        {
            ValueKind::Basic(value) => value.into_struct_value(),
            ValueKind::Instruction(_) => {
                return Err(LlvmAdapterError::Build(
                    "size overflow intrinsic 未返回 aggregate".to_owned(),
                ));
            }
        };
        let bytes = builder
            .build_extract_value(product, 0, &format!("{name}.bytes"))?
            .into_int_value();
        let overflow = builder
            .build_extract_value(product, 1, &format!("{name}.overflow"))?
            .into_int_value();
        let sized = self
            .context
            .append_basic_block(function, &format!("{name}.sized"));
        builder.build_conditional_branch(overflow, failed, sized)?;

        builder.position_at_end(sized);
        let too_large = builder.build_int_compare(
            IntPredicate::UGT,
            bytes,
            self.size_type.const_int(self.max_buffer_bytes, false),
            &format!("{name}.too_large"),
        )?;
        let within_index = self
            .context
            .append_basic_block(function, &format!("{name}.within_index"));
        builder.build_conditional_branch(too_large, failed, within_index)?;
        builder.position_at_end(within_index);
        let no_allocation = self
            .context
            .append_basic_block(function, &format!("{name}.empty"));
        let allocate = self
            .context
            .append_basic_block(function, &format!("{name}.allocate"));
        let ready = self
            .context
            .append_basic_block(function, &format!("{name}.ready"));
        let empty = builder.build_int_compare(
            IntPredicate::EQ,
            bytes,
            self.size_type.const_zero(),
            &format!("{name}.zero_bytes"),
        )?;
        builder.build_conditional_branch(empty, no_allocation, allocate)?;

        builder.position_at_end(no_allocation);
        builder.build_unconditional_branch(ready)?;

        builder.position_at_end(allocate);
        let malloc = self
            .malloc
            .ok_or_else(|| LlvmAdapterError::Build("malloc 未声明".to_owned()))?;
        let allocation = match builder
            .build_call(
                malloc,
                &[BasicMetadataValueEnum::from(bytes)],
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
        let allocation_failed = builder.build_is_null(allocation, &format!("{name}.oom"))?;
        let allocated = self
            .context
            .append_basic_block(function, &format!("{name}.allocated"));
        builder.build_conditional_branch(allocation_failed, failed, allocated)?;

        builder.position_at_end(allocated);
        builder.build_unconditional_branch(ready)?;

        builder.position_at_end(ready);
        let pointer = builder.build_phi(
            self.context.ptr_type(AddressSpace::default()),
            &format!("{name}.pointer"),
        )?;
        pointer.add_incoming(&[(&sentinel, no_allocation), (&allocation, allocated)]);
        Ok(pointer.as_basic_value().into_pointer_value())
    }
}
