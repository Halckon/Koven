//! compilation-unit concrete storage type 到 SSA type identity/layout 的映射。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, UnitTypeId,
        UnitTypeKind,
    },
};

use super::lowering_error;
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Module, SequentialContainerKind as SsaContainerKind, SsaTypeId, SsaTypeKind},
    unit_plan::resolve_nominal_runtime_field_types,
};

/// unit lowering 共享的 concrete type identity 与 nominal layout metadata。
pub(super) struct UnitTypeLowering {
    type_ids: BTreeMap<UnitTypeId, SsaTypeId>,
    heap_payloads: BTreeMap<SsaTypeId, SsaTypeId>,
    field_indices: BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
    enum_payloads: BTreeMap<(SsaTypeId, UnitSymbolId), (usize, SsaTypeId)>,
    active_inline: BTreeSet<UnitTypeId>,
    active_owner_definitions: BTreeMap<UnitTypeId, Span>,
    pending_owners: BTreeMap<UnitTypeId, PendingOwnerDefinition>,
}

enum PendingOwnerDefinition {
    Class {
        owner: SsaTypeId,
        payload_name: String,
        fields: Vec<(UnitTypeId, Span)>,
        span: Span,
    },
    Box {
        owner: SsaTypeId,
        payload: UnitTypeId,
        span: Span,
    },
    Rc {
        owner: SsaTypeId,
        payload: UnitTypeId,
        span: Span,
    },
}

impl PendingOwnerDefinition {
    fn span(&self) -> Span {
        match self {
            Self::Class { span, .. } | Self::Box { span, .. } | Self::Rc { span, .. } => *span,
        }
    }
}

impl UnitTypeLowering {
    pub(super) fn new() -> Self {
        Self {
            type_ids: BTreeMap::new(),
            heap_payloads: BTreeMap::new(),
            field_indices: BTreeMap::new(),
            enum_payloads: BTreeMap::new(),
            active_inline: BTreeSet::new(),
            active_owner_definitions: BTreeMap::new(),
            pending_owners: BTreeMap::new(),
        }
    }

    pub(super) fn type_ids(&self) -> &BTreeMap<UnitTypeId, SsaTypeId> {
        &self.type_ids
    }

    pub(super) fn heap_payloads(&self) -> &BTreeMap<SsaTypeId, SsaTypeId> {
        &self.heap_payloads
    }

    pub(super) fn field_indices(&self) -> &BTreeMap<(UnitTypeId, UnitSymbolId), usize> {
        &self.field_indices
    }

    pub(super) fn enum_payloads(&self) -> &BTreeMap<(SsaTypeId, UnitSymbolId), (usize, SsaTypeId)> {
        &self.enum_payloads
    }

    pub(super) fn intern(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let id = self.intern_inner(module, typed, ty, span)?;
        self.define_pending_owners(module, typed)?;
        Ok(id)
    }

    fn intern_inner(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        if let Some(id) = self.type_ids.get(&ty).copied() {
            return Ok(id);
        }
        let kind = typed
            .types()
            .get(ty)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let id = match kind {
            UnitTypeKind::Builtin(BuiltinType::Boolean) => module.intern_type(SsaTypeKind::Boolean),
            UnitTypeKind::Builtin(BuiltinType::Byte) => module.intern_type(integer_type(8, true)),
            UnitTypeKind::Builtin(BuiltinType::UByte) => module.intern_type(integer_type(8, false)),
            UnitTypeKind::Builtin(BuiltinType::Short) => module.intern_type(integer_type(16, true)),
            UnitTypeKind::Builtin(BuiltinType::UShort) => {
                module.intern_type(integer_type(16, false))
            }
            UnitTypeKind::Builtin(BuiltinType::Int) => module.intern_type(integer_type(32, true)),
            UnitTypeKind::Builtin(BuiltinType::UInt) => module.intern_type(integer_type(32, false)),
            UnitTypeKind::Builtin(BuiltinType::Long) => module.intern_type(integer_type(64, true)),
            UnitTypeKind::Builtin(BuiltinType::ULong) => {
                module.intern_type(integer_type(64, false))
            }
            UnitTypeKind::Builtin(BuiltinType::String) => module.add_string_owner_type(),
            UnitTypeKind::Builtin(BuiltinType::Unit) => module.intern_type(SsaTypeKind::Unit),
            UnitTypeKind::Nullable(inner) => {
                if !is_supported_nullable_inner(typed, inner) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let inner_type = inner;
                let inner = self.intern_inner(module, typed, inner_type, span)?;
                self.define_pending_owner(module, typed, inner_type)?;
                module
                    .add_nullable_handle_type(inner)
                    .map_err(|_| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            }
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                arguments,
            } => self.intern_rc(module, ty, &arguments, span)?,
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                arguments,
            } => self.intern_box(module, ty, &arguments, span)?,
            UnitTypeKind::Intrinsic {
                constructor:
                    constructor @ (IntrinsicTypeConstructor::Array
                    | IntrinsicTypeConstructor::List
                    | IntrinsicTypeConstructor::MutableList),
                arguments,
            } => self.intern_container(module, typed, constructor, &arguments, span)?,
            UnitTypeKind::Nominal {
                declaration,
                arguments,
            } => self.intern_nominal(module, typed, ty, declaration, &arguments, span)?,
            UnitTypeKind::EnumCase { root, .. } => self.intern_inner(module, typed, root, span)?,
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        self.type_ids.insert(ty, id);
        Ok(id)
    }

