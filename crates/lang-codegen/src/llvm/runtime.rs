//! Koven heap owner 的系统分配 ABI 与类型定向 drop glue。
#[cfg(test)]
mod narrow_tests;
mod string;

use std::collections::{BTreeMap, BTreeSet};

use inkwell::{
    AddressSpace, IntPredicate,
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    intrinsics::Intrinsic,
    module::{Linkage, Module as LlvmModule},
    targets::TargetData,
    types::IntType,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind,
    },
};

use crate::ssa::model::{
    ClosureCaptureMode, EntityId, EntityType, FunctionId, Module, Operation, Ownership, SsaTypeId,
    SsaTypeKind, TerminatorKind,
};

use super::{
    LlvmAdapterError, container::max_logical_length, entry::NativeEntryPlan, tagged,
    type_map::TypeMap,
};

pub(super) struct RuntimeAbi<'ctx> {
    context: &'ctx Context,
    size_type: IntType<'ctx>,
    max_buffer_bytes: u64,
    malloc: Option<FunctionValue<'ctx>>,
    abort: Option<FunctionValue<'ctx>>,
    write: Option<FunctionValue<'ctx>>,
    free: Option<FunctionValue<'ctx>>,
    allocation_sizes: BTreeMap<SsaTypeId, u64>,
    shared_allocation_sizes: BTreeMap<SsaTypeId, u64>,
    zst_sentinel: Option<PointerValue<'ctx>>,
    drop_functions: BTreeMap<SsaTypeId, FunctionValue<'ctx>>,
}

impl<'ctx> RuntimeAbi<'ctx> {
    pub(super) fn lower(
        context: &'ctx Context,
        llvm: &LlvmModule<'ctx>,
        module: &Module,
        types: &TypeMap<'ctx>,
        target: &TargetData,
        native_entry: Option<NativeEntryPlan>,
    ) -> Result<Self, LlvmAdapterError> {
        let requirements = RuntimeRequirements::collect(module, native_entry)?;
        let pointer = context.ptr_type(AddressSpace::default());
        let size_type = context.ptr_sized_int_type(target, None);
        let index_bits = pointer_index_bits(target)?;
        let max_buffer_bytes = if index_bits >= 64 {
            i64::MAX as u64
        } else {
            (1_u64 << (index_bits - 1)) - 1
        };
        let malloc = requirements.needs_allocation.then(|| {
            llvm.add_function("malloc", pointer.fn_type(&[size_type.into()], false), None)
        });
        let abort = requirements.needs_abort.then(|| {
            let function =
                llvm.add_function("abort", context.void_type().fn_type(&[], false), None);
            add_noreturn_attribute(context, function);
            function
        });
        let write = requirements.needs_print.then(|| {
            llvm.add_function(
                "write",
                size_type.fn_type(
                    &[context.i32_type().into(), pointer.into(), size_type.into()],
                    false,
                ),
                None,
            )
        });
        let free = requirements.needs_free.then(|| {
            llvm.add_function(
                "free",
                context.void_type().fn_type(&[pointer.into()], false),
                None,
            )
        });
        let zst_sentinel = if requirements.container_allocations.is_empty() {
            None
        } else {
            let alignment = requirements
                .container_allocations
                .iter()
                .map(|ty| types.container_layout(*ty))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|layout| layout.element_alignment)
                .max()
                .unwrap_or(1);
            let global = llvm.add_global(context.i8_type(), None, "koven.zst.sentinel");
            global.set_linkage(Linkage::Private);
            global.set_constant(true);
            global.set_initializer(&context.i8_type().const_zero());
            global.set_alignment(alignment.max(1));
            Some(global.as_pointer_value())
        };

