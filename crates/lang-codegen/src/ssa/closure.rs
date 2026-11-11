//! Concrete callable, closure, Map-result / nullable-Map type construction, and integer operator mappings.

pub(in crate::ssa) mod integer {
    use crate::ssa::model::{
        CheckedArithmeticOperator, ComparisonOperator, IntegerBitwiseOperator,
    };
    #[cfg(test)]
    use crate::ssa::model::{Operation, ValueId};
    use lang_frontend::{
        ast::ExpressionId,
        parser::{AssignmentOperator, BinaryOperator, Expression, ParsedFile},
        type_checking::{BuiltinType, IntegerRangeOperationKind},
    };

    pub(in crate::ssa) const fn checked_operator(
        operator: BinaryOperator,
    ) -> Option<CheckedArithmeticOperator> {
        match operator {
            BinaryOperator::Add => Some(CheckedArithmeticOperator::Add),
            BinaryOperator::Subtract => Some(CheckedArithmeticOperator::Subtract),
            BinaryOperator::Multiply => Some(CheckedArithmeticOperator::Multiply),
            BinaryOperator::Divide => Some(CheckedArithmeticOperator::Divide),
            BinaryOperator::Remainder => Some(CheckedArithmeticOperator::Remainder),
            _ => None,
        }
    }

    pub(in crate::ssa) const fn assignment_operator(
        operator: AssignmentOperator,
    ) -> Option<CheckedArithmeticOperator> {
        match operator {
            AssignmentOperator::AddAssign => Some(CheckedArithmeticOperator::Add),
            AssignmentOperator::SubtractAssign => Some(CheckedArithmeticOperator::Subtract),
            AssignmentOperator::MultiplyAssign => Some(CheckedArithmeticOperator::Multiply),
            AssignmentOperator::DivideAssign => Some(CheckedArithmeticOperator::Divide),
            AssignmentOperator::RemainderAssign => Some(CheckedArithmeticOperator::Remainder),
            AssignmentOperator::Assign => None,
        }
    }

    pub(in crate::ssa) const fn comparison_operator(
        operator: BinaryOperator,
    ) -> Option<ComparisonOperator> {
        match operator {
            BinaryOperator::Equal => Some(ComparisonOperator::Equal),
            BinaryOperator::NotEqual => Some(ComparisonOperator::NotEqual),
            BinaryOperator::Less => Some(ComparisonOperator::LessThan),
            BinaryOperator::LessEqual => Some(ComparisonOperator::LessThanOrEqual),
            BinaryOperator::Greater => Some(ComparisonOperator::GreaterThan),
            BinaryOperator::GreaterEqual => Some(ComparisonOperator::GreaterThanOrEqual),
            _ => None,
        }
    }

    pub(in crate::ssa) const fn bitwise_operator(
        operator: BinaryOperator,
    ) -> Option<IntegerBitwiseOperator> {
        match operator {
            BinaryOperator::BitwiseAnd => Some(IntegerBitwiseOperator::And),
            BinaryOperator::BitwiseOr => Some(IntegerBitwiseOperator::Or),
            BinaryOperator::BitwiseXor => Some(IntegerBitwiseOperator::Xor),
            BinaryOperator::Shl => Some(IntegerBitwiseOperator::Shl),
            BinaryOperator::Shr => Some(IntegerBitwiseOperator::Shr),
            BinaryOperator::Ushr => Some(IntegerBitwiseOperator::Ushr),
            _ => None,
        }
    }

    pub(in crate::ssa) fn intrinsic_receiver_matches(
        parsed: &ParsedFile,
        expression: ExpressionId,
        receiver: ExpressionId,
    ) -> bool {
        let Ok(call) = parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Call {
            callee,
            type_arguments,
            arguments,
            ..
        } = call.payload()
        else {
            return false;
        };
        if !type_arguments.is_empty() || !arguments.is_empty() {
            return false;
        }
        let Ok(callee) = parsed.ast().expressions().get(*callee) else {
            return false;
        };
        matches!(callee.payload(), Expression::Member { receiver: actual, safe: false, .. } if *actual == receiver)
    }

    pub(in crate::ssa) fn matches_range_fact(
        parsed: &ParsedFile,
        expression: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        kind: IntegerRangeOperationKind,
        types: [Option<BuiltinType>; 3],
    ) -> bool {
        use IntegerRangeOperationKind as K;
        let Ok(node) = parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Binary {
            left: actual_left,
            right: actual_right,
            operator,
            ..
        } = node.payload()
        else {
            return false;
        };
        let expected = match kind {
            K::Inclusive => BinaryOperator::InclusiveRange,
            K::Exclusive => BinaryOperator::ExclusiveRange,
            K::Contains => BinaryOperator::In,
            K::NotContains => BinaryOperator::NotIn,
            K::Equal => BinaryOperator::Equal,
            K::NotEqual => BinaryOperator::NotEqual,
        };
        if *actual_left != left || *actual_right != right || *operator != expected {
            return false;
        }
        let endpoint = |range| match range {
            Some(BuiltinType::IntRange) => Some(BuiltinType::Int),
            Some(BuiltinType::LongRange) => Some(BuiltinType::Long),
            _ => None,
        };
        match kind {
            K::Inclusive | K::Exclusive => {
                endpoint(types[2]).is_some()
                    && types[0] == endpoint(types[2])
                    && types[1] == types[0]
            }
            K::Contains | K::NotContains => {
                endpoint(types[1]).is_some()
                    && types[0] == endpoint(types[1])
                    && types[2] == Some(BuiltinType::Boolean)
            }
            K::Equal | K::NotEqual => {
                endpoint(types[0]).is_some()
                    && types[1] == types[0]
                    && types[2] == Some(BuiltinType::Boolean)
            }
        }
    }