    fn define_pending_owners(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
    ) -> Result<(), LoweringError> {
        while let Some(ty) = self.pending_owners.keys().next().copied() {
            self.define_pending_owner(module, typed, ty)?;
        }
        Ok(())
    }

    fn define_pending_owner(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
    ) -> Result<(), LoweringError> {
        if let Some(span) = self.active_owner_definitions.get(&ty).copied() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(definition) = self.pending_owners.remove(&ty) else {
            return Ok(());
        };
        self.active_owner_definitions.insert(ty, definition.span());
        let result = (|| {
            match definition {
                PendingOwnerDefinition::Class {
                    owner,
                    payload_name,
                    fields,
                    span,
                } => {
                    let fields = fields
                        .into_iter()
                        .map(|(field, span)| self.intern_inner(module, typed, field, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let payload = module
                        .add_aggregate_type(payload_name, fields)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    module
                        .define_heap_owner(owner, payload)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    self.heap_payloads.insert(owner, payload);
                }
                PendingOwnerDefinition::Box {
                    owner,
                    payload,
                    span,
                } => {
                    let payload = self.intern_inner(module, typed, payload, span)?;
                    module
                        .define_heap_owner(owner, payload)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    self.heap_payloads.insert(owner, payload);
                }
                PendingOwnerDefinition::Rc {
                    owner,
                    payload,
                    span,
                } => {
                    let payload_id = payload;
                    let payload = self.intern_inner(module, typed, payload_id, span)?;
                    // SharedOwner 要求 payload identity 已完整定义；递归完成其 pending owner。
                    self.define_pending_owner(module, typed, payload_id)?;
                    module
                        .define_shared_owner(owner, payload)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
            }
            Ok(())
        })();
        self.active_owner_definitions.remove(&ty);
        result
    }

    fn intern_rc(
        &mut self,
        module: &mut Module,
        ty: UnitTypeId,
        arguments: &[UnitTypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [payload] = arguments else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let owner = module
            .declare_shared_owner(format!("Rc#u{}", ty.index()))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        // 先登记并延迟定义 handle，使 owner 间接递归可以打断 inline layout cycle。
        self.type_ids.insert(ty, owner);
        self.pending_owners.insert(
            ty,
            PendingOwnerDefinition::Rc {
                owner,
                payload: *payload,
                span,
            },
        );
        Ok(owner)
    }

    fn intern_box(
        &mut self,
        module: &mut Module,
        ty: UnitTypeId,
        arguments: &[UnitTypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [payload] = arguments else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let owner = module
            .declare_heap_owner(format!("Box#u{}", ty.index()))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.type_ids.insert(ty, owner);
        self.pending_owners.insert(
            ty,
            PendingOwnerDefinition::Box {
                owner,
                payload: *payload,
                span,
            },
        );
        Ok(owner)
    }

    fn intern_container(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        constructor: IntrinsicTypeConstructor,
        arguments: &[UnitTypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [element] = arguments else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let kind = match constructor {
            IntrinsicTypeConstructor::Array => SsaContainerKind::Array,
            IntrinsicTypeConstructor::List => SsaContainerKind::List,
            IntrinsicTypeConstructor::MutableList => SsaContainerKind::MutableList,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let element = self.intern_inner(module, typed, *element, span)?;
        module
            .add_sequential_container_type(kind, element)
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    fn intern_nominal(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        declaration: lang_frontend::name_resolution::DeclarationId,
        arguments: &[UnitTypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let nominal = typed
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if arguments.len() != nominal.type_parameters().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if !nominal.type_parameters().is_empty() && nominal.kind() != NominalKind::Class {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if nominal.kind() == NominalKind::EnumClass {
            return self.intern_enum(module, typed, ty, declaration, nominal.enum_cases(), span);
        }
        let fields = nominal.fields().to_vec();
        let concrete_fields = resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?;
        if nominal.kind() == NominalKind::ValueClass && !self.active_inline.insert(ty) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        for (index, field) in fields.iter().enumerate() {
            if self
                .field_indices
                .insert((ty, field.symbol()), index)
                .is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        match nominal.kind() {
            NominalKind::Class => {
                let base_name = format!("class#d{}.u{}", declaration.index(), ty.index());
                let owner = module
                    .declare_heap_owner(&base_name)
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                self.type_ids.insert(ty, owner);
                self.pending_owners.insert(
                    ty,
                    PendingOwnerDefinition::Class {
                        owner,
                        payload_name: format!("{base_name}.payload"),
                        fields: fields
                            .iter()
                            .zip(concrete_fields)
                            .map(|(field, concrete)| (concrete, field.span()))
                            .collect(),
                        span,
                    },
                );
                Ok(owner)
            }
            NominalKind::ValueClass => {
                let result = fields
                    .iter()
                    .map(|field| self.intern_inner(module, typed, field.ty(), field.span()))
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(|fields| {
                        module
                            .add_aggregate_type(
                                format!("value#d{}.u{}", declaration.index(), ty.index()),
                                fields,
                            )
                            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
                    });
                self.active_inline.remove(&ty);
                result
            }
            // v1 object 无运行时状态；空 aggregate 只为 Borrow ABI 提供可 addressize 的 ZST identity。
            NominalKind::Object if fields.is_empty() => module
                .add_aggregate_type(
                    format!("object#d{}.u{}", declaration.index(), ty.index()),
                    Vec::new(),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span)),
            NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    fn intern_enum(
        &mut self,
        module: &mut Module,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        declaration: lang_frontend::name_resolution::DeclarationId,
        cases: &[lang_frontend::type_checking::UnitEnumCaseSignature],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        if cases.is_empty() || !self.active_inline.insert(ty) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let result = (|| {
            let mut payloads = Vec::with_capacity(cases.len());
            for (variant, case) in cases.iter().enumerate() {
                for (field, payload) in case.payloads().iter().enumerate() {
                    if self
                        .field_indices
                        .insert((case.case_type(), payload.symbol()), field)
                        .is_some()
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
                let fields = case
                    .payloads()
                    .iter()
                    .map(|payload| self.intern_inner(module, typed, payload.ty(), payload.span()))
                    .collect::<Result<Vec<_>, _>>()?;
                let payload = module
                    .add_aggregate_type(
                        format!(
                            "enum#d{}.u{}.case{variant}.payload",
                            declaration.index(),
                            ty.index()
                        ),
                        fields,
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                payloads.push((case, payload));
            }
            let tagged = module
                .add_tagged_union_type(
                    format!("enum#d{}.u{}", declaration.index(), ty.index()),
                    payloads.iter().map(|(_, payload)| *payload).collect(),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            for (variant, (case, payload)) in payloads.into_iter().enumerate() {
                self.type_ids.insert(case.case_type(), tagged);
                self.enum_payloads
                    .insert((tagged, case.value_symbol()), (variant, payload));
                self.enum_payloads
                    .insert((tagged, case.type_symbol()), (variant, payload));
            }
            Ok(tagged)
        })();
        self.active_inline.remove(&ty);
        result
    }
}

pub(super) fn is_supported_storage_type(typed: &CompilationUnitTypes, ty: UnitTypeId) -> bool {
    match typed.types().get(ty) {
        Some(UnitTypeKind::Builtin(builtin)) => matches!(
            builtin,
            BuiltinType::Boolean
                | BuiltinType::Byte
                | BuiltinType::UByte
                | BuiltinType::Short
                | BuiltinType::UShort
                | BuiltinType::Int
                | BuiltinType::UInt
                | BuiltinType::Long
                | BuiltinType::ULong
                | BuiltinType::String
                | BuiltinType::Unit
        ),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc | IntrinsicTypeConstructor::Box,
            arguments,
        }) => {
            matches!(arguments.as_slice(), [payload] if is_supported_storage_type(typed, *payload))
        }
        Some(UnitTypeKind::Intrinsic {
            constructor:
                IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList,
            arguments,
        }) => {
            matches!(arguments.as_slice(), [element] if is_supported_storage_type(typed, *element))
        }
        Some(UnitTypeKind::Nullable(inner)) => is_supported_nullable_inner(typed, *inner),
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| {
                arguments.len() == nominal.type_parameters().len()
                    && if nominal.type_parameters().is_empty() {
                        matches!(
                            nominal.kind(),
                            NominalKind::Class
                                | NominalKind::ValueClass
                                | NominalKind::EnumClass
                                | NominalKind::Object
                        )
                    } else {
                        nominal.kind() == NominalKind::Class
                            && resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)
                                .is_ok()
                    }
            }),
        _ => false,
    }
}

fn is_supported_nullable_inner(typed: &CompilationUnitTypes, ty: UnitTypeId) -> bool {
    match typed.types().get(ty) {
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc | IntrinsicTypeConstructor::Box,
            arguments,
        }) => {
            matches!(arguments.as_slice(), [payload] if is_supported_storage_type(typed, *payload))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| {
                nominal.kind() == NominalKind::Class
                    && arguments.len() == nominal.type_parameters().len()
                    && (nominal.type_parameters().is_empty()
                        || resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)
                            .is_ok())
            }),
        _ => false,
    }
}

const fn integer_type(bits: u16, signed: bool) -> SsaTypeKind {
    SsaTypeKind::Integer { bits, signed }
}
