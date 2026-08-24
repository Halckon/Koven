//! Target-aware LLVM type and aggregate layout mapping.

use std::collections::BTreeMap;

use inkwell::{
    AddressSpace,
    context::Context,
    targets::TargetData,
    types::{
        BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, IntType, StructType,
        VoidType,
    },
};

use crate::ssa::model::{CallableSignature, Module, SsaTypeId, SsaTypeKind};

use super::LlvmAdapterError;

pub(super) struct TypeMap<'ctx> {
    void_type: VoidType<'ctx>,
    types: BTreeMap<SsaTypeId, BasicTypeEnum<'ctx>>,
    aggregates: BTreeMap<SsaTypeId, StructType<'ctx>>,
    container_layouts: BTreeMap<SsaTypeId, ContainerLayout<'ctx>>,
    closure_layouts: BTreeMap<SsaTypeId, ClosureLayout<'ctx>>,
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

impl<'ctx> TypeMap<'ctx> {
    pub(super) fn lower(
        context: &'ctx Context,
        module: &Module,
        target: &TargetData,
    ) -> Result<Self, LlvmAdapterError> {
        let mut types = BTreeMap::new();
        let mut aggregates = BTreeMap::new();
        let mut containers = BTreeMap::new();
        let mut closures = BTreeMap::new();
        let pointer = context.ptr_type(AddressSpace::default());
        let size_type = context.ptr_sized_int_type(target, None);

        for (index, kind) in module.types.iter().enumerate() {
            let id = SsaTypeId {
                module: module.id,
                index,
            };
            let ty = match kind {
                SsaTypeKind::Boolean => Some(context.bool_type().into()),
                SsaTypeKind::Integer { bits, .. } => Some(integer_type(context, *bits)?.into()),
                SsaTypeKind::Aggregate { .. } => {
                    let aggregate = context.opaque_struct_type(&format!("koven.t{index}"));
                    aggregates.insert(id, aggregate);
                    Some(aggregate.into())
                }
                SsaTypeKind::HeapOwner { .. } => Some(pointer.into()),
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

        let mut container_layouts = BTreeMap::new();
        for (id, header) in &containers {
            let (_, element) = module.sequential_container(*id).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("顺序容器类型缺少元素定义".to_owned())
            })?;
            let element = types.get(&element).copied().ok_or_else(|| {
                LlvmAdapterError::Unsupported("顺序容器元素不具有 LLVM storage 表示".to_owned())
            })?;
            container_layouts.insert(
                *id,
                ContainerLayout {
                    header: *header,
                    element,
                    stride: target.get_abi_size(&element),
                    element_alignment: target.get_abi_alignment(&element),
                },
            );
        }

        Ok(Self {
            void_type: context.void_type(),
            types,
            aggregates,
            container_layouts,
            closure_layouts,
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
        environment: Option<StructType<'ctx>>,
    ) -> Result<FunctionType<'ctx>, LlvmAdapterError> {
        let mut parameters = environment
            .into_iter()
            .map(BasicMetadataTypeEnum::from)
            .collect::<Vec<_>>();
        parameters.extend(
            signature
                .parameters
                .iter()
                .map(|ty| self.basic_type(*ty).map(BasicMetadataTypeEnum::from))
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
