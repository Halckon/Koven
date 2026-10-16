//! Qualification for one owned List<Int> provider transporting an outer while view.
use super::super::super::{
    Expression, ExpressionCategory, nullable_proof::NullableProof, resolve_concrete_type, span_key,
};
use super::*;
use crate::ssa::model::{Definition, Instruction, ScalarConstant, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{
        IterationSource, OwnershipBindingKind, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationOwnershipPlan,
    },
    type_checking::{
        Copyability, IntrinsicTypeConstructor, IterationProvider, ParameterMode,
        SequentialContainerKind, UnitTypeKind,
    },
};

/// Provenance only: every use reads the current pair and pending roles, never
/// reinstalls the entry owner or loan after a body path has retired it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) struct BorrowedWhileProof {
    root: UnitSymbolId,
    proof: NullableProof,
    source_root: UnitSymbolId,
    element_root: UnitSymbolId,
    statement: UnitStatementId,
    container: SsaTypeId,
    int_type: SsaTypeId,
}

impl BorrowedWhileProof {
    pub(super) fn qualify(
        lowerer: &UnitExpressionLowerer<'_>,
        plan: &UnitIterationOwnershipPlan,
        container: SsaTypeId,
        int_type: SsaTypeId,
        span: Span,
    ) -> Result<Option<Self>, LoweringError> {
        let Some((&root, &proof)) = lowerer.nullable_proofs.first_key_value() else {
            return Ok(None);
        };
        let descriptor = plan.descriptor();
        let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind()
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let UnitSequentialIterationBinding::Name(element_root) = descriptor.binding() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let source_root = place.root();
        if descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
            || descriptor.delivery() != ParameterMode::Borrow
            || plan.source_access() != UnitIterationSourceAccess::Owned
            || !place.is_root()
            || source_root == root
            || *element_root == source_root
            || *element_root == root
            || descriptor.statement().source_unit() != lowerer.source_unit
            || descriptor.source().source_unit() != lowerer.source_unit
            || source_root.source_unit() != lowerer.source_unit
            || element_root.source_unit() != lowerer.source_unit
            || lowerer.borrow_bindings.contains_key(&source_root)
            || lowerer.borrow_bindings.contains_key(&root)
            || lowerer.borrow_bindings.contains_key(element_root)
            || lowerer.bindings.contains_key(element_root)
            || !matches!(plan.bindings(), [binding]
                if binding.symbol() == *element_root && binding.kind() == OwnershipBindingKind::Shared)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete = |ty| {
            resolve_concrete_type(
                lowerer.typed,
                ty,
                lowerer.substitutions,
                lowerer.static_self,
                span,
            )
        };
        let source_type = concrete(descriptor.source_type())?;
        let element_type = concrete(descriptor.element_type())?;
        let storage = lowerer
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let declared = concrete(storage)?;
        let Some(UnitTypeKind::Nullable(inner)) = lowerer.typed.types().get(declared) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !matches!(lowerer.typed.types().get(source_type),
            Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::List, arguments })
                if arguments.as_slice() == [element_type])
            || lowerer.typed.types().get(element_type)
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            || lowerer
                .typed
                .symbol_type(source_root)
                .map(concrete)
                .transpose()?
                != Some(source_type)
            || lowerer
                .typed
                .symbol_type(*element_root)
                .map(concrete)
                .transpose()?
                != Some(element_type)
            || lowerer.type_ids.get(&source_type) != Some(&container)
            || lowerer.type_ids.get(&element_type) != Some(&int_type)
            || lowerer.type_ids.get(inner) != Some(&proof.inner)
            || lowerer.typed.copyability(source_type) != Copyability::MoveOnly
            || lowerer.typed.copyability(declared) != Copyability::MoveOnly
            || lowerer.typed.copyability(*inner) != Copyability::MoveOnly
            || !matches!(
                lowerer.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || lowerer.ssa_types.get(container.index())
                != Some(&SsaTypeKind::SequentialContainer {
                    kind: crate::ssa::model::SequentialContainerKind::List,
                    element: int_type,
                })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Qualify every transparent source layer against the published root.
        let mut operand = descriptor.source();
        loop {
            if lowerer.typed.expression_type(operand) != Some(descriptor.source_type())
                || lowerer.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                || lowerer.typed.non_null_use(operand).is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let node = lowerer
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(lowerer.source_unit, *expression);
                }
                Expression::Name
                    if lowerer.references.get(&span_key(node.span())) == Some(&source_root) =>
                {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let qualified = Self {
            root,
            proof,
            source_root,
            element_root: *element_root,
            statement: descriptor.statement(),
            container,
            int_type,
        };
        qualified.validate_common(lowerer, &lowerer.loop_state(), span)?;
        qualified.source_owner(lowerer, &lowerer.loop_state(), span)?;
        lowerer.ensure_no_while_proof_carriers(span)?;
        // This slice consumes the real source cleanup once. Other exit kinds,
        // hidden owners and extra child cleanup remain outside the qualification.
        let mut exhausted = false;
        for exit in plan.exits() {
            let valid = match exit.kind() {
                UnitIterationExitKind::Continue(_) | UnitIterationExitKind::Fallthrough => {
                    matches!(exit.actions(), [Action::EndBinding { statement, symbol }, Action::EndElement(end)]
                        if *statement == qualified.statement && *end == qualified.statement
                            && *symbol == qualified.element_root)
                }
                UnitIterationExitKind::Exhaustion if !exhausted => {
                    exhausted = true;
                    exit.point() == UnitDropPoint::LoopExit(qualified.statement)
                        && matches!(exit.actions(), [Action::FinishProvider(finish), Action::EndSource(end), Action::Drop(drop)]
                            if *finish == qualified.statement && *end == qualified.statement
                                && drop.point() == exit.point()
                                && drop.target() == UnitDropTarget::Named(source_root))
                }
                _ => false,
            };
            if !valid {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        if !exhausted {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(qualified))
    }

    fn validate_common(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        lowerer.validate_readonly_while_exit(self.root, self.proof, state, span)?;
        if self.proof.while_scope().is_none_or(|scope| {
            !lowerer.loops.iter().any(|context| context.proof_scope == Some(scope))
        }) || lowerer.function.borrow_return.is_some()
            || lowerer.function.carrier_return.is_some()
            || !state.pending_call_frames.is_empty()
            || !state.temporaries.is_empty()
            || !state.capture_loans.is_empty()
            || !state.result_source_loans.is_empty()
            || !state.closure_bindings.is_empty()
            || state.receiver.is_some()
            || !lowerer.temporary_range_slots.is_empty()
            || !lowerer.short_range_ends.is_empty()
            || lowerer.loops.iter().any(|context| {
                context.iteration.as_ref().is_some_and(|iteration| {
                    iteration.statement != self.statement
                        || context.proof_scope.is_some()
                        || iteration.retained_start != 0
                        || !matches!(iteration.state,
                            IterationState::Borrowed { start: 0, element: 3, int_type, .. }
                                if int_type == self.int_type)
                        || iteration.inherited_borrowed != Some(self)
                })
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn source_owner(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let Some(LoweredValue::Value(owner)) = state.bindings.get(&self.source_root) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if state
            .nullable_proofs
            .get(&self.root)
            .is_none_or(|proof| proof.owner == *owner)
            || lowerer
                .function
                .entity(EntityId::Value(*owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(self.container))
            || state.borrow_bindings.contains_key(&self.source_root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(*owner)
    }

    pub(super) fn validate_source(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        source: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = lowerer.loop_state();
        self.validate_common(lowerer, &state, span)?;
        lowerer.ensure_no_while_proof_carriers(span)?;
        let owner = self.source_owner(lowerer, &state, span)?;
        let Some(Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        }) = definition(lowerer, EntityId::Loan(source)).map(|instruction| &instruction.operation)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !matches!(definition(lowerer, EntityId::Place(*place)).map(|instruction| &instruction.operation),
            Some(Operation::RootPlace { owner: actual }) if *actual == owner)
            || lowerer
                .function
                .entity(EntityId::Loan(source))
                .map(|data| data.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: self.container,
                })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_snapshot(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        source: LoanId,
        snapshot: &provider::ProviderSnapshot,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_provider(lowerer, &lowerer.loop_state(), span)?;
        if lowerer.pending_operands
            != [
                EntityId::Loan(source),
                EntityId::Value(snapshot.length()),
                EntityId::Value(snapshot.cursor()),
            ]
            || !matches!(definition(lowerer, EntityId::Value(snapshot.length())).map(|instruction| &instruction.operation),
                Some(Operation::ContainerLength { owner: EntityId::Loan(actual) }) if *actual == source)
            || !matches!(
                definition(lowerer, EntityId::Value(snapshot.cursor()))
                    .map(|instruction| &instruction.operation),
                Some(Operation::Constant(ScalarConstant::Integer(0)))
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_header(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        header: &provider::ProviderHeader,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_provider(lowerer, &lowerer.loop_state(), span)?;
        if lowerer.block != header.block
            || lowerer.pending_operands
                != [
                    EntityId::Loan(header.source),
                    EntityId::Value(header.length),
                    EntityId::Value(header.cursor),
                ]
            || !matches!(
                definition(lowerer, EntityId::Value(header.step))
                    .map(|instruction| &instruction.operation),
                Some(Operation::Constant(ScalarConstant::Integer(1)))
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn validate_active(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        element: Option<bool>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_common(lowerer, state, span)?;
        self.source_owner(lowerer, state, span)?;
        let expected = if element.is_some() { 4 } else { 3 };
        if state.pending_operands.len() != expected {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let proof = state
            .nullable_proofs
            .get(&self.root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for (index, entity) in state.pending_operands.iter().copied().enumerate() {
            let ty = match index {
                0 => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: self.container,
                },
                1 | 2 => EntityType::Value(self.int_type),
                3 => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: self.int_type,
                },
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            };
            if lowerer.function.entity(entity).map(|data| data.ty) != Some(ty)
                || entity == EntityId::Loan(proof.loan)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let element_binding = state
            .borrow_bindings
            .get(&self.element_root)
            .copied()
            .map(EntityId::Loan);
        if match element {
            Some(true) => element_binding != state.pending_operands.get(3).copied(),
            None | Some(false) => element_binding.is_some(),
        } || state.borrow_bindings.iter().any(|(root, &loan)| {
            if *root == self.element_root && element == Some(true) {
                return false;
            }
            !lowerer.entry_boolean_roots.contains(root)
                || !matches!(lowerer.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target })
                        if matches!(lowerer.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn validate_provider(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_active(lowerer, state, None, span)
    }

    pub(super) fn validate_element(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        element: &provider::GuardedElement,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_active(lowerer, &lowerer.loop_state(), Some(true), span)?;
        let Some(Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        }) = definition(lowerer, EntityId::Loan(element.loan()))
            .map(|instruction| &instruction.operation)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if lowerer.block != element.body()
            || lowerer.pending_operands[0] != EntityId::Loan(element.source())
            || lowerer.pending_operands[2] != EntityId::Value(element.cursor())
            || lowerer.pending_operands[3] != EntityId::Loan(element.loan())
            || !matches!(definition(lowerer, EntityId::Place(*place)).map(|instruction| &instruction.operation),
                Some(Operation::ContainerElementPlace { owner: EntityId::Loan(source), index })
                    if *source == element.source() && *index == element.cursor())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_element_cleanup(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = lowerer.loop_state();
        self.validate_active(lowerer, &state, Some(false), span)?;
        // The slot is the current CFG-rebound element, not its original birth ID.
        let EntityId::Loan(element) = state.pending_operands[3] else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let EntityId::Loan(source) = state.pending_operands[0] else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let proof = state.nullable_proofs[&self.root];
        let owner = self.source_owner(lowerer, &state, span)?;
        let operations = block_operations(lowerer, state.block);
        if operations.iter().filter(|operation| matches!(operation,
            Operation::BorrowEnd { loan } if *loan == element)).count() != 1
            || operations.iter().any(|operation| matches!(operation,
                Operation::BorrowEnd { loan } if *loan == source || *loan == proof.loan)
                || matches!(operation, Operation::Drop { owner: dropped } if *dropped == owner || *dropped == proof.owner))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn exhaustion_source(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(ValueId, LoanId), LoweringError> {
        let state = lowerer.loop_state();
        self.validate_provider(lowerer, &state, span)?;
        let EntityId::Loan(source) = state.pending_operands[0] else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        Ok((self.source_owner(lowerer, &state, span)?, source))
    }

    pub(super) fn validate_exhaustion(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        owner: ValueId,
        source: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_exit(lowerer, &lowerer.loop_state(), span)?;
        let operations = block_operations(lowerer, lowerer.block);
        let ends = operations
            .iter()
            .enumerate()
            .filter_map(|(index, operation)| {
                matches!(operation, Operation::BorrowEnd { loan } if *loan == source)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let drops = operations
            .iter()
            .enumerate()
            .filter_map(|(index, operation)| {
                matches!(operation, Operation::Drop { owner: dropped } if *dropped == owner)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        if !matches!((ends.as_slice(), drops.as_slice()), ([end], [drop]) if end < drop) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_exit(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_common(lowerer, state, span)?;
        if !state.pending_operands.is_empty()
            || state.bindings.contains_key(&self.source_root)
            || state.bindings.contains_key(&self.element_root)
            || state.borrow_bindings.iter().any(|(root, &loan)| {
                !lowerer.entry_boolean_roots.contains(root)
                    || !matches!(lowerer.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target })
                            if matches!(lowerer.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }
}

fn definition<'a>(
    lowerer: &'a UnitExpressionLowerer<'_>,
    entity: EntityId,
) -> Option<&'a Instruction> {
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = lowerer.function.entity(entity)?.definition
    else {
        return None;
    };
    lowerer
        .function
        .instruction(instruction)
        .filter(|instruction| instruction.results.first() == Some(&entity))
}

fn block_operations<'a>(
    lowerer: &'a UnitExpressionLowerer<'_>,
    block: BlockId,
) -> Vec<&'a Operation> {
    lowerer
        .function
        .block(block)
        .into_iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|id| {
            lowerer
                .function
                .instruction(*id)
                .map(|instruction| &instruction.operation)
        })
        .collect()
}
