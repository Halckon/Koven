//! A fresh Resource remove owner inside one sequential iteration, not an inherited proof.
use super::{iteration::IterationState, *};
use super::super::{builtin_type, resolve_concrete_type};
use crate::ssa::model::{EntityId, EntityType, InstructionId, LoanKind, SsaTypeId, SsaTypeKind, ValueId};
use lang_frontend::{
    parser::Statement,
    type_checking::{BuiltinType, IterationProvider, UnitTypeId},
};

/// One immediate CFG transition only. No proof or owner is installed into loop state.
pub(in super::super) struct MapBorrowIteration {
    root: UnitSymbolId,
    owner: ValueId,
    block: BlockId,
    pending: Vec<EntityId>,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn qualify_map_borrow_iteration(
        &self,
        root: UnitSymbolId,
        owner: ValueId,
        remove: InstructionId,
        narrowed: UnitTypeId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<MapBorrowIteration>, LoweringError> {
        if self.loops.is_empty() {
            return Ok(None);
        }
        let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        let [context] = self.loops.as_slice() else {
            return Err(unsupported());
        };
        let Some(iteration) = &context.iteration else {
            return Err(unsupported());
        };
        let IterationState::Borrowed { start, element, int_type, .. } = iteration.state else {
            return Err(unsupported());
        };
        let [frame] = self.pending_call_frames.as_slice() else {
            return Err(unsupported());
        };
        if context.proof_scope.is_some()
            || iteration.inherited_borrowed.is_some()
            || !self.nullable_proofs.is_empty()
            || context.entry_symbols.contains(&root)
            || iteration.retained_start != 0
            || frame.receiver
            || frame.loop_depth != 1
            || frame.pending_start != element + 1
        {
            return Err(unsupported());
        }
        match self.typed.is_resource_type(narrowed) {
            Some(true) => {}
            Some(false) => return Err(unsupported()),
            None => return Err(missing()),
        }
        match self.ssa_types.get(target.index()) {
            Some(SsaTypeKind::HeapOwner { .. }) => {}
            Some(_) => return Err(unsupported()),
            None => return Err(missing()),
        }
        let call = self.typed.call(frame.call).ok_or_else(missing)?;
        let result = resolve_concrete_type(
            self.typed, call.return_type(), self.substitutions, self.static_self, span,
        )?;
        if builtin_type(self.typed, result) != Some(BuiltinType::Unit) {
            return Err(unsupported());
        }
        let call_span = self.parsed.ast().expressions().get(frame.call.expression())
            .map_err(|_| missing())?.span();
        // The proof is call-local. Later operands may branch or exit, but must not
        // carry this newly admitted proof into another loop header/backedge.
        if self.parsed.ast().statements().iter().any(|(_, statement)| {
            let at = statement.span();
            at.source_id() == call_span.source_id() && call_span.start() <= at.start()
                && at.end() <= call_span.end()
                && matches!(statement.payload(), Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. })
        }) {
            return Err(unsupported());
        }
        let plan = self.owned.iteration(iteration.statement).ok_or_else(missing)?;
        let descriptor = plan.descriptor();
        if self.typed.sequential_iteration(iteration.statement) != Some(descriptor)
            || !matches!(descriptor.provider(), IterationProvider::Sequential(_))
            || descriptor.statement().source_unit() != self.source_unit
            || root.source_unit() != self.source_unit
            || element != context.entry_pending_count
            || start + 3 != element
        {
            return Err(missing());
        }
        let statement = self.parsed.ast().statements()
            .get(iteration.statement.statement()).map_err(|_| missing())?;
        let Statement::For { body, .. } = statement.payload() else {
            return Err(missing());
        };
        let body_span = self.statement_span(*body)?;
        let origin = self.function.instruction(remove).ok_or_else(missing)?.origin.span();
        let symbol = self.names.names().source_units()
            .get(root.source_unit().index()).ok_or_else(missing)?
            .resolution().symbols().get(root.symbol().index()).ok_or_else(missing)?;
        let inside = |inner: Span| inner.source_id() == body_span.source_id()
            && body_span.start() <= inner.start() && inner.end() <= body_span.end();
        // Position bounds the supported slice; provenance was proved from actual SSA
        // incoming edges to the unique MapRemove result 1, never from this span test.
        if !inside(origin) || !inside(symbol.span()) || !inside(span) {
            return Err(unsupported());
        }
        let source = self.iteration_type(descriptor.source_type(), span)?;
        let item = self.iteration_type(descriptor.element_type(), span)?;
        for (slot, expected) in [
            (start, EntityType::Loan { kind: LoanKind::Shared, target: source }),
            (start + 1, EntityType::Value(int_type)),
            (start + 2, EntityType::Value(int_type)),
            (element, EntityType::Loan { kind: LoanKind::Shared, target: item }),
        ] {
            let entity = self.pending_operands.get(slot).ok_or_else(missing)?;
            if self.function.entity(*entity).map(|data| data.ty) != Some(expected) {
                return Err(missing());
            }
        }
        Ok(Some(MapBorrowIteration {
            root, owner, block: self.block, pending: self.pending_operands.clone(),
        }))
    }

    pub(in super::super) fn validate_map_borrow_iteration_transport(
        &self,
        before: MapBorrowIteration,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(TerminatorKind::NullableBranch { owner, when_non_null, view, .. }) = self.function
            .block(before.block).and_then(|block| block.terminator.as_ref()).map(|term| &term.kind)
        else {
            return Err(bad());
        };
        let parameters = &self.function.block(self.block).ok_or_else(bad)?.parameters;
        if *owner != before.owner || when_non_null.target != self.block
            || parameters.len() != when_non_null.arguments.len() + 1
            || parameters.last() != Some(&EntityId::Loan(*view))
            || before.pending.len() != self.pending_operands.len()
        {
            return Err(bad());
        }
        let transported = |prior: EntityId, current: EntityId| -> bool {
            let mut columns = when_non_null.arguments.iter().enumerate()
                .filter_map(|(index, argument)| (*argument == prior).then_some(index));
            let Some(column) = columns.next() else { return false };
            columns.next().is_none() && parameters.get(column) == Some(&current)
                && self.function.entity(prior).map(|data| data.ty)
                    == self.function.entity(current).map(|data| data.ty)
        };
        let Some(LoweredValue::Value(current)) = self.bindings.get(&before.root) else {
            return Err(bad());
        };
        if !transported(EntityId::Value(before.owner), EntityId::Value(*current))
            || before.pending.iter().zip(&self.pending_operands)
                .any(|(prior, current)| !transported(*prior, *current))
        {
            return Err(bad());
        }
        Ok(())
    }
}
