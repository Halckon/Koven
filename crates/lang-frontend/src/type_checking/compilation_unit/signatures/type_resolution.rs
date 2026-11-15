//! Signature type references, instantiation and environment normalization.
use super::*;

impl SignatureCollector<'_> {
    pub(super) fn resolve_type_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let unit_type_ref = UnitTypeRefId::new(source, id);
        if let Some(ty) = self.type_ref_types.get(&unit_type_ref).copied() {
            return Ok(ty);
        }
        let payload = self.inputs[source.index()]
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let ty = match payload {
            TypeRef::Error => self.error_type(),
            TypeRef::Function {
                move_span,
                parameters,
                return_type,
                ..
            } => {
                let parameters = parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(UnitFunctionParameterType::new(
                            parameter_mode(parameter.mode_marker),
                            self.resolve_type_ref(source, parameter.type_ref)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompilationUnitTypeError>>()?;
                let return_type = self.resolve_type_ref(source, return_type)?;
                self.types.intern(UnitTypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters,
                    return_type,
                })
            }
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                let mut base = if self.reject_enum_case_type(source, id, &segments)? {
                    self.error_type()
                } else {
                    self.resolve_named_type(source, &segments)?
                };
                if nullable_span.is_some() && !self.is_error(base) {
                    base = self.types.intern(UnitTypeKind::Nullable(base));
                }
                base
            }
        };
        self.type_ref_types.insert(unit_type_ref, ty);
        Ok(ty)
    }

    pub(super) fn resolve_named_type(
        &mut self,
        source: SourceUnitId,
        segments: &[TypePathSegment],
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(segment) = segments.last() else {
            return Ok(self.error_type());
        };
        let arguments = segment
            .arguments
            .iter()
            .map(|argument| self.resolve_type_ref(source, *argument))
            .collect::<Result<Vec<_>, _>>()?;
        let target = self
            .reference(source, segment.name_span, Namespace::Type)
            .cloned();
        match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => {
                self.instantiate_nominal(declaration, segment, arguments)
            }
            Some(UnitReferenceTarget::Symbol(symbol)) => {
                self.instantiate_unit_symbol(symbol, segment, arguments)
            }
            Some(UnitReferenceTarget::Symbols(symbols)) if symbols.len() == 1 => {
                self.instantiate_unit_symbol(symbols[0], segment, arguments)
            }
            Some(UnitReferenceTarget::External(external)) => {
                self.instantiate_external(source, external, segment, arguments)
            }
            _ => Ok(self.error_type()),
        }
    }

    pub(super) fn instantiate_nominal(
        &mut self,
        declaration: DeclarationId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(nominal) = self.nominals.get(&declaration) else {
            return Ok(self.error_type());
        };
        if arguments.len() != nominal.type_parameters().len() {
            self.emit_arity(
                segment.name_span,
                self.names.index().declarations()[declaration.index()].name_span(),
                "nominal type has the wrong number of type arguments",
            )?;
            return Ok(self.error_type());
        }
        Ok(self.types.intern(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }))
    }

    pub(super) fn instantiate_unit_symbol(
        &mut self,
        symbol: UnitSymbolId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(ty) = self.symbol_types.get(&symbol).copied() else {
            return Ok(self.error_type());
        };
        if !arguments.is_empty() {
            let label = self.names.source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()
                .get(symbol.symbol().index())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .span();
            self.emit_arity(
                self.argument_primary(symbol.source_unit(), segment),
                label,
                "type parameter does not accept type arguments",
            )?;
            return Ok(self.error_type());
        }
        Ok(ty)
    }

    pub(super) fn instantiate_external(
        &mut self,
        source: SourceUnitId,
        external: crate::name_resolution::ExternalSymbolId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        match self.environment.binding(external).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) => {
                if arguments.is_empty() {
                    Ok(self.builtin(builtin))
                } else {
                    self.emit_with_label(
                        codes::BUILTIN_TYPE_ARGUMENTS,
                        "builtin type does not accept type arguments",
                        self.argument_primary(source, segment),
                        segment.name_span,
                        "builtin type declared here",
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Capability(capability)) => {
                if arguments.is_empty() {
                    Ok(self.types.intern(UnitTypeKind::Capability(capability)))
                } else {
                    self.emit(
                        codes::TYPE_ARGUMENT_ARITY,
                        "compiler capability does not accept type arguments",
                        self.argument_primary(source, segment),
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Intrinsic(constructor)) => {
                if arguments.len() == constructor.expected_type_argument_count() {
                    if constructor == IntrinsicTypeConstructor::Box {
                        return self.instantiate_box_type(source, segment);
                    }
                    Ok(self.types.intern(UnitTypeKind::Intrinsic {
                        constructor,
                        arguments,
                    }))
                } else {
                    self.emit(
                        codes::TYPE_ARGUMENT_ARITY,
                        "intrinsic type has the wrong number of type arguments",
                        segment.name_span,
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Value(ty)) => {
                if arguments.is_empty() {
                    Ok(self.normalize_environment_type(&ty))
                } else {
                    self.emit(
                        codes::BUILTIN_TYPE_ARGUMENTS,
                        "external type does not accept type arguments",
                        self.argument_primary(source, segment),
                    )?;
                    Ok(self.error_type())
                }
            }
            None => Ok(self
                .types
                .intern(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))),
            _ => Ok(self.error_type()),
        }
    }

    pub(super) fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> UnitTypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.types.intern(UnitTypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.normalize_environment_type(&parameter.ty);
                        UnitFunctionParameterType::new(parameter.mode, ty)
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.types.intern(UnitTypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }
}
