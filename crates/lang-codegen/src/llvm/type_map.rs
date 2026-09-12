//! Target-aware LLVM type and aggregate layout mapping.

use std::collections::BTreeMap;

use inkwell::{
    AddressSpace,
    context::Context,
    targets::TargetData,
    types::{
        BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, IntType, PointerType,
        StructType, VoidType,
    },
};

use crate::ssa::model::{CallableSignature, EntityType, Module, SsaTypeId, SsaTypeKind};

use super::{LlvmAdapterError, layout::TargetLayoutPlan};

pub(super) struct TypeMap<'ctx> {
    void_type: VoidType<'ctx>,
    pointer_type: PointerType<'ctx>,
    types: BTreeMap<SsaTypeId, BasicTypeEnum<'ctx>>,
    aggregates: BTreeMap<SsaTypeId, StructType<'ctx>>,
    tagged_layouts: BTreeMap<SsaTypeId, TaggedLayout<'ctx>>,
    container_layouts: BTreeMap<SsaTypeId, ContainerLayout<'ctx>>,
    closure_layouts: BTreeMap<SsaTypeId, ClosureLayout<'ctx>>,
    shared_controls: BTreeMap<SsaTypeId, StructType<'ctx>>,
    shared_control_sizes: BTreeMap<SsaTypeId, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AggregateLayout {
    pub(super) store_size: u64,
    pub(super) abi_size: u64,
    pub(super) abi_alignment: u32,
    pub(super) field_offsets: Vec<u64>,
}

#[derive(Clone, Copy)]
pub(super) struct ContainerLayout<'ctx> {
    pub(super) header: StructType<'ctx>,
    pub(super) element: BasicTypeEnum<'ctx>,
    pub(super) stride: u64,
    pub(super) element_alignment: u32,
}

#[derive(Clone, Copy)]
pub(super) struct ClosureLayout<'ctx> {
    pub(super) value: StructType<'ctx>,
    pub(super) environment: StructType<'ctx>,
}

#[derive(Clone)]
pub(super) struct TaggedLayout<'ctx> {
    pub(super) value: StructType<'ctx>,
    pub(super) payloads: Vec<StructType<'ctx>>,
}

