//! Target-aware LLVM type and aggregate layout mapping.

use std::collections::BTreeMap;

use inkwell::{
    AddressSpace,
    context::Context,
    targets::TargetData,
    types::{BasicTypeEnum, IntType, StructType},
};

use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

use super::LlvmAdapterError;

pub(super) struct TypeMap<'ctx> {
    types: BTreeMap<SsaTypeId, BasicTypeEnum<'ctx>>,
    aggregates: BTreeMap<SsaTypeId, StructType<'ctx>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AggregateLayout {
    pub(super) store_size: u64,
    pub(super) abi_size: u64,
    pub(super) abi_alignment: u32,
    pub(super) field_offsets: Vec<u64>,
}

impl<'ctx> TypeMap<'ctx> {
    pub(super) fn lower(context: &'ctx Context, module: &Module) -> Result<Self, LlvmAdapterError> {
        let mut types = BTreeMap::new();
        let mut aggregates = BTreeMap::new();

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
                SsaTypeKind::HeapOwner { .. } => {
                    Some(context.ptr_type(AddressSpace::default()).into())
                }
                SsaTypeKind::Unit | SsaTypeKind::Opaque { .. } => None,
            };
            if let Some(ty) = ty {
                types.insert(id, ty);
            }
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

        Ok(Self { types, aggregates })
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
