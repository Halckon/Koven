//! callable 签名、参数与普通借用返回声明合同的采集。
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
        if !matches!(
            item,
            Item::Function {
                extension_receiver: Some(_),
                ..
            }
        ) && let Some((span, message)) =
            crate::type_checking::declaration_frontier::unsupported_declaration(
                item,
                self.environment
                    .is_authorized_range_source(self.inputs[source.index()].source_id()),
            )
        {
            self.emit(codes::UNSUPPORTED_BORROW_FLOW, message, span)?;
        }
        let Item::Function {
            name,
            type_parameters,
            parameters,
            form,
            extension_receiver,
            ..
        } = item
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let name_span = marker_span(*name);
        let extension_type = extension_receiver
            .map(|r| self.resolve_type_ref(source, r.type_ref))
            .transpose()?;
        let extension_span = extension_receiver
            .map(|r| {
                self.inputs[source.index()]
                    .ast()
                    .type_refs()
                    .get(r.type_ref)
                    .map(|n| n.span())
            })
            .transpose()
            .map_err(TypeCheckingError::from)?;
        let receiver = extension_span
            .zip(extension_type)
            .map(|(r, ty)| {
                UnitCallableReceiver::new(
                    parameter_mode(modifiers.receiver_mode),
                    ty,
                    r,
                    modifiers.receiver_mode.map(parameter_mode_marker_span),
                )
            })
            .or_else(|| {
                receiver_owner.and_then(|(ty, kind)| {
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
        let result_source = match crate::type_checking::result_source::resolve_result_source(
            self.sources,
            *form,
            parameters,
            receiver.map(|r| r.mode()),
        )
        .map_err(TypeCheckingError::from)?
        {
            Ok(contract) => contract,
            Err(issue) => {
                self.emit(codes::INVALID_BORROW_CONTRACT, issue.message, issue.span)?;
                crate::type_checking::CallableResultSource::Owned
            }
        };
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
        self.check_carrier_contract(result_source, return_type, &parameters, receiver)?;
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
        let range_extension = receiver
            .filter(|_| receiver_owner.is_none())
            .and_then(|r| self.bind_range_extension(source, target, r, return_type, result_source));
        let extension_receiver_symbol = range_extension
            .and_then(|binding| {
                self.names.source_units()[source.index()]
                    .resolution()
                    .receiver_symbol(binding.receiver_span())
            })
            .map(|symbol| UnitSymbolId::new(source, symbol));
        if let Some(symbol) = extension_receiver_symbol
            && let Some(receiver) = receiver
        {
            self.symbol_types.insert(symbol, receiver.ty());
        }
        if let Some(extension) = extension_receiver
            && range_extension.is_none()
        {
            self.emit(
                codes::UNSUPPORTED_BORROW_FLOW,
                "extension receiver lacks a trusted canonical Borrow List/View binding",
                extension.dot_span,
            )?;
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
        )
        .with_result_source(result_source)
        .with_range_extension(range_extension)
        .with_extension_receiver_symbol(extension_receiver_symbol))
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
