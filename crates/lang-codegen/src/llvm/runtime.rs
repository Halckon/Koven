//! Koven heap owner 的系统分配 ABI 与类型定向 drop glue。

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
    values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, ValueKind},
};

use crate::ssa::model::{
    ClosureCaptureMode, EntityId, EntityType, Module, Operation, Ownership, SsaTypeId, SsaTypeKind,
};

use super::{LlvmAdapterError, type_map::TypeMap};

pub(super) struct RuntimeAbi<'ctx> {
    context: &'ctx Context,
    size_type: IntType<'ctx>,
    malloc: Option<FunctionValue<'ctx>>,
    abort: Option<FunctionValue<'ctx>>,
    free: Option<FunctionValue<'ctx>>,
    allocation_sizes: BTreeMap<SsaTypeId, u64>,
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
    ) -> Result<Self, LlvmAdapterError> {
        let requirements = RuntimeRequirements::collect(module)?;
        let pointer = context.ptr_type(AddressSpace::default());
        let size_type = context.ptr_sized_int_type(target, None);
        let malloc = requirements.needs_allocation.then(|| {
            llvm.add_function("malloc", pointer.fn_type(&[size_type.into()], false), None)
        });
        let abort = requirements.needs_abort.then(|| {
            let function =
                llvm.add_function("abort", context.void_type().fn_type(&[], false), None);
            add_noreturn_attribute(context, function);
            function
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
            let size = types.aggregate_layout(target, payload)?.store_size.max(1);
            allocation_sizes.insert(owner, size);
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
            malloc,
            abort,
            free,
            allocation_sizes,
            zst_sentinel,
            drop_functions,
        };
        runtime.define_drop_functions(module, types)?;
        Ok(runtime)
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

    pub(super) fn size_type(&self) -> IntType<'ctx> {
        self.size_type
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
                "当前 target 的 size_t 宽度必须与 Koven Int 一致".to_owned(),
            ));
        }
        let abort = self
            .abort
            .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?;
        let sentinel = self
            .zst_sentinel
            .ok_or_else(|| LlvmAdapterError::Build("容器 sentinel 未声明".to_owned()))?;
        let failed = self
            .context
            .append_basic_block(function, &format!("{name}.abort"));
        let valid = self
            .context
            .append_basic_block(function, &format!("{name}.valid"));
        let negative = builder.build_int_compare(
            IntPredicate::SLT,
            length,
            length.get_type().const_zero(),
            &format!("{name}.negative"),
        )?;
        builder.build_conditional_branch(negative, failed, valid)?;

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

    fn define_drop_functions(
        &self,
        module: &Module,
        types: &TypeMap<'ctx>,
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
                Some(SsaTypeKind::HeapOwner { payload, .. }) => {
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
                let slot = unsafe {
                    builder.build_in_bounds_gep(layout.element, buffer, &[index], "drop.slot")?
                };
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

struct RuntimeRequirements {
    needs_allocation: bool,
    needs_abort: bool,
    needs_free: bool,
    allocated_owners: BTreeSet<SsaTypeId>,
    container_allocations: BTreeSet<SsaTypeId>,
    drop_types: BTreeSet<SsaTypeId>,
}

impl RuntimeRequirements {
    fn collect(module: &Module) -> Result<Self, LlvmAdapterError> {
        let mut requirements = Self {
            needs_allocation: false,
            needs_abort: false,
            needs_free: false,
            allocated_owners: BTreeSet::new(),
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
                    Operation::ContainerConstruct { container, .. }
                    | Operation::ContainerGenerate { container, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_abort = true;
                        requirements.container_allocations.insert(container);
                    }
                    Operation::ContainerElementPlace { .. } => {
                        requirements.needs_abort = true;
                    }
                    Operation::ContainerReplace { owner, .. } => {
                        requirements.needs_abort = true;
                        let container =
                            match function.entity(EntityId::Value(owner)).map(|data| data.ty) {
                                Some(EntityType::Value(ty)) => ty,
                                _ => {
                                    return Err(LlvmAdapterError::InvalidSsa(
                                        "container replace owner 缺少 value type".to_owned(),
                                    ));
                                }
                            };
                        let (_, element) =
                            module.sequential_container(container).ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa(
                                    "container replace owner 类型不是顺序容器".to_owned(),
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
            Some(SsaTypeKind::HeapOwner { payload, .. }) => {
                self.needs_free = true;
                let payload = payload.ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa(
                        "drop operand 使用尚未定义 payload 的 heap owner".to_owned(),
                    )
                })?;
                self.collect_drop_type(module, payload)?;
            }
            Some(SsaTypeKind::SequentialContainer { element, .. }) => {
                self.needs_free = true;
                self.collect_drop_type(module, *element)?;
            }
            Some(SsaTypeKind::Opaque { .. }) => {
                return Err(LlvmAdapterError::Unsupported(
                    "opaque MoveOnly 类型没有可生成的 drop glue".to_owned(),
                ));
            }
            Some(SsaTypeKind::ZeroSized { .. }) => {}
            Some(SsaTypeKind::FunctionPointer { .. }) => {}
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
            Some(SsaTypeKind::Unit | SsaTypeKind::Boolean | SsaTypeKind::Integer { .. }) | None => {
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
