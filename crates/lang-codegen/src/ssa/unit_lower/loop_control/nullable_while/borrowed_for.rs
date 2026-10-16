//! One readonly nullable while inside a live owned List<Int> provider.
#[cfg(test)]
mod guard_tests;

use super::super::iteration::IterationState;
use super::*;
use crate::ssa::model::{
    ComparisonOperator, Definition, Edge, Instruction, LoanKind, Operation, ScalarConstant, ValueId,
};
use lang_frontend::{
    ownership_checking::{
        IterationSource, OwnershipBindingKind, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::Statement,
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, IterationProvider, ParameterMode,
        SequentialContainerKind, UnitSequentialIterationBinding,
    },
};

/// Constructed only at the actual outer element birth. This is relation identity,
/// not a saved lowering state: every use reads and checks all six current columns.
/// The boxed context copy adds only the actual inner header and proof birth.
#[derive(Clone)]
pub(in super::super) struct BorrowedForWhile {
    outer: UnitStatementId,
    outer_header: BlockId,
    scope: UnitStatementId,
    jump: UnitExpressionId,
    items: UnitSymbolId,
    current: UnitSymbolId,
    item: UnitSymbolId,
    container: SsaTypeId,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    int_type: SsaTypeId,
    step: ValueId,
    anchor_block: BlockId,
    anchor: [EntityId; 6],
    header: Option<BlockId>,
    proof: Option<NullableProof>,
}

