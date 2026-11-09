//! Concrete callable and closure type construction.

use super::model::{
    CallableSignature, ClosureCaptureMode, ClosureCaptureType, EntityType, ModelError, Module,
    SsaTypeId, SsaTypeKind,
};

impl Module {
    #[cfg(test)]
    pub(crate) fn add_function_pointer_type(
        &mut self,
        parameters: Vec<SsaTypeId>,
        returns: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        self.add_function_pointer_type_with_parameters(
            parameters.into_iter().map(EntityType::Value).collect(),
            returns,
        )
    }

    #[cfg(test)]
    pub(crate) fn add_function_pointer_type_with_parameters(
        &mut self,
        parameters: Vec<EntityType>,
        returns: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        self.add_function_pointer_signature(CallableSignature {
            parameters,
            returns,
            never_returns: false,
        })
    }

    pub(crate) fn add_function_pointer_signature(
        &mut self,
        signature: CallableSignature,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_callable_signature(&signature)?;
        Ok(self.intern_type(SsaTypeKind::FunctionPointer { signature }))
    }

    /// The ADR-0034 callable view loan target for `signature`.
    pub(crate) fn add_callable_view_type(
        &mut self,
        signature: CallableSignature,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_callable_signature(&signature)?;
        Ok(self.intern_type(SsaTypeKind::CallableView { signature }))
    }

    /// The signature of a callable view loan target.
    pub(crate) fn callable_view_signature(&self, ty: SsaTypeId) -> Option<&CallableSignature> {
        match self.type_kind(ty)? {
            SsaTypeKind::CallableView { signature } => Some(signature),
            _ => None,
        }
    }

    pub(crate) fn add_shared_reference_type(
        &mut self,
        target: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(target)?;
        Ok(self.intern_type(SsaTypeKind::SharedReference { target }))
    }

    #[cfg(test)]
    pub(crate) fn add_concrete_closure_type(
        &mut self,
        name: impl Into<String>,
        parameters: Vec<SsaTypeId>,
        returns: Vec<SsaTypeId>,
        environment: SsaTypeId,
        captures: Vec<ClosureCaptureType>,
    ) -> Result<SsaTypeId, ModelError> {
        self.add_concrete_closure_type_with_parameters(
            name,
            parameters.into_iter().map(EntityType::Value).collect(),
            returns,
            environment,
            captures,
        )
    }

    #[cfg(test)]
    pub(crate) fn add_concrete_closure_type_with_parameters(
        &mut self,
        name: impl Into<String>,
        parameters: Vec<EntityType>,
        returns: Vec<SsaTypeId>,
        environment: SsaTypeId,
        captures: Vec<ClosureCaptureType>,
    ) -> Result<SsaTypeId, ModelError> {
        self.add_concrete_closure_signature(
            name,
            CallableSignature {
                parameters,
                returns,
                never_returns: false,
            },
            environment,
            captures,
        )
    }

    pub(crate) fn add_concrete_closure_signature(
        &mut self,
        name: impl Into<String>,
        signature: CallableSignature,
        environment: SsaTypeId,
        captures: Vec<ClosureCaptureType>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        self.check_type_id(environment)?;
        if captures.is_empty() {
            return Err(ModelError::EmptyClosureCaptures);
        }
        self.check_callable_signature(&signature)?;
        let fields = self
            .aggregate_fields(environment)
            .ok_or(ModelError::ExpectedAggregate { ty: environment })?;
        let expected_fields = captures
            .iter()
            .map(|capture| {
                self.check_type_id(capture.ty)?;
                Ok(match capture.mode {
                    ClosureCaptureMode::Owned => capture.ty,
                    ClosureCaptureMode::Shared => self
                        .types
                        .iter()
                        .enumerate()
                        .find_map(|(index, kind)| {
                            (*kind == SsaTypeKind::SharedReference { target: capture.ty })
                                .then_some(SsaTypeId {
                                    module: self.id,
                                    index,
                                })
                        })
                        .ok_or(ModelError::InvalidClosureEnvironment)?,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        if fields != expected_fields {
            return Err(ModelError::InvalidClosureEnvironment);
        }
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::ConcreteClosure {
                name,
                signature,
                environment,
                captures,
            },
        ))
    }

    pub(crate) fn callable_signature(&self, ty: SsaTypeId) -> Option<&CallableSignature> {
        match self.type_kind(ty)? {
            SsaTypeKind::FunctionPointer { signature }
            | SsaTypeKind::ConcreteClosure { signature, .. } => Some(signature),
            _ => None,
        }
    }

    pub(crate) fn concrete_closure(
        &self,
        ty: SsaTypeId,
    ) -> Option<(SsaTypeId, &[ClosureCaptureType])> {
        match self.type_kind(ty)? {
            SsaTypeKind::ConcreteClosure {
                environment,
                captures,
                ..
            } => Some((*environment, captures)),
            _ => None,
        }
    }

    fn check_callable_signature(&self, signature: &CallableSignature) -> Result<(), ModelError> {
        if signature.returns.len() > 1 || (signature.never_returns && !signature.returns.is_empty())
        {
            return Err(ModelError::InvalidCallableReturnArity);
        }
        for parameter in &signature.parameters {
            if matches!(parameter, EntityType::Place(_)) {
                return Err(ModelError::InvalidCallableParameter);
            }
            self.check_type_id(parameter.semantic_type())?;
        }
        for ty in &signature.returns {
            self.check_type_id(*ty)?;
        }
        Ok(())
    }
}
