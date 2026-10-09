use super::*;

impl SignatureCollector<'_> {
    pub(super) fn callable_signature(
        &mut self,
        source: SourceUnitId,
        item: ItemId,
        target: UnitCallableTarget,
        receiver_owner: Option<(UnitTypeId, NominalKind)>,
    ) -> Result<UnitCallableSignature, CompilationUnitTypeError> {
        let visibility = item_visibility(self.inputs[source.index()].ast(), item)?;
        let modifiers = item_modifiers(self.inputs[source.index()].ast(), item)?;
        let item = unwrapped_item(self.inputs[source.index()].ast(), item)?;
        let Item::Function {
            name,
            type_parameters,
            parameters,
            form,
            ..
        } = item
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let name_span = marker_span(*name);
        let receiver = receiver_owner.and_then(|(ty, kind)| {
            let mode = parameter_mode(modifiers.receiver_mode);
            (kind != NominalKind::Object || mode == ParameterMode::Borrow).then(|| {
                let ty = if kind == NominalKind::Interface {
                    self.types.intern(UnitTypeKind::StaticSelf(ty))
                } else {
                    ty
                };
                UnitCallableReceiver::new(
                    mode,
                    ty,
                    name_span,
                    modifiers.receiver_mode.map(parameter_mode_marker_span),
                )
            })
        });
        if receiver_owner.is_some()
            && receiver.is_none()
            && let Some(marker) = modifiers.receiver_mode
        {
            self.emit(
                codes::INTERFACE_MEMBER_MISMATCH,
                "object instance member receiver must be Borrow",
                parameter_mode_marker_span(marker),
            )?;
        }
        let type_parameters = type_parameters
            .iter()
            .filter_map(|parameter| self.marker_symbol(source, parameter.name))
            .collect::<Vec<_>>();
        let parameters = parameters
            .iter()
            .map(|parameter| self.callable_parameter(source, parameter))
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = match form {
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => {
                self.builtin(BuiltinType::Unit)
            }
            FunctionForm::Explicit { type_ref, .. } => self.resolve_type_ref(source, *type_ref)?,
        };
        let callable_type = self.types.intern(UnitTypeKind::Function {
            move_only: false,
            parameters: parameters
                .iter()
                .map(|parameter| UnitFunctionParameterType::new(parameter.mode(), parameter.ty()))
                .collect(),
            return_type,
        });
        if let UnitCallableTarget::Symbol(symbol) = target {
            self.symbol_types.insert(symbol, callable_type);
        }
        Ok(UnitCallableSignature::new(
            target,
            self.marker_text(*name)?.unwrap_or_default(),
            name_span,
            type_parameters,
            receiver,
            parameters,
            return_type,
            callable_type,
            visibility,
            match form {
                FunctionForm::ImplicitUnitAbsent => false,
                FunctionForm::ImplicitUnitBlock(_) => true,
                FunctionForm::Explicit { body, .. } => {
                    !matches!(body, crate::parser::FunctionBody::Absent)
                }
            },
        ))
    }

    fn callable_parameter(
        &mut self,
        source: SourceUnitId,
        parameter: &ValueParameter,
    ) -> Result<UnitCallableParameter, CompilationUnitTypeError> {
        let symbol = self.marker_symbol(source, parameter.name);
        let ty = self.resolve_type_ref(source, parameter.type_ref)?;
        if let Some(symbol) = symbol {
            self.symbol_types.insert(symbol, ty);
        }
        Ok(UnitCallableParameter::new(
            symbol,
            self.marker_text(parameter.name)?,
            parameter_mode(parameter.mode_marker),
            ty,
            parameter.span,
        ))
    }
}