impl BorrowedForWhile {
    pub(super) fn qualify(
        lowerer: &UnitExpressionLowerer<'_>,
        entry: &DirectNullableWhileEntry,
        span: Span,
    ) -> Result<Option<Self>, LoweringError> {
        if !lowerer.has_iteration_context() {
            return Ok(None);
        }
        let bad = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
        let [outer] = lowerer.loops.as_slice() else {
            return Err(bad());
        };
        let Some(iteration) = &outer.iteration else {
            return Err(bad());
        };
        if outer.borrowed_while_body.is_some() {
            return Err(bad());
        }
        let IterationState::Borrowed {
            start: 0,
            element: 3,
            step,
            int_type,
        } = iteration.state
        else {
            return Err(bad());
        };
        let plan = lowerer
            .owned
            .iteration(iteration.statement)
            .ok_or_else(bad)?;
        let descriptor = plan.descriptor();
        let outer_node = lowerer
            .parsed
            .ast()
            .statements()
            .get(iteration.statement.statement())
            .map_err(|_| bad())?;
        let Statement::For { body, .. } = outer_node.payload() else {
            return Err(bad());
        };
        let outer_body = lowerer
            .parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| bad())?;
        if !matches!(outer_body.payload(), Statement::Block { elements }
            if elements.first() == Some(&entry.scope().statement()))
        {
            return Err(bad());
        }
        let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind()
        else {
            return Err(bad());
        };
        let UnitSequentialIterationBinding::Name(item) = descriptor.binding() else {
            return Err(bad());
        };
        let items = place.root();
        if descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
            || descriptor.delivery() != ParameterMode::Borrow
            || plan.source_access() != UnitIterationSourceAccess::Owned
            || !place.is_root()
            || items == entry.root()
            || *item == items
            || *item == entry.root()
            || descriptor.statement().source_unit() != lowerer.source_unit
            || descriptor.source().source_unit() != lowerer.source_unit
            || items.source_unit() != lowerer.source_unit
            || item.source_unit() != lowerer.source_unit
            || lowerer.typed.sequential_iteration(iteration.statement) != Some(descriptor)
            || !matches!(plan.bindings(), [binding]
                if binding.symbol() == *item && binding.kind() == OwnershipBindingKind::Shared)
        {
            return Err(bad());
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
        let container = *lowerer.type_ids.get(&source_type).ok_or_else(bad)?;
        if !matches!(lowerer.typed.types().get(source_type),
            Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::List, arguments })
                if arguments.as_slice() == [element_type])
            || lowerer.typed.types().get(element_type)
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            || lowerer.type_ids.get(&element_type) != Some(&int_type)
            || lowerer.typed.symbol_type(items).map(concrete).transpose()? != Some(source_type)
            || lowerer.typed.symbol_type(*item).map(concrete).transpose()? != Some(element_type)
            || lowerer.typed.copyability(source_type) != Copyability::MoveOnly
            || lowerer.ssa_types.get(container.index())
                != Some(&SsaTypeKind::SequentialContainer {
                    kind: crate::ssa::model::SequentialContainerKind::List,
                    element: int_type,
                })
        {
            return Err(bad());
        }
        let mut operand = descriptor.source();
        loop {
            let node = lowerer
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| bad())?;
            if lowerer.typed.expression_type(operand) != Some(descriptor.source_type())
                || lowerer.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                || lowerer.typed.non_null_use(operand).is_some()
            {
                return Err(bad());
            }
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(lowerer.source_unit, *expression)
                }
                Expression::Name
                    if lowerer.references.get(&span_key(node.span())) == Some(&items) =>
                {
                    break;
                }
                _ => return Err(bad()),
            }
        }
        let node = lowerer
            .parsed
            .ast()
            .statements()
            .get(entry.scope().statement())
            .map_err(|_| bad())?;
        let Statement::While { body, .. } = node.payload() else {
            return Err(bad());
        };
        let node = lowerer
            .parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| bad())?;
        let (Statement::Block { elements } | Statement::ControlBody { elements }) = node.payload()
        else {
            return Err(bad());
        };
        let last = elements.last().ok_or_else(bad)?;
        let node = lowerer
            .parsed
            .ast()
            .statements()
            .get(*last)
            .map_err(|_| bad())?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(bad());
        };
        if !matches!(
            lowerer
                .parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| bad())?
                .payload(),
            Expression::Break { .. }
        ) {
            return Err(bad());
        }
        let jump = UnitExpressionId::new(lowerer.source_unit, *expression);
        let mut fallthrough = false;
        let mut exhaustion = false;
        for exit in plan.exits() {
            let valid = match exit.kind() {
                UnitIterationExitKind::Fallthrough if !fallthrough => {
                    fallthrough = true;
                    matches!(exit.actions(), [Action::EndBinding { statement, symbol }, Action::EndElement(end)]
                        if *statement == iteration.statement && *end == iteration.statement && *symbol == *item)
                }
                UnitIterationExitKind::Exhaustion if !exhaustion => {
                    exhaustion = true;
                    exit.point() == UnitDropPoint::LoopExit(iteration.statement)
                        && matches!(exit.actions(), [Action::FinishProvider(finish), Action::EndSource(end), Action::Drop(source), Action::Drop(current)]
                            if *finish == iteration.statement && *end == iteration.statement
                                && source.point() == exit.point() && current.point() == exit.point()
                                && source.target() == UnitDropTarget::Named(items)
                                && current.target() == UnitDropTarget::Named(entry.root()))
                }
                _ => false,
            };
            if !valid {
                return Err(bad());
            }
        }
        let local_points = [
            UnitDropPoint::ControlTransfer(jump),
            UnitDropPoint::LoopExit(entry.scope()),
        ];
        if !fallthrough
            || !exhaustion
            || local_points
                .iter()
                .any(|point| lowerer.owned.iteration_cleanup_at(*point).is_some())
            || lowerer.owned.drops().iter().any(|drop| {
                local_points.contains(&drop.point())
                    || matches!(drop.target(), UnitDropTarget::Named(root)
                            if (root == items || root == entry.root())
                                && drop.point() != UnitDropPoint::LoopExit(iteration.statement))
            })
            || lowerer
                .owned
                .conditional_receiver_drops()
                .iter()
                .any(|drop| local_points.contains(&drop.point()))
        {
            return Err(bad());
        }
        let state = lowerer.loop_state();
        let anchor = roles(&state, items, entry.root(), *item, true, span)?;
        let token = Self {
            outer: iteration.statement,
            outer_header: outer.header,
            scope: entry.scope(),
            jump,
            items,
            current: entry.root(),
            item: *item,
            container,
            nullable: entry.nullable(),
            inner: entry.inner(),
            int_type,
            step,
            anchor_block: state.block,
            anchor,
            header: None,
            proof: None,
        };
        token.validate_exit(lowerer, &state, span)?;
        token.validate_birth(lowerer, step, span)?;
        Ok(Some(token))
    }

    pub(super) fn bind_header(&mut self, header: BlockId) {
        self.header = Some(header);
    }

    pub(super) fn bind_proof(
        &mut self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let proof = *lowerer
            .nullable_proofs
            .get(&self.current)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(TerminatorKind::NullableBranch {
            view,
            when_non_null,
            ..
        }) = self
            .header
            .and_then(|header| lowerer.function.block(header))
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if proof.while_scope() != Some(self.scope)
            || proof.inner != self.inner
            || proof.loan != *view
            || when_non_null.target != lowerer.block
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.proof = Some(proof);
        self.validate(lowerer, &lowerer.loop_state(), true, false, span)
    }

    pub(super) fn validate_exit(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate(lowerer, state, false, false, span)
    }

    pub(super) fn install_outer(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let context = lowerer
            .loops
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if context.header != self.outer_header
            || context.borrowed_while_body.is_some()
            || context
                .iteration
                .as_ref()
                .is_none_or(|iteration| iteration.statement != self.outer)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        // The outer copy stores only this already qualified relation. No current
        // exit operands or ended element IDs are saved for later reconstruction.
        context.borrowed_while_body = Some(Box::new(self.clone()));
        Ok(())
    }

    pub(in super::super) fn validate_element_end(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = lowerer.loop_state();
        let current = roles(&state, self.items, self.current, self.item, false, span)?;
        let ends = lowerer
            .function
            .block(state.block)
            .into_iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|id| lowerer.function.instruction(*id))
            .filter(|instruction| {
                matches!(&instruction.operation, Operation::BorrowEnd { loan }
                if EntityId::Loan(*loan) == current[5])
            })
            .count();
        if ends != 1 || lowerer.loops.len() != 1 {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate(lowerer, &state, false, true, span)
    }

    pub(in super::super) fn validate_backedge(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        context: &LoopContext,
        state: &BranchExit,
        edge: &Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let [source, length, cursor] = state.pending_operands.as_slice() else {
            return Err(bad());
        };
        let Some(LoweredValue::Value(items)) = state.bindings.get(&self.items) else {
            return Err(bad());
        };
        let Some(LoweredValue::Value(current)) = state.bindings.get(&self.current) else {
            return Err(bad());
        };
        if context.header != self.outer_header || edge.target != self.outer_header
            || context.iteration.as_ref().is_none_or(|iteration| iteration.statement != self.outer
                || !matches!(iteration.state, IterationState::Borrowed { start: 0, element: 3, step, int_type }
                    if step == self.step && int_type == self.int_type))
            || context.continues.len() != 1 || !context.breaks.is_empty()
            || !state.nullable_proofs.is_empty() || state.bindings.contains_key(&self.item)
            || !state.pending_call_frames.is_empty() || !state.temporaries.is_empty()
            || !state.capture_loans.is_empty() || !state.result_source_loans.is_empty()
            || !state.closure_bindings.is_empty() || state.receiver.is_some() || state.consumed_receiver.is_some()
            || state.borrow_bindings.iter().any(|(root, &loan)| !lowerer.entry_boolean_roots.contains(root)
                || !matches!(lowerer.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target })
                        if matches!(lowerer.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean))))
            || edge.arguments.iter().any(|argument| matches!(argument, EntityId::Loan(loan)
                if argument != source && !state.borrow_bindings.values().any(|value| value == loan)))
        { return Err(bad()); }
        let exit_roles = [
            EntityId::Value(*items),
            EntityId::Value(*current),
            *source,
            *length,
            *cursor,
        ];
        let instruction = definition(lowerer, *cursor).ok_or_else(bad)?;
        let Operation::Binary {
            operator: crate::ssa::model::BinaryOperator::Add,
            left,
            right,
        } = &instruction.operation
        else {
            return Err(bad());
        };
        if instruction.block != state.block
            || *right != self.step
            || lowerer.function.entity(*cursor).map(|data| data.ty)
                != Some(EntityType::Value(self.int_type))
            || !matches!(
                definition(lowerer, EntityId::Value(*right)).map(|i| &i.operation),
                Some(Operation::Constant(ScalarConstant::Integer(1)))
            )
        {
            return Err(bad());
        }
        let mut before_advance = exit_roles;
        before_advance[4] = EntityId::Value(*left);
        // Recheck every predecessor now, after body lowering and the actual cursor
        // advance. The ended element is deliberately absent from this header tuple.
        self.transport(
            lowerer,
            state.block,
            &before_advance,
            None,
            None,
            &mut BTreeSet::new(),
            span,
        )?;
        let Some(TerminatorKind::Conditional { when_true, .. }) = lowerer
            .function
            .block(self.outer_header)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(bad());
        };
        for (anchor, actual) in self.anchor[..5].iter().zip(exit_roles) {
            let header = incoming(lowerer, *anchor, when_true, span)?;
            let Definition::BlockParameter { block, index } =
                lowerer.function.entity(header).ok_or_else(bad)?.definition
            else {
                return Err(bad());
            };
            if block != self.outer_header || edge.arguments.get(index) != Some(&actual) {
                return Err(bad());
            }
        }
        Ok(())
    }

    fn validate(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        active: bool,
        ending: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
        let Some(outer) = lowerer.loops.first() else {
            return Err(bad());
        };
        let Some(iteration) = &outer.iteration else {
            return Err(bad());
        };
        if outer.header != self.outer_header
            || outer.proof_scope.is_some()
            || outer.borrowed_for_while.is_some()
            || outer.borrowed_while_body.as_ref().is_some_and(|token| {
                token.outer != self.outer
                    || token.scope != self.scope
                    || token.anchor != self.anchor
            })
            || iteration.statement != self.outer
            || iteration.retained_start != 0
            || outer.entry_pending_count != 3
            || !iteration.outer_temporaries.is_empty()
            || iteration.inherited_borrowed.is_some()
            || !matches!(iteration.state, IterationState::Borrowed { start: 0, element: 3, int_type, step }
                if int_type == self.int_type && step == self.step)
            || lowerer.loops.len() > 2
            || lowerer.loops.get(1).is_some_and(|context| {
                context.proof_scope != Some(self.scope)
                    || Some(context.header) != self.header
                    || context.iteration.is_some()
                    || context.entry_pending_count != 4
                    || context
                        .borrowed_for_while
                        .as_ref()
                        .is_none_or(|token| token.jump != self.jump || token.outer != self.outer)
            })
            || lowerer.function.borrow_return.is_some()
            || lowerer.function.carrier_return.is_some()
            || !state.pending_call_frames.is_empty()
            || !state.temporaries.is_empty()
            || !state.capture_loans.is_empty()
            || !state.result_source_loans.is_empty()
            || !state.closure_bindings.is_empty()
            || state.receiver.is_some()
            || state.consumed_receiver.is_some()
            || !lowerer.temporary_range_slots.is_empty()
            || !lowerer.short_range_ends.is_empty()
            || state.borrow_bindings.contains_key(&self.items)
            || state.borrow_bindings.contains_key(&self.current)
            || state.bindings.contains_key(&self.item)
        {
            return Err(bad());
        }
        let current = roles(state, self.items, self.current, self.item, !ending, span)?;
        let shared = |target| EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        let types = [
            EntityType::Value(self.container),
            EntityType::Value(self.nullable),
            shared(self.container),
            EntityType::Value(self.int_type),
            EntityType::Value(self.int_type),
            shared(self.int_type),
        ];
        if current.iter().zip(types).any(|(entity, ty)| lowerer.function.entity(*entity).map(|data| data.ty) != Some(ty))
            || current[0] == current[1] || current[2] == current[5]
            || state.borrow_bindings.iter().any(|(root, &loan)| *root != self.item
                && (!lowerer.entry_boolean_roots.contains(root)
                    || !matches!(lowerer.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target })
                            if matches!(lowerer.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))))
        { return Err(bad()); }
        let view = if active {
            let proof = state.nullable_proofs.get(&self.current).ok_or_else(bad)?;
            if state.nullable_proofs.len() != 1
                || proof.while_scope() != Some(self.scope)
                || proof.inner != self.inner
                || Some(proof.origin) != self.proof.map(|birth| birth.origin)
                || EntityId::Value(proof.owner) != current[1]
                || current.contains(&EntityId::Loan(proof.loan))
            {
                return Err(bad());
            }
            lowerer.validate_nullable_proofs(&state.bindings, &state.nullable_proofs, span)?;
            Some(EntityId::Loan(proof.loan))
        } else {
            if !state.nullable_proofs.is_empty() {
                return Err(bad());
            }
            None
        };
        self.transport(
            lowerer,
            state.block,
            &current,
            view,
            ending.then_some(state.block),
            &mut BTreeSet::new(),
            span,
        )
    }

    fn validate_birth(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        step: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        }) = definition(lowerer, self.anchor[5]).map(|i| &i.operation)
        else {
            return Err(bad());
        };
        if definition(lowerer, self.anchor[5]).is_none_or(|i| i.block != self.anchor_block)
            || !matches!(definition(lowerer, EntityId::Place(*place)).map(|i| &i.operation),
                Some(Operation::ContainerElementPlace { owner, index })
                    if *owner == self.anchor[2] && EntityId::Value(*index) == self.anchor[4])
            || !matches!(
                definition(lowerer, EntityId::Value(step)).map(|i| &i.operation),
                Some(Operation::Constant(ScalarConstant::Integer(1)))
            )
        {
            return Err(bad());
        }
        let Some(TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        }) = lowerer
            .function
            .block(self.outer_header)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(bad());
        };
        if when_true.target != self.anchor_block || when_false.target == self.anchor_block {
            return Err(bad());
        }
        let mut header = self.anchor;
        for role in &mut header[..5] {
            *role = incoming(lowerer, *role, when_true, span)?;
        }
        if !matches!(definition(lowerer, EntityId::Value(*condition)).map(|i| &i.operation),
            Some(Operation::Compare { operator: ComparisonOperator::LessThan, left, right })
                if EntityId::Value(*left) == header[4] && EntityId::Value(*right) == header[3])
        {
            return Err(bad());
        }
        let predecessors = lowerer
            .function
            .blocks
            .iter()
            .filter_map(|block| match &block.terminator.as_ref()?.kind {
                TerminatorKind::Branch(edge) if edge.target == self.outer_header => Some(edge),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [edge] = predecessors.as_slice() else {
            return Err(bad());
        };
        if lowerer
            .function
            .blocks
            .iter()
            .filter(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.kind.targets().contains(&self.outer_header))
            })
            .count()
            != 1
        {
            return Err(bad());
        }
        for role in &mut header[..5] {
            *role = incoming(lowerer, *role, edge, span)?;
        }
        let Some(Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        }) = definition(lowerer, header[2]).map(|i| &i.operation)
        else {
            return Err(bad());
        };
        if !matches!(definition(lowerer, EntityId::Place(*place)).map(|i| &i.operation),
            Some(Operation::RootPlace { owner }) if EntityId::Value(*owner) == header[0])
            || header[0] == header[1]
            || !matches!(definition(lowerer, header[3]).map(|i| &i.operation),
                Some(Operation::ContainerLength { owner }) if *owner == header[2])
            || !matches!(
                definition(lowerer, header[4]).map(|i| &i.operation),
                Some(Operation::Constant(ScalarConstant::Integer(0)))
            )
        {
            return Err(bad());
        }
        Ok(())
    }

    /// Walk only this fixed role tuple through actual predecessor arguments. Every
    /// incoming edge must preserve the relation; absent roles cannot use ambient IDs.
    #[allow(clippy::too_many_arguments)]
    fn transport(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        block: BlockId,
        roles: &[EntityId],
        view: Option<EntityId>,
        ending: Option<BlockId>,
        visiting: &mut BTreeSet<BlockId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let data = lowerer.function.block(block).ok_or_else(bad)?;
        if !visiting.insert(block) {
            return Err(bad());
        }
        for instruction in &data.instructions {
            let instruction = lowerer.function.instruction(*instruction).ok_or_else(bad)?;
            if matches!(&instruction.operation, Operation::BorrowEnd { loan }
                if (roles.contains(&EntityId::Loan(*loan))
                    && !(ending == Some(block) && roles.get(5) == Some(&EntityId::Loan(*loan))))
                    || view == Some(EntityId::Loan(*loan)))
                || matches!(&instruction.operation, Operation::Drop { owner } if roles.contains(&EntityId::Value(*owner)))
                || matches!(&instruction.operation, Operation::BorrowBegin { .. })
                    && instruction.results.as_slice() != [self.anchor[5]]
            {
                return Err(bad());
            }
        }
        if block == self.anchor_block {
            visiting.remove(&block);
            return if roles == &self.anchor[..roles.len()] && view.is_none() {
                Ok(())
            } else {
                Err(bad())
            };
        }
        let mut count = 0;
        for predecessor in &lowerer.function.blocks {
            let Some(term) = &predecessor.terminator else {
                continue;
            };
            let edges: Vec<&Edge> = match &term.kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                } => vec![when_null, when_non_null],
                _ => Vec::new(),
            };
            for edge in edges.into_iter().filter(|edge| edge.target == block) {
                count += 1;
                let mut before = roles.to_vec();
                for role in &mut before {
                    *role = incoming(lowerer, *role, edge, span)?;
                }
                let before_view = match (&term.kind, view) {
                    (
                        TerminatorKind::NullableBranch {
                            owner,
                            when_null,
                            when_non_null,
                            view: birth,
                        },
                        view,
                    ) => {
                        if Some(predecessor.id) != self.header
                            || before[1] != EntityId::Value(*owner)
                            || when_null.target == when_non_null.target
                            || when_null.arguments != when_non_null.arguments
                            || lowerer
                                .function
                                .block(when_non_null.target)
                                .is_none_or(|body| {
                                    body.parameters.last() != Some(&EntityId::Loan(*birth))
                                        || body.parameters.len()
                                            != when_non_null.arguments.len() + 1
                                })
                            || lowerer.function.block(when_null.target).is_none_or(|body| {
                                body.parameters.len() != when_null.arguments.len()
                            })
                            || view.is_some_and(|view| {
                                view != EntityId::Loan(*birth) || block != when_non_null.target
                            })
                        {
                            return Err(bad());
                        }
                        None
                    }
                    (_, Some(view)) => Some(incoming(lowerer, view, edge, span)?),
                    (_, None) => None,
                };
                self.transport(
                    lowerer,
                    predecessor.id,
                    &before,
                    before_view,
                    ending,
                    visiting,
                    span,
                )?;
            }
        }
        visiting.remove(&block);
        if count == 0 {
            return Err(bad());
        }
        Ok(())
    }
}

