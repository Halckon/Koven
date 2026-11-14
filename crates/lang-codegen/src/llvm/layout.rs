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
    shared_controls: BTreeMap<SsaTypeId, CheckedLayout>,
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
            if matches!(module.type_kind(ty), Some(SsaTypeKind::Opaque { .. })) {
                continue;
            }
            let raw = calculator.layout(ty)?;
            layouts.insert(ty, calculator.checked(ty, LayoutQuantity::Size, raw)?);
        }
        let mut shared_controls = BTreeMap::new();
        for index in 0..module.types.len() {
            let owner = SsaTypeId {
                module: module.id,
                index,
            };
            let Some(SsaTypeKind::SharedOwner {
                payload: Some(payload),
                ..
            }) = module.type_kind(owner)
            else {
                continue;
            };
            let payload = calculator.dependency(*payload)?;
            let raw = calculator.record(owner, &[size_layout, payload])?;
            calculator.check_quantity(owner, LayoutQuantity::Size, raw.size)?;
            calculator.check_quantity(owner, LayoutQuantity::Alignment, raw.alignment)?;
            shared_controls.insert(owner, calculator.checked(owner, LayoutQuantity::Size, raw)?);
        }
        Ok(Self {
            layouts,
            shared_controls,
        })
    }

    pub(super) fn layout(&self, ty: SsaTypeId) -> Result<CheckedLayout, LlvmAdapterError> {
        self.layouts.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::Unsupported(
                "SSA type does not have a preflighted storage layout".to_owned(),
            )
        })
    }

    pub(super) fn shared_control_layout(
        &self,
        ty: SsaTypeId,
    ) -> Result<CheckedLayout, LlvmAdapterError> {
        self.shared_controls.get(&ty).copied().ok_or_else(|| {
            LlvmAdapterError::Unsupported(
                "SSA type does not have a preflighted shared control layout".to_owned(),
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
            Some(SsaTypeKind::Unit) => {
                self.llvm_layout(self.context.struct_type(&[], false).into())
            }
            Some(SsaTypeKind::Boolean) => self.llvm_layout(self.context.bool_type().into()),
            Some(SsaTypeKind::Char) => self.llvm_layout(self.context.i32_type().into()),
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
                | SsaTypeKind::SharedOwner { .. }
                | SsaTypeKind::NullableHandle { .. }
                | SsaTypeKind::SharedReference { .. }
                | SsaTypeKind::FunctionPointer { .. },
            ) => self.pointer_layout,
            Some(SsaTypeKind::InlineNullable { inner, niche: true }) => self.dependency(*inner)?,
            Some(SsaTypeKind::InlineNullable {
                inner,
                niche: false,
            }) => {
                let present = self.llvm_layout(self.context.bool_type().into());
                let payload = self.dependency(*inner)?;
                self.record(ty, &[present, payload])?
            }
            Some(SsaTypeKind::CallableView { .. }) => {
                self.record(ty, &[self.pointer_layout, self.pointer_layout])?
            }
            Some(SsaTypeKind::RangeView { .. } | SsaTypeKind::StringOwner) => self.record(
                ty,
                &[self.pointer_layout, self.size_layout, self.size_layout],
            )?,
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
            Some(SsaTypeKind::MapContainer { key, value, .. }) => {
                let key = self.dependency(*key)?;
                let value = self.dependency(*value)?;
                let state = self.llvm_layout(self.context.i32_type().into());
                let slot =
                    self.record(ty, &[state, key, value, self.size_layout, self.size_layout])?;
                self.check_quantity(ty, LayoutQuantity::Stride, slot.size)?;
                self.checked(ty, LayoutQuantity::Stride, slot)?;
                self.record(
                    ty,
                    &[
                        self.pointer_layout,
                        self.size_layout,
                        self.size_layout,
                        self.size_layout,
                        self.size_layout,
                        self.size_layout,
                    ],
                )?
            }
            Some(SsaTypeKind::ConcreteClosure { environment, .. }) => {
                let environment = self.dependency(*environment)?;
                self.record(ty, &[self.pointer_layout, environment])?
            }
            Some(SsaTypeKind::Opaque { .. }) => {
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

pub(crate) mod type_map {
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

    use super::super::{LlvmAdapterError, layout::TargetLayoutPlan};

    pub(crate) struct TypeMap<'ctx> {
        void_type: VoidType<'ctx>,
        pointer_type: PointerType<'ctx>,
        types: BTreeMap<SsaTypeId, BasicTypeEnum<'ctx>>,
        // Unit storage is deliberately absent from the general ABI/aggregate type table.
        container_unit_storage: BTreeMap<SsaTypeId, BasicTypeEnum<'ctx>>,
        aggregates: BTreeMap<SsaTypeId, StructType<'ctx>>,
        tagged_layouts: BTreeMap<SsaTypeId, TaggedLayout<'ctx>>,
        container_layouts: BTreeMap<SsaTypeId, ContainerLayout<'ctx>>,
        closure_layouts: BTreeMap<SsaTypeId, ClosureLayout<'ctx>>,
        shared_controls: BTreeMap<SsaTypeId, StructType<'ctx>>,
        shared_control_sizes: BTreeMap<SsaTypeId, u64>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct AggregateLayout {
        pub(crate) store_size: u64,
        pub(crate) abi_size: u64,
        pub(crate) abi_alignment: u32,
        pub(crate) field_offsets: Vec<u64>,
    }

    #[derive(Clone, Copy)]
    pub(crate) struct ContainerLayout<'ctx> {
        pub(crate) header: StructType<'ctx>,
        pub(crate) element: BasicTypeEnum<'ctx>,
        pub(crate) stride: u64,
        pub(crate) element_alignment: u32,
    }

    #[derive(Clone, Copy)]
    pub(crate) struct ClosureLayout<'ctx> {
        pub(crate) value: StructType<'ctx>,
        pub(crate) environment: StructType<'ctx>,
    }

    #[derive(Clone)]
    pub(crate) struct TaggedLayout<'ctx> {
        pub(crate) value: StructType<'ctx>,
        pub(crate) payloads: Vec<StructType<'ctx>>,
    }

    impl<'ctx> TypeMap<'ctx> {
        pub(crate) fn lower(
            context: &'ctx Context,
            module: &Module,
            target: &TargetData,
        ) -> Result<Self, LlvmAdapterError> {
            // Validate the closed SSA type graph before creating any LLVM composite type. This keeps
            // oversized layouts out of opaque struct bodies, GEPs, and allocator lowering.
            let layout_plan = TargetLayoutPlan::build(context, module, target)?;
            let mut types = BTreeMap::new();
            let mut container_unit_storage = BTreeMap::new();
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
                    // ADR-0033: inner types precede their nullable identity in the type table.
                    SsaTypeKind::InlineNullable { inner, niche } => {
                        let inner = types.get(inner).copied().ok_or_else(|| {
                            LlvmAdapterError::Unsupported(
                                "inline nullable inner 不具有 LLVM storage".to_owned(),
                            )
                        })?;
                        if *niche {
                            Some(inner)
                        } else {
                            let nullable =
                                context.opaque_struct_type(&format!("koven.nullable.t{index}"));
                            nullable.set_body(&[context.bool_type().into(), inner], false);
                            Some(nullable.into())
                        }
                    }
                    SsaTypeKind::SharedOwner { .. } => {
                        let control = context.opaque_struct_type(&format!("koven.shared.t{index}"));
                        shared_controls.insert(id, control);
                        Some(pointer.into())
                    }
                    SsaTypeKind::SharedReference { .. } | SsaTypeKind::FunctionPointer { .. } => {
                        Some(pointer.into())
                    }
                    // ADR-0034 view descriptor `{code, environment}`; loans of it are pointers.
                    SsaTypeKind::CallableView { .. } => Some(
                        context
                            .struct_type(&[pointer.into(), pointer.into()], false)
                            .into(),
                    ),
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
                    SsaTypeKind::RangeView { .. } => Some(
                        context
                            .struct_type(
                                &[pointer.into(), size_type.into(), size_type.into()],
                                false,
                            )
                            .into(),
                    ),
                    SsaTypeKind::MapContainer { .. } => {
                        let map_type = context.opaque_struct_type(&format!("koven.map.t{index}"));
                        map_type.set_body(
                            &[
                                pointer.into(),
                                size_type.into(),
                                size_type.into(),
                                size_type.into(),
                                size_type.into(),
                                size_type.into(),
                            ],
                            false,
                        );
                        Some(map_type.into())
                    }
                    SsaTypeKind::StringOwner => {
                        let string = context.opaque_struct_type(&format!("koven.string.t{index}"));
                        string
                            .set_body(&[pointer.into(), size_type.into(), size_type.into()], false);
                        Some(string.into())
                    }
                    SsaTypeKind::ZeroSized { .. } => {
                        let zst = context.opaque_struct_type(&format!("koven.zst.t{index}"));
                        zst.set_body(&[], false);
                        Some(zst.into())
                    }
                    SsaTypeKind::ConcreteClosure { .. } => {
                        let closure =
                            context.opaque_struct_type(&format!("koven.closure.t{index}"));
                        closures.insert(id, closure);
                        Some(closure.into())
                    }
                    SsaTypeKind::Unit => {
                        container_unit_storage.insert(id, context.struct_type(&[], false).into());
                        None
                    }
                    SsaTypeKind::Opaque { .. } => None,
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
                    LlvmAdapterError::Unsupported(
                        "shared owner payload 不具有 LLVM storage".to_owned(),
                    )
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

            let mut result = Self {
                void_type: context.void_type(),
                pointer_type: pointer,
                types,
                container_unit_storage,
                aggregates,
                tagged_layouts,
                container_layouts: BTreeMap::new(),
                closure_layouts,
                shared_controls,
                shared_control_sizes,
            };
            for (id, header) in &containers {
                let (_, element) = module.sequential_container(*id).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("顺序容器类型缺少元素定义".to_owned())
                })?;
                let element_layout = layout_plan.layout(element)?;
                let element = result.container_element_type(element)?;
                result.container_layouts.insert(
                    *id,
                    ContainerLayout {
                        header: *header,
                        element,
                        stride: element_layout.size,
                        element_alignment: element_layout.alignment,
                    },
                );
            }

            Ok(result)
        }

        pub(crate) fn shared_control(
            &self,
            ty: SsaTypeId,
        ) -> Result<StructType<'ctx>, LlvmAdapterError> {
            self.shared_controls.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("SSA type is not a shared owner".to_owned())
            })
        }

        pub(crate) fn shared_control_size(&self, ty: SsaTypeId) -> Result<u64, LlvmAdapterError> {
            self.shared_control_sizes.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("SSA type is not a shared owner".to_owned())
            })
        }

        pub(crate) fn tagged_layout(
            &self,
            ty: SsaTypeId,
        ) -> Result<&TaggedLayout<'ctx>, LlvmAdapterError> {
            self.tagged_layouts.get(&ty).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("SSA value 类型不是 tagged union".to_owned())
            })
        }

        pub(crate) fn basic_type(
            &self,
            ty: SsaTypeId,
        ) -> Result<BasicTypeEnum<'ctx>, LlvmAdapterError> {
            self.types.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::Unsupported("SSA 类型不具有 LLVM first-class 表示".to_owned())
            })
        }

        /// Storage at a container element boundary, without enabling general Unit value ABI.
        pub(crate) fn container_element_type(
            &self,
            ty: SsaTypeId,
        ) -> Result<BasicTypeEnum<'ctx>, LlvmAdapterError> {
            self.container_unit_storage
                .get(&ty)
                .copied()
                .map_or_else(|| self.basic_type(ty), Ok)
        }

        pub(crate) fn int_type(&self, ty: SsaTypeId) -> Result<IntType<'ctx>, LlvmAdapterError> {
            match self.basic_type(ty)? {
                BasicTypeEnum::IntType(ty) => Ok(ty),
                _ => Err(LlvmAdapterError::InvalidSsa(
                    "integer operation 的类型不是 LLVM integer".to_owned(),
                )),
            }
        }

        pub(crate) fn aggregate_type(
            &self,
            ty: SsaTypeId,
        ) -> Result<StructType<'ctx>, LlvmAdapterError> {
            self.aggregates.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("SSA value 类型不是 aggregate".to_owned())
            })
        }

        pub(crate) fn aggregate_layout(
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

        pub(crate) fn container_layout(
            &self,
            ty: SsaTypeId,
        ) -> Result<ContainerLayout<'ctx>, LlvmAdapterError> {
            self.container_layouts.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("layout 查询目标不是顺序容器".to_owned())
            })
        }

        pub(crate) fn closure_layout(
            &self,
            ty: SsaTypeId,
        ) -> Result<ClosureLayout<'ctx>, LlvmAdapterError> {
            self.closure_layouts.get(&ty).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("layout query target is not a closure".to_owned())
            })
        }

        pub(crate) fn callable_function_type(
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
                        EntityType::Value(ty) => {
                            self.basic_type(*ty).map(BasicMetadataTypeEnum::from)
                        }
                        EntityType::Loan { .. } => {
                            Ok(BasicMetadataTypeEnum::from(self.pointer_type))
                        }
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
}
