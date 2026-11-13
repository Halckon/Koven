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

use crate::ssa::model::{
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

    pub(in crate::ssa) fn valid_nullable_map_type(
        &self,
        nullable: SsaTypeId,
        inner: SsaTypeId,
    ) -> bool {
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

    pub(in crate::ssa) fn valid_map_result_type(
        &self,
        result: SsaTypeId,
        value: SsaTypeId,
    ) -> bool {
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

pub(in crate::ssa) mod closure_content {
    //! Current callable contents at synchronous Borrow reads and source call boundaries.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::ssa::{
        model::{
            ClosureCaptureMode, ClosureCaptureOperand, Definition, EntityId, EntityType, Function,
            FunctionId, Instruction, InstructionId, LoanId, Module, Operation, PlaceAccess,
            TerminatorKind,
        },
        verify::{
            closure_escape::{TypeCaptures, proof::content_index},
            content_flow,
        },
        verify_ownership::{AliasRoots, all_entities},
    };

    #[derive(Clone)]
    pub(in crate::ssa) struct CallableContent {
        pub(in crate::ssa) known: bool,
        pub(in crate::ssa) loans: BTreeSet<LoanId>,
    }

    impl CallableContent {
        fn empty() -> Self {
            Self {
                known: true,
                loans: BTreeSet::new(),
            }
        }
        fn join(&mut self, other: &Self) -> bool {
            let before = (self.known, self.loans.len());
            self.known &= other.known;
            self.loans.extend(&other.loans);
            before != (self.known, self.loans.len())
        }
    }

    type State = Vec<CallableContent>;

    struct Proof<'a> {
        function: &'a Function,
        defaults: State,
        borrowed: Vec<bool>,
        affected: BTreeMap<EntityId, Vec<usize>>,
        view_loans: Vec<bool>,
        trusted_view_fields: Option<&'a BTreeSet<usize>>,
    }

    /// The erased environment is trusted only where every closure formation binds a checked
    /// shared view and no other instruction can call or expose the thunk directly.
    pub(in crate::ssa) fn trusted_view_fields(
        module: &Module,
    ) -> BTreeMap<FunctionId, BTreeSet<usize>> {
        let mut fields = BTreeMap::<_, BTreeSet<_>>::new();
        let mut other_entries = BTreeSet::new();
        for instruction in module.functions.iter().flat_map(|body| &body.instructions) {
            match instruction.operation {
                Operation::ClosureConstruct { thunk, closure, .. } => {
                    let shared = module
                        .concrete_closure(closure)
                        .map(|(_, captures)| {
                            captures
                                .iter()
                                .enumerate()
                                .filter_map(|(index, capture)| {
                                    (capture.mode == ClosureCaptureMode::Shared
                                        && module.callable_view_signature(capture.ty).is_some())
                                    .then_some(index)
                                })
                                .collect::<BTreeSet<_>>()
                        })
                        .unwrap_or_default();
                    fields
                        .entry(thunk)
                        .and_modify(|previous| {
                            previous.retain(|index| shared.contains(index));
                        })
                        .or_insert(shared);
                }
                Operation::DirectCall { callee, .. }
                | Operation::BorrowCall { callee, .. }
                | Operation::RangeCall { callee, .. }
                | Operation::FunctionAddress { target: callee }
                | Operation::ContainerGenerate {
                    initializer: callee,
                    ..
                } => {
                    other_entries.insert(callee);
                }
                _ => {}
            }
        }
        fields.retain(|thunk, fields| !fields.is_empty() && !other_entries.contains(thunk));
        fields
    }

    /// A helper's entry Borrow relies on the caller proving the same current capture contents.
    pub(in crate::ssa) fn borrowed_callable_contents(
        module: &Module,
        function: &Function,
        capture_types: &TypeCaptures,
        aliases: &AliasRoots,
        trusted_view_fields: Option<&BTreeSet<usize>>,
    ) -> BTreeMap<(InstructionId, EntityId), CallableContent> {
        if !function
            .instructions
            .iter()
            .any(|instruction| !callable_reads(module, function, instruction).is_empty())
        {
            return BTreeMap::new();
        }
        let proof = Proof::new(
            module,
            function,
            capture_types,
            aliases,
            trusted_view_fields,
        );
        let entries = content_flow::block_entries(
            function,
            &proof.defaults,
            |block, state| {
                for id in &block.instructions {
                    proof.apply(
                        function.instruction(*id).expect("verified instruction"),
                        state,
                    );
                }
                proof.apply_terminator(
                    &block.terminator.as_ref().expect("verified terminator").kind,
                    state,
                );
            },
            |edge, state| {
                let target = function.block(edge.target).expect("verified target");
                let rebindings = edge
                    .arguments
                    .iter()
                    .zip(&target.parameters)
                    .filter_map(|(argument, parameter)| match (argument, parameter) {
                        (EntityId::Loan(argument), EntityId::Loan(parameter)) => {
                            Some((*argument, *parameter))
                        }
                        _ => None,
                    })
                    .collect::<BTreeMap<_, _>>();
                let rebound = edge
                    .arguments
                    .iter()
                    .zip(&target.parameters)
                    .map(|(argument, parameter)| {
                        let mut content = state[content_index(function, *argument)].clone();
                        content.loans = content
                            .loans
                            .iter()
                            .map(|loan| rebindings.get(loan).copied().unwrap_or(*loan))
                            .collect();
                        (content_index(function, *parameter), content)
                    })
                    .collect::<Vec<_>>();
                let mut incoming = state.clone();
                for (parameter, content) in rebound {
                    incoming[parameter] = content;
                }
                incoming
            },
            |previous, incoming| {
                let mut changed = false;
                for (previous, incoming) in previous.iter_mut().zip(incoming) {
                    changed |= previous.join(&incoming);
                }
                changed
            },
        );
        let mut reads = BTreeMap::new();
        for block in &function.blocks {
            let mut state = entries[block.id.index()].clone();
            for id in &block.instructions {
                let instruction = function.instruction(*id).expect("verified instruction");
                for initializer in callable_reads(module, function, instruction) {
                    reads.insert(
                        (instruction.id, initializer),
                        state[content_index(function, initializer)].clone(),
                    );
                }
                proof.apply(instruction, &mut state);
            }
        }
        reads
    }

    /// Ordinary non-callable Borrow operands retain their existing ownership contract.
    fn callable_reads(
        module: &Module,
        function: &Function,
        instruction: &Instruction,
    ) -> Vec<EntityId> {
        let (receiver, arguments) = match &instruction.operation {
            Operation::ContainerGenerateBorrowed { initializer, .. }
            | Operation::MapWithValue {
                action: initializer,
                ..
            } => {
                return vec![EntityId::Loan(*initializer)];
            }
            Operation::ClosureConstruct { captures, .. } => {
                return captures
                    .iter()
                    .filter_map(|capture| match capture {
                        ClosureCaptureOperand::Shared(loan) => Some(EntityId::Loan(*loan)),
                        ClosureCaptureOperand::Owned(_) => None,
                    })
                    .filter(|entity| {
                        let target = function
                            .entity(*entity)
                            .expect("verified entity")
                            .ty
                            .semantic_type();
                        module.callable_view_signature(target).is_some()
                    })
                    .collect();
            }
            Operation::DirectCall {
                receiver,
                arguments,
                ..
            } => (*receiver, arguments.as_slice()),
            Operation::BorrowCall { arguments, .. } | Operation::RangeCall { arguments, .. } => {
                (None, arguments.as_slice())
            }
            Operation::CallableInvoke {
                callable,
                arguments,
            } => (Some(EntityId::Value(*callable)), arguments.as_slice()),
            Operation::CallableInvokeView { view, arguments } => {
                (Some(EntityId::Loan(*view)), arguments.as_slice())
            }
            _ => return Vec::new(),
        };
        receiver
            .into_iter()
            .chain(arguments.iter().copied())
            .filter(|entity| matches!(entity, EntityId::Loan(_)) || Some(*entity) == receiver)
            .filter(|entity| {
                let target = function
                    .entity(*entity)
                    .expect("verified entity")
                    .ty
                    .semantic_type();
                match entity {
                    EntityId::Loan(_) => {
                        module.callable_signature(target).is_some()
                            || module.callable_view_signature(target).is_some()
                    }
                    EntityId::Value(_) => module.concrete_closure(target).is_some(),
                    EntityId::Place(_) => false,
                }
            })
            .collect()
    }

    fn direct_root(function: &Function, entity: EntityId) -> bool {
        matches!(entity, EntityId::Place(_))
            && matches!(function.entity(entity).expect("verified entity").definition,
                Definition::InstructionResult { instruction, .. }
                    if matches!(function.instruction(instruction).expect("verified instruction").operation,
                        Operation::RootPlace { .. }))
    }

    impl<'a> Proof<'a> {
        fn new(
            module: &Module,
            function: &'a Function,
            types: &TypeCaptures,
            aliases: &AliasRoots,
            trusted_view_fields: Option<&'a BTreeSet<usize>>,
        ) -> Self {
            let entities = all_entities(function);
            let view_loans = entities
                .iter()
                .map(|entity| {
                    matches!(entity, EntityId::Loan(_))
                        && module
                            .callable_view_signature(
                                function
                                    .entity(*entity)
                                    .expect("verified entity")
                                    .ty
                                    .semantic_type(),
                            )
                            .is_some()
                })
                .collect::<Vec<_>>();
            let borrowed = entities
                .iter()
                .zip(&view_loans)
                .map(|(entity, view)| {
                    *view
                        || types.contains(
                            function
                                .entity(*entity)
                                .expect("verified entity")
                                .ty
                                .semantic_type(),
                        )
                })
                .collect::<Vec<_>>();
            let defaults = entities.iter().zip(&borrowed).map(|(entity, borrowed)| {
                let data = function.entity(*entity).expect("verified entity");
                let external_borrow = matches!(data.ty, EntityType::Loan { .. })
                    && matches!(data.definition, Definition::BlockParameter { block, .. } if Some(block) == function.entry_block());
                // Borrow parameters rely on the caller's active loan, never on invented caller-local IDs.
                CallableContent { known: external_borrow || !borrowed, loans: BTreeSet::new() }
            }).collect();
            let mut affected = BTreeMap::new();
            for instruction in &function.instructions {
                let Operation::Mutate { place, .. } = instruction.operation else {
                    continue;
                };
                let written = EntityId::Place(place);
                affected.entry(written).or_insert_with(|| {
                    entities
                        .iter()
                        .enumerate()
                        .filter_map(|(index, &entity)| {
                            // A store does not modify an SSA value or a different allocation.
                            if !borrowed[index]
                                || matches!(entity, EntityId::Value(_))
                                || (entity != written
                                    && direct_root(function, written)
                                    && direct_root(function, entity))
                                || !aliases.overlap(written, entity)
                            {
                                return None;
                            }
                            Some(index)
                        })
                        .collect()
                });
            }
            Self {
                function,
                defaults,
                borrowed,
                affected,
                view_loans,
                trusted_view_fields,
            }
        }

        fn follows_entry_capture(&self, source: LoanId) -> bool {
            let Some(Definition::InstructionResult { instruction, .. }) = self
                .function
                .entity(EntityId::Loan(source))
                .map(|entity| entity.definition)
            else {
                return false;
            };
            let Operation::SharedFieldLoan { base, field } = self
                .function
                .instruction(instruction)
                .expect("verified instruction")
                .operation
            else {
                return false;
            };
            self.trusted_view_fields
                .is_some_and(|fields| fields.contains(&field))
                && self.function.blocks[0].parameters.first() == Some(&EntityId::Loan(base))
        }

        fn apply(&self, instruction: &Instruction, state: &mut State) {
            let content = |entity| state[content_index(self.function, entity)].clone();
            let value = |value| content(EntityId::Value(value));
            let result = match &instruction.operation {
                Operation::ClosureConstruct { captures, .. } => {
                    let mut result = CallableContent::empty();
                    for capture in captures {
                        match capture {
                            ClosureCaptureOperand::Shared(loan) => {
                                result.loans.insert(*loan);
                                result.join(&content(EntityId::Loan(*loan)));
                            }
                            ClosureCaptureOperand::Owned(owner) => {
                                result.join(&value(*owner));
                            }
                        }
                    }
                    Some(result)
                }
                Operation::FunctionAddress { .. } | Operation::NullableNull { .. } => {
                    Some(CallableContent::empty())
                }
                Operation::AggregateConstruct { fields, .. }
                | Operation::ContainerConstruct {
                    elements: fields, ..
                } => {
                    let mut result = CallableContent::empty();
                    for field in fields {
                        result.join(&value(*field));
                    }
                    Some(result)
                }
                Operation::TaggedConstruct { payload, .. }
                | Operation::HeapAllocate { payload, .. }
                | Operation::SharedAllocate { payload, .. } => Some(value(*payload)),
                Operation::NullableWrap { owner, .. } | Operation::NullableTake { owner, .. } => {
                    Some(value(*owner))
                }
                Operation::SharedRetain { owner } => Some(content(*owner)),
                Operation::RootPlace { owner } => Some(value(*owner)),
                Operation::BorrowBegin { place, .. } => Some(content(EntityId::Place(*place))),
                Operation::SharedReborrow { source } | Operation::CallableViewOf { source } => {
                    Some(content(EntityId::Loan(*source)))
                }
                // Following an address cannot recover captured LoanIds from a capture-free slot.
                // Capturing targets keep the unknown default; capture-free targets keep exact emptiness.
                Operation::SharedReferenceFollow { source }
                    if self.view_loans[content_index(self.function, instruction.results[0])]
                        && self.follows_entry_capture(*source) =>
                {
                    Some(CallableContent::empty())
                }
                Operation::SharedReferenceFollow { .. } => None,
                Operation::RootPlaceTake { place, .. } => Some(content(EntityId::Place(*place))),
                Operation::Read { source } => Some(content(match source {
                    PlaceAccess::Place(place) => EntityId::Place(*place),
                    PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
                })),
                Operation::Copy { source } => Some(value(*source)),
                Operation::RootReplace {
                    loan, replacement, ..
                } => {
                    let replacement = value(*replacement);
                    let old = content(EntityId::Loan(*loan));
                    for (result, contents) in instruction.results.iter().zip([replacement, old]) {
                        state[content_index(self.function, *result)] = contents;
                    }
                    return;
                }
                Operation::RootSwap { loans, .. } => {
                    let left = content(EntityId::Loan(loans[0]));
                    let right = content(EntityId::Loan(loans[1]));
                    for (result, contents) in instruction.results.iter().zip([right, left]) {
                        state[content_index(self.function, *result)] = contents;
                    }
                    return;
                }
                // These projections cannot identify a field's capture IDs. Capture-free target types
                // still use the exact empty default; borrowed callable contents remain explicitly unknown.
                Operation::FieldPlace { .. }
                | Operation::SharedFieldLoan { .. }
                | Operation::SharedHeapFieldLoan { .. }
                | Operation::HeapPayloadPlace { .. }
                | Operation::SharedPayloadPlace { .. }
                | Operation::ContainerElementPlace { .. }
                | Operation::RangeElementPlace { .. }
                | Operation::TaggedPayloadPlace { .. }
                | Operation::AggregateProject { .. }
                | Operation::AggregateExplode { .. }
                | Operation::AggregateCopyExplode { .. } => None,
                _ => None,
            };
            if let Operation::Mutate {
                place,
                value: replacement,
            } = instruction.operation
            {
                let replacement = value(replacement);
                for index in &self.affected[&EntityId::Place(place)] {
                    state[*index].join(&replacement);
                }
                // This operand denotes the complete overwritten value even for a projected place.
                state[content_index(self.function, EntityId::Place(place))] = replacement;
            }
            for entity in &instruction.results {
                let index = content_index(self.function, *entity);
                state[index] = if !self.borrowed[index] {
                    CallableContent::empty()
                } else {
                    result
                        .as_ref()
                        .map_or_else(|| self.defaults[index].clone(), |result| result.clone())
                };
            }
        }

        fn apply_terminator(&self, terminator: &TerminatorKind, state: &mut State) {
            if let TerminatorKind::NullableBranch { owner, view, .. } = terminator {
                state[content_index(self.function, EntityId::Loan(*view))] =
                    state[content_index(self.function, EntityId::Value(*owner))].clone();
            }
        }
    }
}