        let mut allocation_sizes = BTreeMap::new();
        for owner in requirements.allocated_owners {
            let payload = module.heap_payload(owner).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("heap owner 缺少已定义 payload".to_owned())
            })?;
            let size = if matches!(
                module.type_kind(payload),
                Some(SsaTypeKind::TaggedUnion { .. })
            ) {
                // Box<enum> stores the already-validated tagged value, with no extra wrapper.
                target.get_store_size(&types.basic_type(payload)?)
            } else {
                types.aggregate_layout(target, payload)?.store_size
            }
            .max(1);
            if size > max_buffer_bytes {
                return Err(LlvmAdapterError::Unsupported(
                    "heap allocation exceeds pointer index range".to_owned(),
                ));
            }
            allocation_sizes.insert(owner, size);
        }
        let mut shared_allocation_sizes = BTreeMap::new();
        for owner in requirements.shared_allocated_owners {
            let size = types.shared_control_size(owner)?.max(1);
            if size > max_buffer_bytes {
                return Err(LlvmAdapterError::Unsupported(
                    "shared allocation exceeds pointer index range".to_owned(),
                ));
            }
            shared_allocation_sizes.insert(owner, size);
        }

        let mut drop_functions = BTreeMap::new();
        for ty in &requirements.drop_types {
            let parameter = types.basic_type(*ty)?;
            let function = llvm.add_function(
                &format!("koven.drop.t{}", ty.index()),
                context.void_type().fn_type(&[parameter.into()], false),
                Some(Linkage::Internal),
            );
            drop_functions.insert(*ty, function);
        }

        let runtime = Self {
            context,
            size_type,
            max_buffer_bytes,
            malloc,
            abort,
            write,
            free,
            allocation_sizes,
            shared_allocation_sizes,
            zst_sentinel,
            drop_functions,
        };
        Ok(runtime)
    }

    pub(super) fn free(&self) -> Option<FunctionValue<'ctx>> {
        self.free
    }

    pub(super) fn allocate(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        owner: SsaTypeId,
        payload: BasicValueEnum<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let size = self.allocation_sizes.get(&owner).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("heap allocation 缺少 target-derived size".to_owned())
        })?;
        let malloc = self
            .malloc
            .ok_or_else(|| LlvmAdapterError::Build("malloc 未声明".to_owned()))?;
        let call = builder.build_call(
            malloc,
            &[BasicMetadataValueEnum::from(
                self.size_type.const_int(size, false),
            )],
            name,
        )?;
        let allocation = match call.try_as_basic_value() {
            ValueKind::Basic(BasicValueEnum::PointerValue(pointer)) => pointer,
            _ => {
                return Err(LlvmAdapterError::Build(
                    "malloc 未返回 LLVM pointer".to_owned(),
                ));
            }
        };
        let failed = builder.build_is_null(allocation, &format!("{name}.failed"))?;
        let oom = self
            .context
            .append_basic_block(function, &format!("{name}.oom"));
        let initialized = self
            .context
            .append_basic_block(function, &format!("{name}.initialized"));
        builder.build_conditional_branch(failed, oom, initialized)?;

        builder.position_at_end(oom);
        builder.build_call(
            self.abort
                .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?,
            &[],
            "",
        )?;
        builder.build_unreachable()?;

        builder.position_at_end(initialized);
        builder.build_store(allocation, payload)?;
        Ok(allocation)
    }

    pub(super) fn emit_drop(
        &self,
        builder: &Builder<'ctx>,
        ty: SsaTypeId,
        value: BasicValueEnum<'ctx>,
    ) -> Result<(), LlvmAdapterError> {
        let function = self.drop_functions.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::Unsupported("MoveOnly 类型缺少 LLVM drop glue".to_owned())
        })?;
        builder.build_call(function, &[BasicMetadataValueEnum::from(value)], "")?;
        Ok(())
    }

    pub(super) fn allocate_shared(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        types: &TypeMap<'ctx>,
        owner: SsaTypeId,
        payload: BasicValueEnum<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let size = self
            .shared_allocation_sizes
            .get(&owner)
            .copied()
            .ok_or_else(|| {
                LlvmAdapterError::InvalidSsa(
                    "shared allocation 缺少 target-derived size".to_owned(),
                )
            })?;
        let malloc = self
            .malloc
            .ok_or_else(|| LlvmAdapterError::Build("malloc 未声明".to_owned()))?;
        let allocation = match builder
            .build_call(
                malloc,
                &[BasicMetadataValueEnum::from(
                    self.size_type.const_int(size, false),
                )],
                name,
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
        let failed = builder.build_is_null(allocation, &format!("{name}.failed"))?;
        self.abort_if(builder, function, failed, name)?;
        let control = types.shared_control(owner)?;
        let strong = builder.build_struct_gep(control, allocation, 0, &format!("{name}.strong"))?;
        builder.build_store(strong, self.size_type.const_int(1, false))?;
        let value = builder.build_struct_gep(control, allocation, 1, &format!("{name}.payload"))?;
        builder.build_store(value, payload)?;
        Ok(allocation)
    }

    pub(super) fn retain_shared(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        types: &TypeMap<'ctx>,
        owner_type: SsaTypeId,
        owner: PointerValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let control = types.shared_control(owner_type)?;
        let strong = builder.build_struct_gep(control, owner, 0, &format!("{name}.strong"))?;
        let current = builder
            .build_load(self.size_type, strong, &format!("{name}.current"))?
            .into_int_value();
        let overflow = builder.build_int_compare(
            IntPredicate::EQ,
            current,
            self.size_type.const_all_ones(),
            &format!("{name}.overflow"),
        )?;
        self.abort_if(builder, function, overflow, name)?;
        let next = builder.build_int_add(
            current,
            self.size_type.const_int(1, false),
            &format!("{name}.next"),
        )?;
        builder.build_store(strong, next)?;
        Ok(owner)
    }

    pub(super) fn shared_payload_place(
        &self,
        builder: &Builder<'ctx>,
        types: &TypeMap<'ctx>,
        owner_type: SsaTypeId,
        owner: PointerValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        builder
            .build_struct_gep(types.shared_control(owner_type)?, owner, 1, name)
            .map_err(Into::into)
    }

    pub(super) fn emit_abort(&self, builder: &Builder<'ctx>) -> Result<(), LlvmAdapterError> {
        builder.build_call(
            self.abort
                .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?,
            &[],
            "",
        )?;
        builder.build_unreachable()?;
        Ok(())
    }

    pub(super) fn emit_print_literal(
        &self,
        llvm: &LlvmModule<'ctx>,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        bytes: &[u8],
        name: &str,
    ) -> Result<(), LlvmAdapterError> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| LlvmAdapterError::Build("stdout literal 长度超出 u64".to_owned()))?;
        let constant = self.context.const_string(bytes, false);
        let global = llvm.add_global(constant.get_type(), None, &format!("{name}.bytes"));
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&constant);
        let call = builder.build_call(
            self.write
                .ok_or_else(|| LlvmAdapterError::Build("write 未声明".to_owned()))?,
            &[
                BasicMetadataValueEnum::from(self.context.i32_type().const_int(1, false)),
                BasicMetadataValueEnum::from(global.as_pointer_value()),
                BasicMetadataValueEnum::from(self.size_type.const_int(length, false)),
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
            self.size_type.const_int(length, false),
            &format!("{name}.incomplete"),
        )?;
        self.abort_if(builder, function, incomplete, name)
    }

    pub(super) fn size_type(&self) -> IntType<'ctx> {
        self.size_type
    }

    pub(super) fn container_int_to_size(
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
        self.abort_if(builder, function, negative, name)?;
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
                self.abort_if(builder, function, too_large, name)?;
                Ok(builder.build_int_truncate(length, self.size_type, &format!("{name}.size"))?)
            }
        }
    }

    pub(super) fn allocate_buffer(
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
        let abort = self
            .abort
            .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?;
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
        builder.build_call(abort, &[], "")?;
        builder.build_unreachable()?;

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

    pub(super) fn abort_if(
        &self,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        condition: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<(), LlvmAdapterError> {
        let failed = self
            .context
            .append_basic_block(function, &format!("{name}.abort"));
        let valid = self
            .context
            .append_basic_block(function, &format!("{name}.valid"));
        builder.build_conditional_branch(condition, failed, valid)?;
        builder.position_at_end(failed);
        builder.build_call(
            self.abort
                .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?,
            &[],
            "",
        )?;
        builder.build_unreachable()?;
        builder.position_at_end(valid);
        Ok(())
    }

    pub(super) fn define_drop_functions(
        &self,
        module: &Module,
        types: &TypeMap<'ctx>,
        functions: &BTreeMap<FunctionId, FunctionValue<'ctx>>,
    ) -> Result<(), LlvmAdapterError> {
        for (ty, function) in &self.drop_functions {
            let entry = self.context.append_basic_block(*function, "entry");
            let builder = self.context.create_builder();
            builder.position_at_end(entry);
            let value = function
                .get_first_param()
                .ok_or_else(|| LlvmAdapterError::Build("drop glue 缺少 owner 参数".to_owned()))?;
            match module.type_kind(*ty) {
                Some(SsaTypeKind::Aggregate { fields, .. }) => {
                    let aggregate = value.into_struct_value();
                    for (index, field) in fields.iter().enumerate().rev() {
                        if module.type_ownership(*field) == Some(Ownership::MoveOnly) {
                            let value = builder.build_extract_value(
                                aggregate,
                                index as u32,
                                &format!("field{index}"),
                            )?;
                            self.emit_drop(&builder, *field, value)?;
                        }
                    }
                }
                Some(SsaTypeKind::TaggedUnion { variants, .. }) => {
                    let tagged_value = value.into_struct_value();
                    let tag = builder
                        .build_extract_value(tagged_value, 0, "tag")?
                        .into_int_value();
                    let done = self.context.append_basic_block(*function, "done");
                    let cases = variants
                        .iter()
                        .enumerate()
                        .map(|(index, _)| {
                            (
                                self.context.i32_type().const_int(index as u64, false),
                                self.context
                                    .append_basic_block(*function, &format!("case{index}")),
                            )
                        })
                        .collect::<Vec<_>>();
                    builder.build_switch(tag, done, &cases)?;
                    for (index, (payload, (_, block))) in variants.iter().zip(&cases).enumerate() {
                        builder.position_at_end(*block);
                        let payload_ty = *payload;
                        if module.type_ownership(payload_ty) == Some(Ownership::MoveOnly) {
                            let place = tagged::payload_place(
                                &builder,
                                types,
                                *ty,
                                index,
                                tagged_value,
                                &format!("case{index}.payload"),
                            )?;
                            let payload = builder.build_load(
                                types.basic_type(payload_ty)?,
                                place,
                                &format!("case{index}.value"),
                            )?;
                            self.emit_drop(&builder, payload_ty, payload)?;
                        }
                        builder.build_unconditional_branch(done)?;
                    }
                    builder.position_at_end(done);
                }
                Some(SsaTypeKind::HeapOwner { payload, .. }) => {
                    if let Some(deinit) = module.deinit(*ty) {
                        let deinit = functions.get(&deinit).copied().ok_or_else(|| {
                            LlvmAdapterError::InvalidSsa("deinit body is not declared".to_owned())
                        })?;
                        // The existing Borrow ABI passes a pointer to the owner handle, not
                        // its payload. The readonly body returns before any field is released.
                        let receiver =
                            builder.build_alloca(types.basic_type(*ty)?, "deinit.receiver")?;
                        builder.build_store(receiver, value)?;
                        builder.build_call(deinit, &[receiver.into()], "")?;
                    }
                    let payload = payload.ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "drop glue 的 heap owner 尚未定义 payload".to_owned(),
                        )
                    })?;
                    if module.type_ownership(payload) == Some(Ownership::MoveOnly) {
                        let owner = value.into_pointer_value();
                        let payload_value =
                            builder.build_load(types.basic_type(payload)?, owner, "payload")?;
                        self.emit_drop(&builder, payload, payload_value)?;
                    }
                    builder.build_call(
                        self.free
                            .ok_or_else(|| LlvmAdapterError::Build("free 未声明".to_owned()))?,
                        &[BasicMetadataValueEnum::from(value)],
                        "",
                    )?;
                }
                Some(SsaTypeKind::SharedOwner { payload, .. }) => {
                    let payload = payload.ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "drop glue 的 shared owner 尚未定义 payload".to_owned(),
                        )
                    })?;
                    let owner = value.into_pointer_value();
                    let control = types.shared_control(*ty)?;
                    let strong = builder.build_struct_gep(control, owner, 0, "strong")?;
                    let current = builder
                        .build_load(self.size_type, strong, "strong.current")?
                        .into_int_value();
                    let next = builder.build_int_sub(
                        current,
                        self.size_type.const_int(1, false),
                        "strong.next",
                    )?;
                    builder.build_store(strong, next)?;
                    let release = self.context.append_basic_block(*function, "release");
                    let done = self.context.append_basic_block(*function, "done");
                    let last = builder.build_int_compare(
                        IntPredicate::EQ,
                        next,
                        self.size_type.const_zero(),
                        "strong.last",
                    )?;
                    builder.build_conditional_branch(last, release, done)?;
                    builder.position_at_end(release);
                    if module.type_ownership(payload) == Some(Ownership::MoveOnly) {
                        let place = builder.build_struct_gep(control, owner, 1, "payload.place")?;
                        let payload_value =
                            builder.build_load(types.basic_type(payload)?, place, "payload")?;
                        self.emit_drop(&builder, payload, payload_value)?;
                    }
                    builder.build_call(
                        self.free
                            .ok_or_else(|| LlvmAdapterError::Build("free 未声明".to_owned()))?,
                        &[BasicMetadataValueEnum::from(owner)],
                        "",
                    )?;
                    builder.build_unconditional_branch(done)?;
                    builder.position_at_end(done);
                }
                Some(SsaTypeKind::NullableHandle { inner }) => {
                    let owner = value.into_pointer_value();
                    let drop_inner = self.context.append_basic_block(*function, "non_null");
                    let done = self.context.append_basic_block(*function, "done");
                    let is_null = builder.build_is_null(owner, "is_null")?;
                    builder.build_conditional_branch(is_null, done, drop_inner)?;
                    builder.position_at_end(drop_inner);
                    self.emit_drop(&builder, *inner, owner.into())?;
                    builder.build_unconditional_branch(done)?;
                    builder.position_at_end(done);
                }
                Some(SsaTypeKind::SequentialContainer { element, .. }) => {
                    self.define_container_drop(
                        module,
                        types,
                        &builder,
                        *function,
                        *ty,
                        *element,
                        value.into_struct_value(),
                    )?;
                }
                Some(SsaTypeKind::StringOwner) => {
                    let string = value.into_struct_value();
                    let bytes = builder
                        .build_extract_value(string, 0, "bytes")?
                        .into_pointer_value();
                    let capacity = builder
                        .build_extract_value(string, 2, "capacity")?
                        .into_int_value();
                    let release = self.context.append_basic_block(*function, "release");
                    let done = self.context.append_basic_block(*function, "done");
                    let owns_buffer = builder.build_int_compare(
                        IntPredicate::NE,
                        capacity,
                        self.size_type.const_zero(),
                        "owns_buffer",
                    )?;
                    builder.build_conditional_branch(owns_buffer, release, done)?;
                    builder.position_at_end(release);
                    builder.build_call(
                        self.free
                            .ok_or_else(|| LlvmAdapterError::Build("free 未声明".to_owned()))?,
                        &[BasicMetadataValueEnum::from(bytes)],
                        "",
                    )?;
                    builder.build_unconditional_branch(done)?;
                    builder.position_at_end(done);
                }
                Some(SsaTypeKind::ConcreteClosure {
                    environment,
                    captures,
                    ..
                }) => {
                    let closure = value.into_struct_value();
                    let environment_value = builder
                        .build_extract_value(closure, 1, "environment")?
                        .into_struct_value();
                    for (index, capture) in captures.iter().enumerate().rev() {
                        if capture.mode == ClosureCaptureMode::Owned
                            && module.type_ownership(capture.ty) == Some(Ownership::MoveOnly)
                        {
                            let captured = builder.build_extract_value(
                                environment_value,
                                index as u32,
                                &format!("capture{index}"),
                            )?;
                            self.emit_drop(&builder, capture.ty, captured)?;
                        }
                    }
                    let _ = environment;
                }
                Some(SsaTypeKind::ZeroSized { .. } | SsaTypeKind::FunctionPointer { .. }) => {}
                _ => {
                    return Err(LlvmAdapterError::Unsupported(
                        "当前 LLVM drop glue 只支持 aggregate 与 heap owner".to_owned(),
                    ));
                }
            }
            builder.build_return(None)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn define_container_drop(
        &self,
        module: &Module,
        types: &TypeMap<'ctx>,
        builder: &Builder<'ctx>,
        function: FunctionValue<'ctx>,
        container: SsaTypeId,
        element: SsaTypeId,
        owner: inkwell::values::StructValue<'ctx>,
    ) -> Result<(), LlvmAdapterError> {
        let layout = types.container_layout(container)?;
        let buffer = builder
            .build_extract_value(owner, 0, "buffer")?
            .into_pointer_value();
        let length = builder
            .build_extract_value(owner, 1, "length")?
            .into_int_value();
        if module.type_ownership(element) == Some(Ownership::MoveOnly) {
            let preheader = builder.get_insert_block().ok_or_else(|| {
                LlvmAdapterError::Build("container drop 缺少 preheader".to_owned())
            })?;
            let loop_header = self.context.append_basic_block(function, "drop.loop");
            let loop_body = self.context.append_basic_block(function, "drop.body");
            let released = self.context.append_basic_block(function, "drop.released");
            builder.build_unconditional_branch(loop_header)?;

            builder.position_at_end(loop_header);
            let remaining_phi = builder.build_phi(self.size_type, "remaining")?;
            remaining_phi.add_incoming(&[(&length, preheader)]);
            let remaining = remaining_phi.as_basic_value().into_int_value();
            let has_element = builder.build_int_compare(
                IntPredicate::NE,
                remaining,
                self.size_type.const_zero(),
                "has_element",
            )?;
            builder.build_conditional_branch(has_element, loop_body, released)?;

            builder.position_at_end(loop_body);
            let index = builder.build_int_sub(
                remaining,
                self.size_type.const_int(1, false),
                "drop.index",
            )?;
            let value = if layout.stride == 0 {
                layout.element.const_zero()
            } else {
                // SAFETY: remaining is non-zero and starts at logical length; decrementing before
                // addressing visits exactly length-1 down to zero inside the allocated buffer.
                // Non-inbounds GEP uses the allocation-checked physical offset.
                let slot =
                    unsafe { builder.build_gep(layout.element, buffer, &[index], "drop.slot")? };
                builder.build_load(layout.element, slot, "drop.element")?
            };
            self.emit_drop(builder, element, value)?;
            let backedge = builder.get_insert_block().ok_or_else(|| {
                LlvmAdapterError::Build("container drop 缺少 backedge".to_owned())
            })?;
            builder.build_unconditional_branch(loop_header)?;
            remaining_phi.add_incoming(&[(&index, backedge)]);
            builder.position_at_end(released);
        }
        if layout.stride != 0 {
            let free_buffer = self.context.append_basic_block(function, "drop.free");
            let done = self.context.append_basic_block(function, "drop.done");
            let has_allocation = builder.build_int_compare(
                IntPredicate::NE,
                length,
                self.size_type.const_zero(),
                "has_allocation",
            )?;
            builder.build_conditional_branch(has_allocation, free_buffer, done)?;
            builder.position_at_end(free_buffer);
            builder.build_call(
                self.free
                    .ok_or_else(|| LlvmAdapterError::Build("free 未声明".to_owned()))?,
                &[BasicMetadataValueEnum::from(buffer)],
                "",
            )?;
            builder.build_unconditional_branch(done)?;
            builder.position_at_end(done);
        }
        Ok(())
    }
}

