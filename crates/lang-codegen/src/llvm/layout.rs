//! 在创建 LLVM 复合类型前验证 target-dependent storage layout。

use std::collections::BTreeMap;

use inkwell::{AddressSpace, context::Context, targets::TargetData, types::BasicTypeEnum};

use crate::ssa::model::{Module, SequentialContainerKind, SsaTypeId, SsaTypeKind};

use super::LlvmAdapterError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayoutQuantity {
    Size,
    Alignment,
    Stride,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayoutFailure {
    ArithmeticOverflow,
    ExceedsTarget { value: u128, maximum: u128 },
    InvalidAlignment { value: u128 },
}

/// 一个在 LLVM 复合类型构造前产生的确定性布局错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TargetLayoutError {
    pub(crate) ty: SsaTypeId,
    pub(crate) quantity: LayoutQuantity,
    pub(crate) failure: LayoutFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CheckedLayout {
    pub(super) size: u64,
    pub(super) alignment: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RawLayout {
    pub(super) size: u128,
    pub(super) alignment: u128,
}

/// verified SSA 在当前 target 上的受检布局事实。
#[derive(Debug)]
pub(super) struct TargetLayoutPlan {
    layouts: BTreeMap<SsaTypeId, CheckedLayout>,
}

impl TargetLayoutPlan {
    pub(super) fn build(
        context: &Context,
        module: &Module,
        target: &TargetData,
    ) -> Result<Self, LlvmAdapterError> {
        let pointer = context.ptr_type(AddressSpace::default());
        let size_type = context.ptr_sized_int_type(target, None);
        let pointer_layout = RawLayout {
            size: u128::from(target.get_abi_size(&pointer)),
            alignment: u128::from(target.get_abi_alignment(&pointer)),
        };
        let size_layout = RawLayout {
            size: u128::from(target.get_abi_size(&size_type)),
            alignment: u128::from(target.get_abi_alignment(&size_type)),
        };
        let pointer_bits = target
            .get_pointer_byte_size(Some(AddressSpace::default()))
            .checked_mul(8)
            .ok_or_else(|| LlvmAdapterError::Target("target pointer width overflow".to_owned()))?;
        let maximum = unsigned_max(pointer_bits);
        let mut calculator = LayoutCalculator {
            context,
            module,
            target,
            pointer_layout,
            size_layout,
            maximum,
            raw: BTreeMap::new(),
        };
        let mut layouts = BTreeMap::new();
        for index in 0..module.types.len() {
            let ty = SsaTypeId {
                module: module.id,
                index,
            };
            if matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::Unit | SsaTypeKind::Opaque { .. })
            ) {
                continue;
            }
            let raw = calculator.layout(ty)?;
            layouts.insert(ty, calculator.checked(ty, LayoutQuantity::Size, raw)?);
        }
        Ok(Self { layouts })
    }

    pub(super) fn layout(&self, ty: SsaTypeId) -> Result<CheckedLayout, LlvmAdapterError> {
        self.layouts.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::Unsupported(
                "SSA type does not have a preflighted storage layout".to_owned(),
            )
        })
    }
}

struct LayoutCalculator<'a, 'ctx> {
    context: &'ctx Context,
    module: &'a Module,
    target: &'a TargetData,
    pointer_layout: RawLayout,
    size_layout: RawLayout,
    maximum: u128,
    raw: BTreeMap<SsaTypeId, RawLayout>,
}

