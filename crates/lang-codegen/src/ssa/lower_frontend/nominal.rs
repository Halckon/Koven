//! Frontend concrete nominal instances to target-independent SSA storage types.

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::{NameResolution, SymbolId},
    source::Span,
    type_checking::{
        BuiltinType, ConstructionTarget, IntrinsicTypeConstructor, NominalId, NominalKind, TypeId,
        TypeKind, TypedFile,
    },
};

use super::{LoweringError, LoweringErrorKind, error};
use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

pub(super) struct NominalTypeMapper {
    type_ids: BTreeMap<TypeId, SsaTypeId>,
    heap_payloads: BTreeMap<SsaTypeId, SsaTypeId>,
    construction_fields: BTreeMap<TypeId, Vec<TypeId>>,
    active: BTreeSet<TypeId>,
}

impl NominalTypeMapper {
    pub(super) fn new(typed: &TypedFile) -> Result<Self, LoweringError> {
        let mut construction_fields = BTreeMap::new();
        for construction in typed.constructions() {
            if matches!(construction.target(), ConstructionTarget::EnumCase(_)) {
                continue;
            }
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
            match construction_fields.get(&construction.result_type()) {
                Some(existing) if existing != &fields => {
                    return Err(LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    });
                }
                Some(_) => {}
                None => {
                    construction_fields.insert(construction.result_type(), fields);
                }
            }
        }
        Ok(Self {
            type_ids: BTreeMap::new(),
            heap_payloads: BTreeMap::new(),
            construction_fields,
            active: BTreeSet::new(),
        })
    }

    pub(super) fn into_parts(
        self,
    ) -> (BTreeMap<TypeId, SsaTypeId>, BTreeMap<SsaTypeId, SsaTypeId>) {
        (self.type_ids, self.heap_payloads)
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
                let owner = module
                    .declare_heap_owner(format!("Box#t{}", ty.index()))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                self.type_ids.insert(ty, owner);
                let payload = self.intern(module, names, typed, *payload, span)?;
                module
                    .define_heap_owner(owner, payload)
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                self.heap_payloads.insert(owner, payload);
                owner
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
            return Ok(owner);
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
        module
            .add_aggregate_type(format!("{base_name}#t{}", ty.index()), fields)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
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
