//! Koven heap owner 的系统分配 ABI 与类型定向 drop glue。

use std::collections::{BTreeMap, BTreeSet};

use inkwell::{
    AddressSpace,
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    targets::TargetData,
    types::IntType,
    values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, ValueKind},
};

use crate::ssa::model::{
    EntityId, EntityType, Module, Operation, Ownership, SsaTypeId, SsaTypeKind,
};

use super::{LlvmAdapterError, type_map::TypeMap};

pub(super) struct RuntimeAbi<'ctx> {
    context: &'ctx Context,
    size_type: IntType<'ctx>,
    malloc: Option<FunctionValue<'ctx>>,
    abort: Option<FunctionValue<'ctx>>,
    free: Option<FunctionValue<'ctx>>,
    allocation_sizes: BTreeMap<SsaTypeId, u64>,
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
        let abort = requirements.needs_allocation.then(|| {
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
}

struct RuntimeRequirements {
    needs_allocation: bool,
    needs_free: bool,
    allocated_owners: BTreeSet<SsaTypeId>,
    drop_types: BTreeSet<SsaTypeId>,
}

impl RuntimeRequirements {
    fn collect(module: &Module) -> Result<Self, LlvmAdapterError> {
        let mut requirements = Self {
            needs_allocation: false,
            needs_free: false,
            allocated_owners: BTreeSet::new(),
            drop_types: BTreeSet::new(),
        };
        for function in &module.functions {
            for instruction in &function.instructions {
                match instruction.operation {
                    Operation::HeapAllocate { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.allocated_owners.insert(owner);
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
            Some(SsaTypeKind::Opaque { .. }) => {
                return Err(LlvmAdapterError::Unsupported(
                    "opaque MoveOnly 类型没有可生成的 drop glue".to_owned(),
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