impl LayoutCalculator<'_, '_> {
    fn layout(&mut self, ty: SsaTypeId) -> Result<RawLayout, LlvmAdapterError> {
        let layout = match self.module.type_kind(ty) {
            Some(SsaTypeKind::Boolean) => self.llvm_layout(self.context.bool_type().into()),
            Some(SsaTypeKind::Integer { bits, .. }) => {
                let integer = match bits {
                    8 => self.context.i8_type(),
                    16 => self.context.i16_type(),
                    32 => self.context.i32_type(),
                    64 => self.context.i64_type(),
                    _ => {
                        return Err(LlvmAdapterError::Unsupported(
                            "target layout only supports 8/16/32/64-bit integers".to_owned(),
                        ));
                    }
                };
                self.llvm_layout(integer.into())
            }
            Some(SsaTypeKind::ZeroSized { .. }) => RawLayout {
                size: 0,
                alignment: 1,
            },
            Some(
                SsaTypeKind::HeapOwner { .. }
                | SsaTypeKind::SharedReference { .. }
                | SsaTypeKind::FunctionPointer { .. },
            ) => self.pointer_layout,
            Some(SsaTypeKind::Aggregate { fields, .. }) => {
                let mut layouts = Vec::with_capacity(fields.len());
                for field in fields {
                    layouts.push(self.dependency(*field)?);
                }
                self.record(ty, &layouts)?
            }
            Some(SsaTypeKind::TaggedUnion { variants, .. }) => {
                let mut payload = RawLayout {
                    size: 0,
                    alignment: 1,
                };
                for variant in variants {
                    let layout = self.dependency(*variant)?;
                    payload.size = payload.size.max(layout.size);
                    payload.alignment = payload.alignment.max(layout.alignment);
                }
                let tag = self.llvm_layout(self.context.i32_type().into());
                self.record(ty, &[tag, payload])?
            }
            Some(SsaTypeKind::SequentialContainer { kind, element }) => {
                let element = *element;
                let element_layout = self.dependency(element)?;
                self.check_quantity(ty, LayoutQuantity::Stride, element_layout.size)?;
                let fields = match kind {
                    SequentialContainerKind::Array | SequentialContainerKind::List => {
                        vec![self.pointer_layout, self.size_layout]
                    }
                    SequentialContainerKind::MutableList => {
                        vec![self.pointer_layout, self.size_layout, self.size_layout]
                    }
                };
                self.record(ty, &fields)?
            }
            Some(SsaTypeKind::ConcreteClosure { environment, .. }) => {
                let environment = self.dependency(*environment)?;
                self.record(ty, &[self.pointer_layout, environment])?
            }
            Some(SsaTypeKind::Unit | SsaTypeKind::Opaque { .. }) => {
                return Err(LlvmAdapterError::Unsupported(
                    "SSA type does not have target storage".to_owned(),
                ));
            }
            None => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "target layout preflight encountered an unknown type".to_owned(),
                ));
            }
        };
        self.check_quantity(ty, LayoutQuantity::Size, layout.size)?;
        self.check_quantity(ty, LayoutQuantity::Alignment, layout.alignment)?;
        self.raw.insert(ty, layout);
        Ok(layout)
    }

    fn dependency(&self, ty: SsaTypeId) -> Result<RawLayout, LlvmAdapterError> {
        self.raw.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa(
                "target layout dependency is forward, cyclic, foreign, or has no storage"
                    .to_owned(),
            )
        })
    }

    fn llvm_layout(&self, ty: BasicTypeEnum<'_>) -> RawLayout {
        RawLayout {
            size: u128::from(self.target.get_abi_size(&ty)),
            alignment: u128::from(self.target.get_abi_alignment(&ty)),
        }
    }

    fn record(&self, ty: SsaTypeId, fields: &[RawLayout]) -> Result<RawLayout, LlvmAdapterError> {
        checked_record(ty, fields, self.maximum)
    }

    fn check_quantity(
        &self,
        ty: SsaTypeId,
        quantity: LayoutQuantity,
        value: u128,
    ) -> Result<(), LlvmAdapterError> {
        if value > self.maximum {
            return Err(invalid_layout(
                ty,
                quantity,
                LayoutFailure::ExceedsTarget {
                    value,
                    maximum: self.maximum,
                },
            ));
        }
        Ok(())
    }

    fn checked(
        &self,
        ty: SsaTypeId,
        quantity: LayoutQuantity,
        raw: RawLayout,
    ) -> Result<CheckedLayout, LlvmAdapterError> {
        let size = u64::try_from(raw.size).map_err(|_| {
            invalid_layout(
                ty,
                quantity,
                LayoutFailure::ExceedsTarget {
                    value: raw.size,
                    maximum: self.maximum,
                },
            )
        })?;
        let alignment = u32::try_from(raw.alignment).map_err(|_| {
            invalid_layout(
                ty,
                LayoutQuantity::Alignment,
                LayoutFailure::ExceedsTarget {
                    value: raw.alignment,
                    maximum: u128::from(u32::MAX),
                },
            )
        })?;
        Ok(CheckedLayout { size, alignment })
    }
}

pub(super) fn checked_record(
    ty: SsaTypeId,
    fields: &[RawLayout],
    maximum: u128,
) -> Result<RawLayout, LlvmAdapterError> {
    let mut size = 0_u128;
    let mut alignment = 1_u128;
    for field in fields {
        if field.alignment == 0 {
            return Err(invalid_layout(
                ty,
                LayoutQuantity::Alignment,
                LayoutFailure::InvalidAlignment { value: 0 },
            ));
        }
        size = align_up(size, field.alignment).ok_or_else(|| {
            invalid_layout(ty, LayoutQuantity::Size, LayoutFailure::ArithmeticOverflow)
        })?;
        size = size.checked_add(field.size).ok_or_else(|| {
            invalid_layout(ty, LayoutQuantity::Size, LayoutFailure::ArithmeticOverflow)
        })?;
        if size > maximum {
            return Err(invalid_layout(
                ty,
                LayoutQuantity::Size,
                LayoutFailure::ExceedsTarget {
                    value: size,
                    maximum,
                },
            ));
        }
        alignment = alignment.max(field.alignment);
    }
    size = align_up(size, alignment).ok_or_else(|| {
        invalid_layout(ty, LayoutQuantity::Size, LayoutFailure::ArithmeticOverflow)
    })?;
    if size > maximum {
        return Err(invalid_layout(
            ty,
            LayoutQuantity::Size,
            LayoutFailure::ExceedsTarget {
                value: size,
                maximum,
            },
        ));
    }
    Ok(RawLayout { size, alignment })
}

fn align_up(value: u128, alignment: u128) -> Option<u128> {
    if alignment == 0 {
        return None;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - remainder)
    }
}

fn invalid_layout(
    ty: SsaTypeId,
    quantity: LayoutQuantity,
    failure: LayoutFailure,
) -> LlvmAdapterError {
    LlvmAdapterError::InvalidLayout(TargetLayoutError {
        ty,
        quantity,
        failure,
    })
}

fn unsigned_max(bits: u32) -> u128 {
    match bits {
        0 => 0,
        1..=127 => (1_u128 << bits) - 1,
        _ => u128::MAX,
    }
}
