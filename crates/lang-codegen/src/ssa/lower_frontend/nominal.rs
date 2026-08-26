//! Frontend concrete nominal instances to target-independent SSA storage types.

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::{EnumCaseId, NameResolution, SymbolId},
    source::Span,
    type_checking::{
        BuiltinType, ConstructionTarget, IntrinsicTypeConstructor, NominalId, NominalKind, TypeId,
        TypeKind, TypedFile,
    },
};

use super::{LoweringError, LoweringErrorKind, error};
use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind, TypeOrigin};

type FrontendTypeMap = BTreeMap<TypeId, SsaTypeId>;
type HeapPayloadMap = BTreeMap<SsaTypeId, SsaTypeId>;
type EnumPayloadMap = BTreeMap<(SsaTypeId, EnumCaseId), (usize, SsaTypeId)>;

pub(super) struct NominalTypeMapper {
    type_ids: FrontendTypeMap,
    heap_payloads: HeapPayloadMap,
    construction_fields: BTreeMap<TypeId, Vec<TypeId>>,
    enum_construction_fields: BTreeMap<(TypeId, EnumCaseId), Vec<TypeId>>,
    enum_payloads: EnumPayloadMap,
    active: BTreeSet<TypeId>,
}

impl NominalTypeMapper {
    pub(super) fn new(typed: &TypedFile) -> Result<Self, LoweringError> {
        let mut construction_fields = BTreeMap::new();
        let mut enum_construction_fields = BTreeMap::new();
        for construction in typed.constructions() {
            let fields = construction
                .arguments()
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    (argument.parameter_index() == index).then_some(argument.parameter_type())
                })
                .collect::<Option<Vec<_>>>()
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            let inserted = match construction.target() {
                ConstructionTarget::EnumCase(case) => insert_fields(
                    &mut enum_construction_fields,
                    (construction.result_type(), case),
                    fields,
                ),
                ConstructionTarget::Nominal(_)
                | ConstructionTarget::IntrinsicBox
                | ConstructionTarget::IntrinsicRc => {
                    insert_fields(&mut construction_fields, construction.result_type(), fields)
                }
            };
            if !inserted {
                return Err(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                });
            }
        }
        Ok(Self {
            type_ids: BTreeMap::new(),
            heap_payloads: BTreeMap::new(),
            construction_fields,
            enum_construction_fields,
            enum_payloads: BTreeMap::new(),
            active: BTreeSet::new(),
        })
    }

    pub(super) fn into_parts(self) -> (FrontendTypeMap, HeapPayloadMap, EnumPayloadMap) {
        (self.type_ids, self.heap_payloads, self.enum_payloads)
    }

    pub(super) fn intern(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        ty: TypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        if let Some(mapped) = self.type_ids.get(&ty).copied() {
            return Ok(mapped);
        }
        let mapped = match typed.types().get(ty) {
            Some(TypeKind::Builtin(builtin)) => intern_builtin(module, *builtin, span)?,
            Some(TypeKind::Nominal { nominal, .. }) => {
                self.intern_nominal(module, names, typed, ty, *nominal, span)?
            }
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                arguments,
            }) => {
                let [payload] = arguments.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let payload_type = *payload;
                let owner = module
                    .declare_heap_owner(format!("Box#t{}", ty.index()))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                self.type_ids.insert(ty, owner);
                let payload = self.intern(module, names, typed, payload_type, span)?;
                module
                    .define_heap_owner(owner, payload)
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                self.heap_payloads.insert(owner, payload);
                module.set_type_origin(
                    owner,
                    TypeOrigin {
                        primary: span,
                        declaration: declaration_span_for_type(names, typed, payload_type, span),
                    },
                );
                owner
            }
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                arguments,
            }) => {
                let [payload] = arguments.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let payload_type = *payload;
                let owner = module
                    .declare_shared_owner(format!("Rc#t{}", ty.index()))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                self.type_ids.insert(ty, owner);
                let payload = self.intern(module, names, typed, payload_type, span)?;
                module
                    .define_shared_owner(owner, payload)
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                module.set_type_origin(
                    owner,
                    TypeOrigin {
                        primary: span,
                        declaration: declaration_span_for_type(names, typed, payload_type, span),
                    },
                );
                owner
            }
            Some(TypeKind::Nullable(inner)) => {
                let inner = self.intern(module, names, typed, *inner, span)?;
                module
                    .add_nullable_handle_type(inner)
                    .map_err(|_| error(LoweringErrorKind::UnsupportedNode, span))?
            }
            Some(_) => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            None => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        self.type_ids.insert(ty, mapped);
        Ok(mapped)
    }

    fn intern_nominal(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        ty: TypeId,
        nominal: NominalId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let descriptor = typed
            .nominals()
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let base_name = symbol_name(names, nominal.symbol(), span)?;
        let declaration = symbol_span(names, nominal.symbol()).unwrap_or(span);
        if descriptor.kind() == NominalKind::Class {
            let owner = module
                .declare_heap_owner(format!("{base_name}#t{}", ty.index()))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.type_ids.insert(ty, owner);
            let fields = self.field_types(typed, ty, descriptor.fields(), span)?;
            let fields = fields
                .into_iter()
                .map(|field| self.intern(module, names, typed, field, span))
                .collect::<Result<Vec<_>, _>>()?;
            let payload = module
                .add_aggregate_type(format!("{base_name}#t{}.payload", ty.index()), fields)
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            module
                .define_heap_owner(owner, payload)
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.heap_payloads.insert(owner, payload);
            let origin = TypeOrigin {
                primary: span,
                declaration,
            };
            module.set_type_origin(payload, origin);
            module.set_type_origin(owner, origin);
            return Ok(owner);
        }
        if descriptor.kind() == NominalKind::EnumClass {
            return self.intern_enum(module, names, typed, ty, nominal, base_name, span);
        }
        if descriptor.kind() != NominalKind::ValueClass || !self.active.insert(ty) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let fields = self.field_types(typed, ty, descriptor.fields(), span)?;
        let fields = fields
            .into_iter()
            .map(|field| self.intern(module, names, typed, field, span))
            .collect::<Result<Vec<_>, _>>()?;
        self.active.remove(&ty);
        let aggregate = module
            .add_aggregate_type(format!("{base_name}#t{}", ty.index()), fields)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        module.set_type_origin(
            aggregate,
            TypeOrigin {
                primary: span,
                declaration,
            },
        );
        Ok(aggregate)
    }

    #[allow(clippy::too_many_arguments)]
    fn intern_enum(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        ty: TypeId,
        nominal: NominalId,
        base_name: &str,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        if !self.active.insert(ty) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let cases = typed
            .enum_cases()
            .iter()
            .filter(|case| case.root() == nominal)
            .collect::<Vec<_>>();
        if cases.is_empty() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut payloads = Vec::with_capacity(cases.len());
        for (index, case) in cases.iter().enumerate() {
            let fields = self
                .enum_construction_fields
                .get(&(ty, case.id()))
                .cloned()
                .unwrap_or_else(|| case.payloads().iter().map(|(_, ty)| *ty).collect());
            let fields = fields
                .into_iter()
                .map(|field| self.intern(module, names, typed, field, span))
                .collect::<Result<Vec<_>, _>>()?;
            let payload = module
                .add_aggregate_type(
                    format!("{base_name}#t{}.case{index}.payload", ty.index()),
                    fields,
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            module.set_type_origin(
                payload,
                TypeOrigin {
                    primary: span,
                    declaration: symbol_span(names, case.type_symbol()).unwrap_or(span),
                },
            );
            payloads.push((case.id(), payload));
        }
        self.active.remove(&ty);
        let tagged = module
            .add_tagged_union_type(
                format!("{base_name}#t{}", ty.index()),
                payloads.iter().map(|(_, payload)| *payload).collect(),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        module.set_type_origin(
            tagged,
            TypeOrigin {
                primary: span,
                declaration: symbol_span(names, nominal.symbol()).unwrap_or(span),
            },
        );
        self.type_ids.insert(ty, tagged);
        for (index, (case, payload)) in payloads.into_iter().enumerate() {
            self.enum_payloads.insert((tagged, case), (index, payload));
        }
        Ok(tagged)
    }

    fn field_types(
        &self,
        typed: &TypedFile,
        instance: TypeId,
        fields: &[SymbolId],
        span: Span,
    ) -> Result<Vec<TypeId>, LoweringError> {
        if let Some(fields) = self.construction_fields.get(&instance) {
            return Ok(fields.clone());
        }
        fields
            .iter()
            .map(|field| {
                let ty = typed
                    .symbol_type(*field)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if matches!(typed.types().get(ty), Some(TypeKind::TypeParameter(_))) {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                Ok(ty)
            })
            .collect()
    }
}

fn insert_fields<K: Ord>(
    layouts: &mut BTreeMap<K, Vec<TypeId>>,
    key: K,
    fields: Vec<TypeId>,
) -> bool {
    match layouts.entry(key) {
        std::collections::btree_map::Entry::Occupied(existing) => existing.get() == &fields,
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(fields);
            true
        }
    }
}

fn symbol_span(names: &NameResolution, symbol: SymbolId) -> Option<Span> {
    names
        .symbols()
        .get(symbol.index())
        .map(|symbol| symbol.span())
}

fn declaration_span_for_type(
    names: &NameResolution,
    typed: &TypedFile,
    ty: TypeId,
    fallback: Span,
) -> Span {
    match typed.types().get(ty) {
        Some(TypeKind::Nominal { nominal, .. }) => {
            symbol_span(names, nominal.symbol()).unwrap_or(fallback)
        }
        Some(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            arguments,
        }) => arguments
            .first()
            .map(|payload| declaration_span_for_type(names, typed, *payload, fallback))
            .unwrap_or(fallback),
        _ => fallback,
    }
}