fn pointer_index_bits(target: &TargetData) -> Result<u32, LlvmAdapterError> {
    let pointer_bits = target
        .get_pointer_byte_size(Some(AddressSpace::default()))
        .checked_mul(8)
        .ok_or_else(|| LlvmAdapterError::Target("target pointer width overflow".to_owned()))?;
    let layout = target.get_data_layout();
    let layout = layout
        .as_str()
        .to_str()
        .map_err(|_| LlvmAdapterError::Target("target DataLayout is not UTF-8".to_owned()))?;
    let index_bits = layout
        .split('-')
        .rfind(|part| part.starts_with("p:") || part.starts_with("p0:"))
        .and_then(|part| part.split(':').nth(4))
        .map(str::parse::<u32>)
        .transpose()
        .map_err(|_| LlvmAdapterError::Target("invalid pointer index width".to_owned()))?
        .unwrap_or(pointer_bits);
    if index_bits == 0 || index_bits > pointer_bits || index_bits > 64 {
        return Err(LlvmAdapterError::Target(
            "unsupported pointer index width".to_owned(),
        ));
    }
    Ok(index_bits)
}

struct RuntimeRequirements {
    needs_allocation: bool,
    needs_abort: bool,
    needs_print: bool,
    needs_free: bool,
    allocated_owners: BTreeSet<SsaTypeId>,
    shared_allocated_owners: BTreeSet<SsaTypeId>,
    container_allocations: BTreeSet<SsaTypeId>,
    drop_types: BTreeSet<SsaTypeId>,
}

