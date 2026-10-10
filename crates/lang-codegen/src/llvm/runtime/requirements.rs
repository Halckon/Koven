//! Collect exact runtime dependencies from verified SSA without emitting IR.
use super::*;

pub(super) struct RuntimeRequirements {
    pub(super) needs_allocation: bool,
    pub(super) needs_abort: bool,
    pub(super) needs_print: bool,
    pub(super) needs_free: bool,
    pub(super) allocated_owners: BTreeSet<SsaTypeId>,
    pub(super) shared_allocated_owners: BTreeSet<SsaTypeId>,
    pub(super) container_allocations: BTreeSet<SsaTypeId>,
    pub(super) drop_types: BTreeSet<SsaTypeId>,
}

impl RuntimeRequirements {
    pub(super) fn collect(
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
                    Operation::RangeConstruct { .. } => {
                        requirements.needs_abort = true;
                    }
                    Operation::ContainerElementPlace { .. }
                    | Operation::RangeElementPlace { .. } => {
                        requirements.needs_abort = true;
                    }
                    Operation::ContainerAppend { owner, .. }
                    | Operation::ContainerInsertAt { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_free = true;
                        requirements.needs_abort = true;
                        let Some(EntityType::Value(container)) =
                            function.entity(EntityId::Value(owner)).map(|data| data.ty)
                        else {
                            return Err(LlvmAdapterError::InvalidSsa(
                                "append owner 缺少 value type".to_owned(),
                            ));
                        };
                        requirements.container_allocations.insert(container);
                        let (_, element) =
                            module.sequential_container(container).ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa("append owner 不是顺序容器".to_owned())
                            })?;
                        requirements.collect_drop_type(module, element)?;
                    }
                    Operation::ContainerClear { owner } => {
                        let container =
                            match function.entity(EntityId::Value(owner)).map(|data| data.ty) {
                                Some(EntityType::Value(ty)) => ty,
                                _ => {
                                    return Err(LlvmAdapterError::InvalidSsa(
                                        "clear owner 缺少 value type".to_owned(),
                                    ));
                                }
                            };
                        let (_, element) =
                            module.sequential_container(container).ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa("clear owner 不是顺序容器".to_owned())
                            })?;
                        requirements.collect_drop_type(module, element)?;
                    }
                    Operation::ContainerRemoveAt { .. }
                    | Operation::ContainerRemoveFirst { .. }
                    | Operation::ContainerRemoveLast { .. } => {
                        requirements.needs_abort = true;
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
                    Operation::MapConstruct { .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_free = true;
                        requirements.needs_abort = true;
                    }
                    Operation::MapPut { owner, .. } | Operation::MapRemove { owner, .. } => {
                        requirements.needs_allocation = true;
                        requirements.needs_free = true;
                        requirements.needs_abort = true;
                        let Some(EntityType::Value(container)) =
                            function.entity(EntityId::Value(owner)).map(|data| data.ty)
                        else {
                            return Err(LlvmAdapterError::InvalidSsa(
                                "map mutation owner 缺少 value type".to_owned(),
                            ));
                        };
                        let (_, key, value) = module.map_container(container).ok_or_else(|| {
                            LlvmAdapterError::InvalidSsa("map mutation owner 不是 Map".to_owned())
                        })?;
                        // put drops the old entry; remove drops only its key and returns V.
                        // These dependencies exist even if every continuation aborts without Drop.
                        requirements.collect_drop_type(module, key)?;
                        if matches!(instruction.operation, Operation::MapPut { .. }) {
                            requirements.collect_drop_type(module, value)?;
                        }
                    }
                    Operation::MapGet { .. } | Operation::MapResultUnwrap { .. } => {
                        requirements.needs_abort = true;
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
            Some(SsaTypeKind::MapContainer { key, value, .. }) => {
                self.needs_free = true;
                self.collect_drop_type(module, *key)?;
                self.collect_drop_type(module, *value)?;
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
            Some(SsaTypeKind::RangeView { .. } | SsaTypeKind::SharedReference { .. }) => {
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