fn symbol_name(
    names: &NameResolution,
    symbol: SymbolId,
    span: Span,
) -> Result<&str, LoweringError> {
    names
        .symbols()
        .get(symbol.index())
        .map(|symbol| symbol.name())
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
}

fn intern_builtin(
    module: &mut Module,
    builtin: BuiltinType,
    span: Span,
) -> Result<SsaTypeId, LoweringError> {
    let kind = match builtin {
        BuiltinType::Boolean => SsaTypeKind::Boolean,
        BuiltinType::Byte => SsaTypeKind::Integer {
            bits: 8,
            signed: true,
        },
        BuiltinType::UByte => SsaTypeKind::Integer {
            bits: 8,
            signed: false,
        },
        BuiltinType::Short => SsaTypeKind::Integer {
            bits: 16,
            signed: true,
        },
        BuiltinType::UShort => SsaTypeKind::Integer {
            bits: 16,
            signed: false,
        },
        BuiltinType::Int => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        BuiltinType::UInt => SsaTypeKind::Integer {
            bits: 32,
            signed: false,
        },
        BuiltinType::Long => SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        },
        BuiltinType::ULong => SsaTypeKind::Integer {
            bits: 64,
            signed: false,
        },
        BuiltinType::Unit => SsaTypeKind::Unit,
        _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
    };
    Ok(module.intern_type(kind))
}
