//! Callable body scope, receiver context and explicit return validation.
use super::*;

impl BodyChecker<'_> {
    pub(super) fn check_function(
        &mut self,
        source: SourceUnitId,
        form: FunctionForm,
        callable: &UnitCallableSignature,
    ) -> Result<(), CompilationUnitTypeError> {
        let previous_receiver = self.current_receiver;
        let previous_mode = self.current_receiver_mode;
        if callable.range_extension().is_some() {
            self.current_receiver = callable.receiver().map(|r| r.ty());
            self.current_receiver_mode = Some(ParameterMode::Borrow);
        }
        self.callable_loop_bases.push(self.loop_depth);
        let result = self.check_function_body(source, form, callable);
        self.callable_loop_bases.pop();
        self.current_receiver = previous_receiver;
        self.current_receiver_mode = previous_mode;
        result
    }

    fn check_function_body(
        &mut self,
        source: SourceUnitId,
        form: FunctionForm,
        callable: &UnitCallableSignature,
    ) -> Result<(), CompilationUnitTypeError> {
        self.flow_facts.clear();
        let expected_span = match form {
            FunctionForm::Explicit { type_ref, .. } => Some(
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_ref)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            ),
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => None,
        };
        self.current_return_span = expected_span;
        match form {
            FunctionForm::ImplicitUnitAbsent => Ok(()),
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(source, body, callable.return_type(), expected_span)?;
                Ok(())
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => Ok(()),
                FunctionBody::Expression { expression, .. } => {
                    self.check_expression(
                        source,
                        expression,
                        Some(callable.return_type()),
                        expected_span,
                        callable.return_type(),
                    )?;
                    Ok(())
                }
                FunctionBody::Block(body) => {
                    let result =
                        self.check_statement(source, body, callable.return_type(), expected_span)?;
                    if result.falls_through
                        && !self.is_builtin(callable.return_type(), BuiltinType::Unit)
                        && !self.is_error(callable.return_type())
                    {
                        let primary = self
                            .file(source)
                            .ast()
                            .statements()
                            .get(body)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit_maybe_label(
                            codes::MISSING_RETURN,
                            "non-Unit function can reach the end of its body",
                            self.sources
                                .span(primary.source_id(), primary.end(), primary.end())
                                .map_err(TypeCheckingError::from)?,
                            expected_span,
                            "function return type declared here",
                        )?;
                    }
                    Ok(())
                }
            },
        }
    }
}