    #[cfg(test)]
    pub(in crate::ssa) fn range_operation(
        kind: IntegerRangeOperationKind,
        left: ValueId,
        right: ValueId,
    ) -> (Operation, bool) {
        use IntegerRangeOperationKind as K;
        match kind {
            K::Inclusive | K::Exclusive => (
                Operation::IntegerRangeConstruct {
                    first: left,
                    bound: right,
                    inclusive: kind == K::Inclusive,
                },
                false,
            ),
            K::Contains | K::NotContains => (
                Operation::IntegerRangeContains {
                    value: left,
                    range: right,
                },
                kind == K::NotContains,
            ),
            K::Equal | K::NotEqual => (
                Operation::IntegerRangeEqual { left, right },
                kind == K::NotEqual,
            ),
        }
    }
}

use super::model::{
    CallableSignature, ClosureCaptureMode, ClosureCaptureType, EntityType, ModelError, Module,
    Ownership, SsaTypeId, SsaTypeKind,
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

    pub(crate) fn add_map_result_type(
        &mut self,
        value: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(value)?;
        if matches!(
            self.type_kind(value),
            Some(SsaTypeKind::MapContainer { .. })
        ) {
            return self.add_nullable_map_type(value);
        }
        if !matches!(
            self.type_ownership(value),
            Some(Ownership::Copyable | Ownership::MoveOnly)
        ) {
            return Err(ModelError::ExpectedAggregate { ty: value });
        }
        if let Some((&result, _)) = self.map_results.iter().find(|(_, inner)| **inner == value) {
            return Ok(result);
        }
        let present = self.intern_type(SsaTypeKind::Boolean);
        let result = self.add_aggregate_type(
            format!("$map.result.t{}", value.index()),
            vec![present, value],
        )?;
        self.map_results.insert(result, value);
        Ok(result)
    }

    pub(crate) fn map_result_value(&self, result: SsaTypeId) -> Option<SsaTypeId> {
        self.map_results.get(&result).copied()
    }

    pub(crate) fn add_nullable_map_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        if !self.type_is_defined(inner)
            || !matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::MapContainer { .. })
            )
            || self.type_ownership(inner) != Some(Ownership::MoveOnly)
        {
            return Err(ModelError::ExpectedMapContainer { ty: inner });
        }
        if let Some((&nullable, _)) = self
            .nullable_maps
            .iter()
            .find(|(_, value)| **value == inner)
        {
            return Ok(nullable);
        }
        let present = self.intern_type(SsaTypeKind::Boolean);
        let nullable = self.add_aggregate_type(
            format!("$nullable.map.t{}", inner.index()),
            vec![present, inner],
        )?;
        self.nullable_maps.insert(nullable, inner);
        Ok(nullable)
    }

    pub(crate) fn nullable_map_inner(&self, nullable: SsaTypeId) -> Option<SsaTypeId> {
        self.nullable_maps.get(&nullable).copied()
    }

    /// Only physical nullable layout/drop/encapsulation may use this union.
    /// MapGet, MapResultUnwrap and scalar comparisons retain their R-only contracts.
    pub(crate) fn presence_payload_type(&self, wrapper: SsaTypeId) -> Option<SsaTypeId> {
        self.nullable_map_inner(wrapper)
            .or_else(|| self.map_result_value(wrapper))
    }

    pub(super) fn valid_nullable_map_type(&self, nullable: SsaTypeId, inner: SsaTypeId) -> bool {
        let Some([present, payload]) = self.aggregate_fields(nullable) else {
            return false;
        };
        self.type_is_defined(nullable)
            && self.type_is_defined(inner)
            && matches!(self.type_kind(*present), Some(SsaTypeKind::Boolean))
            && *payload == inner
            && matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::MapContainer { .. })
            )
            && self.type_ownership(inner) == Some(Ownership::MoveOnly)
            && self.type_ownership(nullable) == Some(Ownership::MoveOnly)
    }

    pub(super) fn valid_map_result_type(&self, result: SsaTypeId, value: SsaTypeId) -> bool {
        let Some([present, payload]) = self.aggregate_fields(result) else {
            return false;
        };
        matches!(self.type_kind(*present), Some(SsaTypeKind::Boolean))
            && *payload == value
            && !self.nullable_maps.contains_key(&result)
            && !matches!(
                self.type_kind(value),
                Some(SsaTypeKind::MapContainer { .. })
            )
            && self.type_ownership(value).is_some()
            && self.type_ownership(result) == self.type_ownership(value)
    }
}