impl<'ctx> TypeMap<'ctx> {
    pub(super) fn lower(
        context: &'ctx Context,
        module: &Module,
        target: &TargetData,
    ) -> Result<Self, LlvmAdapterError> {
        // Validate the closed SSA type graph before creating any LLVM composite type. This keeps
        // oversized layouts out of opaque struct bodies, GEPs, and allocator lowering.
        let layout_plan = TargetLayoutPlan::build(context, module, target)?;
        let mut types = BTreeMap::new();
        let mut aggregates = BTreeMap::new();
        let mut containers = BTreeMap::new();
        let mut closures = BTreeMap::new();
        let mut tagged = BTreeMap::new();
        let mut shared_controls = BTreeMap::new();
        let pointer = context.ptr_type(AddressSpace::default());
        let size_type = context.ptr_sized_int_type(target, None);

        for (index, kind) in module.types.iter().enumerate() {
            let id = SsaTypeId {
                module: module.id,
                index,
            };
            let ty = match kind {
                SsaTypeKind::Boolean => Some(context.bool_type().into()),
                SsaTypeKind::Char => Some(context.i32_type().into()),
                SsaTypeKind::Integer { bits, .. } => Some(integer_type(context, *bits)?.into()),
                SsaTypeKind::Aggregate { .. } => {
                    let aggregate = context.opaque_struct_type(&format!("koven.t{index}"));
                    aggregates.insert(id, aggregate);
                    Some(aggregate.into())
                }
                SsaTypeKind::TaggedUnion { .. } => {
                    let value = context.opaque_struct_type(&format!("koven.enum.t{index}"));
                    tagged.insert(id, value);
                    Some(value.into())
                }
                SsaTypeKind::HeapOwner { .. } | SsaTypeKind::NullableHandle { .. } => {
                    Some(pointer.into())
                }
                SsaTypeKind::SharedOwner { .. } => {
                    let control = context.opaque_struct_type(&format!("koven.shared.t{index}"));
                    shared_controls.insert(id, control);
                    Some(pointer.into())
                }
                SsaTypeKind::SharedReference { .. } | SsaTypeKind::FunctionPointer { .. } => {
                    Some(pointer.into())
                }
                SsaTypeKind::SequentialContainer { kind, .. } => {
                    let container =
                        context.opaque_struct_type(&format!("koven.container.t{index}"));
                    let mut fields = vec![pointer.into(), size_type.into()];
                    if *kind == crate::ssa::model::SequentialContainerKind::MutableList {
                        fields.push(size_type.into());
                    }
                    container.set_body(&fields, false);
                    containers.insert(id, container);
                    Some(container.into())
                }
                SsaTypeKind::StringOwner => {
                    let string = context.opaque_struct_type(&format!("koven.string.t{index}"));
                    string.set_body(&[pointer.into(), size_type.into(), size_type.into()], false);
                    Some(string.into())
                }
                SsaTypeKind::ZeroSized { .. } => {
                    let zst = context.opaque_struct_type(&format!("koven.zst.t{index}"));
                    zst.set_body(&[], false);
                    Some(zst.into())
                }
                SsaTypeKind::ConcreteClosure { .. } => {
                    let closure = context.opaque_struct_type(&format!("koven.closure.t{index}"));
                    closures.insert(id, closure);
                    Some(closure.into())
                }
                SsaTypeKind::Unit | SsaTypeKind::Opaque { .. } => None,
            };
            if let Some(ty) = ty {
                types.insert(id, ty);
            }
        }

        let mut closure_layouts = BTreeMap::new();
        for (id, closure) in closures {
            let Some(SsaTypeKind::ConcreteClosure { environment, .. }) = module.type_kind(id)
            else {
                return Err(LlvmAdapterError::InvalidSsa(
                    "closure type missing concrete definition".to_owned(),
                ));
            };
            let environment = aggregates.get(environment).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa(
                    "closure environment must lower as an aggregate".to_owned(),
                )
            })?;
            closure.set_body(&[pointer.into(), environment.into()], false);
            closure_layouts.insert(
                id,
                ClosureLayout {
                    value: closure,
                    environment,
                },
            );
        }

        for (id, control) in &shared_controls {
            let payload = module.shared_payload(*id).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("shared owner 缺少 payload 定义".to_owned())
            })?;
            let payload = types.get(&payload).copied().ok_or_else(|| {
                LlvmAdapterError::Unsupported("shared owner payload 不具有 LLVM storage".to_owned())
            })?;
            control.set_body(&[size_type.into(), payload], false);
        }
        let shared_control_sizes = shared_controls
            .keys()
            .map(|owner| {
                layout_plan
                    .shared_control_layout(*owner)
                    .map(|layout| (*owner, layout.size))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        for (id, aggregate) in &aggregates {
            let fields = module.aggregate_fields(*id).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("aggregate type 缺少字段定义".to_owned())
            })?;
            let fields = fields
                .iter()
                .map(|field| {
                    types.get(field).copied().ok_or_else(|| {
                        LlvmAdapterError::Unsupported(
                            "aggregate 字段不具有 LLVM first-class 表示".to_owned(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            aggregate.set_body(&fields, false);
        }

        let mut tagged_layouts = BTreeMap::new();
        for (id, value) in tagged {
            let variants = module.tagged_variants(id).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("tagged union 缺少 payload 定义".to_owned())
            })?;
            let payloads = variants
                .iter()
                .map(|variant| {
                    aggregates.get(variant).copied().ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "tagged union payload 必须 lower 为 aggregate".to_owned(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let maximum_size = payloads
                .iter()
                .map(|payload| target.get_store_size(payload))
                .max()
                .unwrap_or(0);
            let maximum_alignment = payloads
                .iter()
                .map(|payload| target.get_abi_alignment(payload))
                .max()
                .unwrap_or(1)
                .max(1);
            let storage_unit = integer_for_alignment(context, target, maximum_alignment)?;
            let unit_size = target.get_store_size(&storage_unit).max(1);
            let units = maximum_size.div_ceil(unit_size);
            let units = u32::try_from(units).map_err(|_| {
                LlvmAdapterError::InvalidLayout(crate::llvm::layout::TargetLayoutError {
                    ty: id,
                    quantity: crate::llvm::layout::LayoutQuantity::Size,
                    failure: crate::llvm::layout::LayoutFailure::ExceedsTarget {
                        value: u128::from(maximum_size),
                        maximum: u128::from(u32::MAX) * u128::from(unit_size),
                    },
                })
            })?;
            let storage = storage_unit.array_type(units);
            value.set_body(&[context.i32_type().into(), storage.into()], false);
            tagged_layouts.insert(id, TaggedLayout { value, payloads });
        }

        let mut container_layouts = BTreeMap::new();
        for (id, header) in &containers {
            let (_, element) = module.sequential_container(*id).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("顺序容器类型缺少元素定义".to_owned())
            })?;
            let element_layout = layout_plan.layout(element)?;
            let element = types.get(&element).copied().ok_or_else(|| {
                LlvmAdapterError::Unsupported("顺序容器元素不具有 LLVM storage 表示".to_owned())
            })?;
            container_layouts.insert(
                *id,
                ContainerLayout {
                    header: *header,
                    element,
                    stride: element_layout.size,
                    element_alignment: element_layout.alignment,
                },
            );
        }

        Ok(Self {
            void_type: context.void_type(),
            pointer_type: pointer,
            types,
            aggregates,
            tagged_layouts,
            container_layouts,
            closure_layouts,
            shared_controls,
            shared_control_sizes,
        })
    }

    pub(super) fn shared_control(
        &self,
        ty: SsaTypeId,
    ) -> Result<StructType<'ctx>, LlvmAdapterError> {
        self.shared_controls.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("SSA type is not a shared owner".to_owned())
        })
    }

    pub(super) fn shared_control_size(&self, ty: SsaTypeId) -> Result<u64, LlvmAdapterError> {
        self.shared_control_sizes.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("SSA type is not a shared owner".to_owned())
        })
    }

    pub(super) fn tagged_layout(
        &self,
        ty: SsaTypeId,
    ) -> Result<&TaggedLayout<'ctx>, LlvmAdapterError> {
        self.tagged_layouts.get(&ty).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("SSA value 类型不是 tagged union".to_owned())
        })
    }

    pub(super) fn basic_type(
        &self,
        ty: SsaTypeId,
    ) -> Result<BasicTypeEnum<'ctx>, LlvmAdapterError> {
        self.types.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::Unsupported("SSA 类型不具有 LLVM first-class 表示".to_owned())
        })
    }

    pub(super) fn int_type(&self, ty: SsaTypeId) -> Result<IntType<'ctx>, LlvmAdapterError> {
        match self.basic_type(ty)? {
            BasicTypeEnum::IntType(ty) => Ok(ty),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "integer operation 的类型不是 LLVM integer".to_owned(),
            )),
        }
    }

    pub(super) fn aggregate_type(
        &self,
        ty: SsaTypeId,
    ) -> Result<StructType<'ctx>, LlvmAdapterError> {
        self.aggregates
            .get(&ty)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("SSA value 类型不是 aggregate".to_owned()))
    }

    pub(super) fn aggregate_layout(
        &self,
        target: &TargetData,
        ty: SsaTypeId,
    ) -> Result<AggregateLayout, LlvmAdapterError> {
        let aggregate = self.aggregates.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("layout 查询目标不是 aggregate".to_owned())
        })?;
        let field_offsets = (0..aggregate.count_fields())
            .map(|index| {
                target.offset_of_element(&aggregate, index).ok_or_else(|| {
                    LlvmAdapterError::Build("target DataLayout 无法计算字段 offset".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AggregateLayout {
            store_size: target.get_store_size(&aggregate),
            abi_size: target.get_abi_size(&aggregate),
            abi_alignment: target.get_abi_alignment(&aggregate),
            field_offsets,
        })
    }

    pub(super) fn container_layout(
        &self,
        ty: SsaTypeId,
    ) -> Result<ContainerLayout<'ctx>, LlvmAdapterError> {
        self.container_layouts
            .get(&ty)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("layout 查询目标不是顺序容器".to_owned()))
    }

    pub(super) fn closure_layout(
        &self,
        ty: SsaTypeId,
    ) -> Result<ClosureLayout<'ctx>, LlvmAdapterError> {
        self.closure_layouts.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("layout query target is not a closure".to_owned())
        })
    }

    pub(super) fn callable_function_type(
        &self,
        signature: &CallableSignature,
        has_environment: bool,
    ) -> Result<FunctionType<'ctx>, LlvmAdapterError> {
        let mut parameters = has_environment
            .then_some(BasicMetadataTypeEnum::from(self.pointer_type))
            .into_iter()
            .collect::<Vec<_>>();
        parameters.extend(
            signature
                .parameters
                .iter()
                .map(|parameter| match parameter {
                    EntityType::Value(ty) => self.basic_type(*ty).map(BasicMetadataTypeEnum::from),
                    EntityType::Loan { .. } => Ok(BasicMetadataTypeEnum::from(self.pointer_type)),
                    EntityType::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                        "callable signature parameter cannot be a place".to_owned(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        match signature.returns.as_slice() {
            [] => Ok(self.void_type.fn_type(&parameters, false)),
            [result] => Ok(self.basic_type(*result)?.fn_type(&parameters, false)),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "callable signature has more than one return type".to_owned(),
            )),
        }
    }
}

fn integer_for_alignment<'ctx>(
    context: &'ctx Context,
    target: &TargetData,
    alignment: u32,
) -> Result<IntType<'ctx>, LlvmAdapterError> {
    for candidate in [
        context.i8_type(),
        context.i16_type(),
        context.i32_type(),
        context.i64_type(),
        context.i128_type(),
    ] {
        if target.get_abi_alignment(&candidate) == alignment {
            return Ok(candidate);
        }
    }
    Err(LlvmAdapterError::Unsupported(
        "tagged union payload alignment has no supported integer storage unit".to_owned(),
    ))
}

fn integer_type<'ctx>(
    context: &'ctx Context,
    bits: u16,
) -> Result<IntType<'ctx>, LlvmAdapterError> {
    match bits {
        8 => Ok(context.i8_type()),
        16 => Ok(context.i16_type()),
        32 => Ok(context.i32_type()),
        64 => Ok(context.i64_type()),
        _ => Err(LlvmAdapterError::Unsupported(
            "LLVM adapter 只接受 8/16/32/64-bit integer".to_owned(),
        )),
    }
}