impl UnitExpressionLowerer<'_> {
    /// Only this exact AST break may retire the inner view while provider roles
    /// survive. Ordinary retirement and all other jumps retain their strict gate.
    pub(in super::super) fn retire_borrowed_for_while_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(context) = self.loops.last() else {
            return Ok(());
        };
        let Some(token) = &context.borrowed_for_while else {
            return Ok(());
        };
        if token.jump != UnitExpressionId::new(self.source_unit, expression)
            || context.proof_scope != Some(token.scope)
            || Some(context.header) != token.header
            || context.iteration.is_some()
            || self.loops.len() != 2
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        // The caller already consumed the original control-transfer facts. Refuse
        // any missing, changed or ended role before emitting our own BorrowEnd.
        token.validate(self, &self.loop_state(), true, false, span)?;
        let root = token.current;
        let proof = *self
            .nullable_proofs
            .get(&root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.function
            .append_instruction(
                self.block,
                Operation::BorrowEnd { loan: proof.loan },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.nullable_proofs.remove(&root);
        Ok(())
    }
}

fn roles(
    state: &BranchExit,
    items: UnitSymbolId,
    current: UnitSymbolId,
    item: UnitSymbolId,
    bound: bool,
    span: Span,
) -> Result<[EntityId; 6], LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let Some(LoweredValue::Value(items)) = state.bindings.get(&items) else {
        return Err(bad());
    };
    let Some(LoweredValue::Value(current)) = state.bindings.get(&current) else {
        return Err(bad());
    };
    let [source, length, cursor, element] = state.pending_operands.as_slice() else {
        return Err(bad());
    };
    if state
        .borrow_bindings
        .get(&item)
        .copied()
        .map(EntityId::Loan)
        != bound.then_some(*element)
    {
        return Err(bad());
    }
    Ok([
        EntityId::Value(*items),
        EntityId::Value(*current),
        *source,
        *length,
        *cursor,
        *element,
    ])
}

fn incoming(
    lowerer: &UnitExpressionLowerer<'_>,
    entity: EntityId,
    edge: &Edge,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let Definition::BlockParameter { block, index } =
        lowerer.function.entity(entity).ok_or_else(bad)?.definition
    else {
        return Err(bad());
    };
    if block != edge.target
        || lowerer
            .function
            .block(block)
            .and_then(|block| block.parameters.get(index))
            != Some(&entity)
    {
        return Err(bad());
    }
    let incoming = *edge.arguments.get(index).ok_or_else(bad)?;
    if lowerer.function.entity(incoming).map(|data| data.ty)
        != lowerer.function.entity(entity).map(|data| data.ty)
    {
        return Err(bad());
    }
    Ok(incoming)
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