impl RuntimeRequirements {
    fn collect(
        module: &Module,
        native_entry: Option<NativeEntryPlan>,
    ) -> Result<Self, LlvmAdapterError> {
        let mut requirements = Self {
            needs_allocation: false,
            needs_abort: false,
            needs_print: false,
            needs_free: false,
            allocated_owners: BTreeSet::new(),
            shared_allocated_owners: BTreeSet::new(),
            container_allocations: BTreeSet::new(),
            drop_types: BTreeSet::new(),
        };
        for function in &module.functions {
            for instruction in &function.instructions {
                match instruction.operation {
                    Operation::HeapAllocate { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_abort = true;
                        requirements.allocated_owners.insert(owner);
                    }
                    Operation::SharedAllocate { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_abort = true;
                        requirements.shared_allocated_owners.insert(owner);
                    }
                    Operation::SharedRetain { .. } => {
                        requirements.needs_abort = true;
                    }
                    Operation::HeapFieldReplace {
                        receiver, field, ..
                    } => {
                        let owner = match function
                            .entity(EntityId::Loan(receiver))
                            .map(|data| data.ty)
                        {
                            Some(EntityType::Loan { target, .. }) => target,
                            _ => {
                                return Err(LlvmAdapterError::InvalidSsa(
                                    "heap field replace receiver 缺少 loan target".to_owned(),
                                ));
                            }
                        };
                        let field = module
                            .heap_payload(owner)
                            .and_then(|payload| module.aggregate_fields(payload))
                            .and_then(|fields| fields.get(field).copied())
                            .ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa(
                                    "heap field replace 缺少 payload field type".to_owned(),
                                )
                            })?;
                        requirements.collect_drop_type(module, field)?;
                    }
                    Operation::InlineFieldReplace {
                        receiver, field, ..
                    } => {
                        let owner = match function
                            .entity(EntityId::Loan(receiver))
                            .map(|data| data.ty)
                        {
                            Some(EntityType::Loan { target, .. }) => target,
                            _ => {
                                return Err(LlvmAdapterError::InvalidSsa(
                                    "inline field replace receiver 缺少 loan target".to_owned(),
                                ));
                            }
                        };
                        let field = module
                            .aggregate_fields(owner)
                            .and_then(|fields| fields.get(field).copied())
                            .ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa(
                                    "inline field replace 缺少 field type".to_owned(),
                                )
                            })?;
                        requirements.collect_drop_type(module, field)?;
                    }
                    Operation::PrintLiteral { .. } => {
                        requirements.needs_print = true;
                        requirements.needs_abort = true;
                    }
                    Operation::StringConcat { .. } | Operation::StringClone { .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_abort = true;
                    }
                    Operation::PrintString { .. } => {
                        requirements.needs_print = true;
                        requirements.needs_abort = true;
                    }
                    Operation::ContainerConstruct { container, .. }
                    | Operation::ContainerGenerate { container, .. }
                    | Operation::ContainerGenerateBorrowed { container, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_abort = true;
                        requirements.container_allocations.insert(container);
                    }
                    Operation::ContainerElementPlace { .. } => {
                        requirements.needs_abort = true;
                    }
                    Operation::ContainerAppend { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_free = true;
                        requirements.needs_abort = true;
                        let container =
                            match function.entity(EntityId::Value(owner)).map(|data| data.ty) {
                                Some(EntityType::Value(ty)) => ty,
                                _ => {
                                    return Err(LlvmAdapterError::InvalidSsa(
                                        "append owner 缺少 value type".to_owned(),
                                    ));
                                }
                            };
                        requirements.container_allocations.insert(container);
                        let (_, element) =
                            module.sequential_container(container).ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa("append owner 不是顺序容器".to_owned())
                            })?;
                        requirements.collect_drop_type(module, element)?;
                    }
                    Operation::ContainerReplace { owner, .. } => {
                        requirements.needs_abort = true;
                        let container =
                            match function.entity(EntityId::Value(owner)).map(|data| data.ty) {
                                Some(EntityType::Value(ty)) => ty,
                                _ => {
                                    return Err(LlvmAdapterError::InvalidSsa(
                                        "replace owner 缺少 value type".to_owned(),
                                    ));
                                }
                            };
                        let (_, element) =
                            module.sequential_container(container).ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa(
                                    "replace owner 不是顺序容器".to_owned(),
                                )
                            })?;
                        requirements.collect_drop_type(module, element)?;
                    }
                    Operation::Drop { owner } => {
                        let ty = match function.entity(EntityId::Value(owner)).map(|data| data.ty) {
                            Some(EntityType::Value(ty)) => ty,
                            _ => {
                                return Err(LlvmAdapterError::InvalidSsa(
                                    "drop operand 缺少 value type".to_owned(),
                                ));
                            }
                        };
                        requirements.collect_drop_type(module, ty)?;
                    }
                    _ => {}
                }
            }
            if function.blocks.iter().any(|block| {
                matches!(
                    block.terminator.as_ref().map(|terminator| &terminator.kind),
                    Some(TerminatorKind::Abort)
                )
            }) {
                requirements.needs_abort = true;
            }
        }
        if let Some(NativeEntryPlan::BorrowedArguments { arguments, .. }) = native_entry {
            requirements.needs_allocation = true;
            requirements.needs_abort = true;
            requirements.container_allocations.insert(arguments);
            requirements.collect_drop_type(module, arguments)?;
        }
        Ok(requirements)
    }

    fn collect_drop_type(
        &mut self,
        module: &Module,
        ty: SsaTypeId,
    ) -> Result<(), LlvmAdapterError> {
        if module.type_ownership(ty) != Some(Ownership::MoveOnly) || !self.drop_types.insert(ty) {
            return Ok(());
        }
        match module.type_kind(ty) {
            Some(SsaTypeKind::Aggregate { fields, .. }) => {
                for field in fields {
                    self.collect_drop_type(module, *field)?;
                }
            }
            Some(SsaTypeKind::TaggedUnion { variants, .. }) => {
                for payload in variants {
                    self.collect_drop_type(module, *payload)?;
                }
            }
            Some(SsaTypeKind::HeapOwner { payload, .. }) => {
                self.needs_free = true;
                let payload = payload.ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa(
                        "drop operand 使用尚未定义 payload 的 heap owner".to_owned(),
                    )
                })?;
                self.collect_drop_type(module, payload)?;
            }
            Some(SsaTypeKind::SharedOwner { payload, .. }) => {
                self.needs_free = true;
                let payload = payload.ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa(
                        "drop operand 使用尚未定义 payload 的 shared owner".to_owned(),
                    )
                })?;
                self.collect_drop_type(module, payload)?;
            }
            Some(SsaTypeKind::NullableHandle { inner }) => {
                self.collect_drop_type(module, *inner)?;
            }
            Some(SsaTypeKind::SequentialContainer { element, .. }) => {
                self.needs_free = true;
                self.collect_drop_type(module, *element)?;
            }
            Some(SsaTypeKind::StringOwner) => self.needs_free = true,
            Some(SsaTypeKind::Opaque { .. }) => {
                return Err(LlvmAdapterError::Unsupported(
                    "opaque MoveOnly 类型没有可生成的 drop glue".to_owned(),
                ));
            }
            Some(SsaTypeKind::ZeroSized { .. } | SsaTypeKind::FunctionPointer { .. }) => {}
            Some(SsaTypeKind::ConcreteClosure { captures, .. }) => {
                for capture in captures {
                    if capture.mode == ClosureCaptureMode::Owned {
                        self.collect_drop_type(module, capture.ty)?;
                    }
                }
            }
            Some(SsaTypeKind::SharedReference { .. }) => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "shared reference cannot enter owned drop glue collection".to_owned(),
                ));
            }
            Some(
                SsaTypeKind::Unit
                | SsaTypeKind::Boolean
                | SsaTypeKind::Char
                | SsaTypeKind::Integer { .. },
            )
            | None => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "Copyable 或未知类型进入 drop glue 收集".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

fn add_noreturn_attribute(context: &Context, function: FunctionValue<'_>) {
    let kind = Attribute::get_named_enum_kind_id("noreturn");
    if kind != 0 {
        function.add_attribute(
            AttributeLoc::Function,
            context.create_enum_attribute(kind, 0),
        );
    }
}
