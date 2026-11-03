//! AST + typed + ownership facts → slot CFG. Ownership decisions come only from facts:
//! drops at their published points, loans per Borrow argument, moves per Value delivery.
mod callable;
mod condition;
mod container;
mod container_for;
mod expression;
mod field;
mod heap;
mod inout;
mod map;
mod place;
mod range_borrow;
mod rc;
mod receiver;
mod scalar_text;
mod tagged;
mod value;
mod when;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, Statement},
    source::Span,
    type_checking::{
        Copyability, ParameterMode, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeId,
        UnitTypeKind,
    },
};

use super::{
    RefineBody, RefineInputs, RefineTypes,
    body::{BlockRef, ROp, RStmt, RTerm, RefinedBody, Slot},
};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, SsaTypeId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lowered {
    Value(Slot),
    Unit,
    Diverged,
}

pub(super) struct Builder<'i, 'a> {
    inputs: &'i RefineInputs<'a>,
    types: &'i RefineTypes<'i>,
    body: RefinedBody,
    /// `None` after a terminator: the rest of the sequence is unreachable.
    block: Option<BlockRef>,
    bindings: BTreeMap<UnitSymbolId, Slot>,
    /// Borrow and Inout parameters: the shared or exclusive loan the caller passed.
    borrowed: BTreeMap<UnitSymbolId, Slot>,
    /// Thunk-entry capture loans (parent field loans and shared reference follow loans).
    capture_loans: Vec<Slot>,
    /// The current receiver's value, or the shared loan of a Borrow receiver.
    receiver: Option<Slot>,
    /// Copyable inner values proven non-null on the current edge.
    narrowed: BTreeMap<UnitSymbolId, Slot>,
    temporaries: BTreeMap<UnitExpressionId, Slot>,
    consumed_drops: BTreeSet<usize>,
    /// MoveOnly non-null views created by a condition, keyed by the branch they enter.
    pending_views: Vec<View>,
    /// Views live on the current path, innermost last.
    active_views: Vec<View>,
    /// Enclosing loops, innermost last.
    loops: Vec<LoopFrame>,
    /// Named owners already dropped on the current path, and on entry to each block.
    /// A binding can die inside a loop on one path and at its `LoopExit` on another.
    dead: BTreeSet<UnitSymbolId>,
    this_dead: bool,
    dead_block: Option<BlockRef>,
    entry_dead: BTreeMap<BlockRef, BTreeSet<UnitSymbolId>>,
    entry_this_dead: BTreeSet<BlockRef>,
    /// Loans created for call arguments/receiver that are currently active in the AST evaluation stack.
    pending_call_loans: Vec<Slot>,
    /// Borrowed result/range source loans kept alive until a local `borrow val` binding ends.
    result_source_loans: BTreeMap<UnitSymbolId, Vec<Slot>>,
    /// Value slots of local `borrow val` `RangeView` descriptors (`NewRangeDescriptor`).
    range_views: BTreeMap<UnitSymbolId, Slot>,
    /// Short-lived `RangeView` temporaries created for call arguments or `for` iteration:
    /// `(view_slot, protecting_loan, continued_loans)`.
    short_ranges: BTreeMap<UnitExpressionId, (Slot, Slot, Vec<Slot>)>,
}

/// Jump targets of one loop; views above `views` belong to its iteration.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopFrame {
    pub(super) statement: UnitStatementId,
    pub(super) continue_target: BlockRef,
    pub(super) exit: BlockRef,
    pub(super) views: usize,
    pub(super) loans: usize,
    pub(super) broken: bool,
    pub(super) seq_for: Option<SeqForFrame>,
    pub(super) map_for: Option<MapForFrame>,
}

/// Active sequential-container or `RangeView` `for` provider loans and cursor slots.
#[derive(Clone, Copy, Debug)]
pub(super) struct SeqForFrame {
    pub(super) source_loan: Slot,
    pub(super) elem_loan: Slot,
    pub(super) cursor: Slot,
    pub(super) step: Slot,
    pub(super) int_ty: SsaTypeId,
    pub(super) range_source: Option<ExpressionId>,
}

/// Active `Map` / `MutableMap` `for` provider loans and cursor slots.
#[derive(Clone, Copy, Debug)]
pub(super) struct MapForFrame {
    pub(super) source_loan: Slot,
    pub(super) key_binding: UnitSymbolId,
    pub(super) value_binding: UnitSymbolId,
    pub(super) key_loan: Slot,
    pub(super) value_loan: Slot,
    pub(super) cursor: Slot,
    pub(super) cursor_ty: SsaTypeId,
}

/// A shared proof view of a MoveOnly nullable binding's inner value.
#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    pub(super) symbol: UnitSymbolId,
    pub(super) slot: Slot,
    pub(super) target: BlockRef,
}

pub(super) fn build(
    inputs: &RefineInputs<'_>,
    entities: &[crate::ssa::model::EntityId],
    types: &RefineTypes<'_>,
) -> Result<RefinedBody, LoweringError> {
    let mut builder = Builder {
        inputs,
        types,
        body: RefinedBody::default(),
        block: None,
        bindings: BTreeMap::new(),
        borrowed: BTreeMap::new(),
        capture_loans: Vec::new(),
        receiver: None,
        narrowed: BTreeMap::new(),
        temporaries: BTreeMap::new(),
        consumed_drops: BTreeSet::new(),
        pending_views: Vec::new(),
        active_views: Vec::new(),
        loops: Vec::new(),
        dead: BTreeSet::new(),
        this_dead: false,
        dead_block: None,
        entry_dead: BTreeMap::new(),
        entry_this_dead: BTreeSet::new(),
        pending_call_loans: Vec::new(),
        result_source_loans: BTreeMap::new(),
        range_views: BTreeMap::new(),
        short_ranges: BTreeMap::new(),
    };
    let entry = builder.body.block();
    builder.block = Some(entry);
    let unit_return = builder.is_unit(inputs.return_type);
    if let RefineBody::Lambda(thunk) = &inputs.body {
        builder.bind_thunk_entry(thunk, entities)?;
        builder.lower_thunk_body(thunk, unit_return)?;
        builder.check_all_drops_consumed()?;
        return Ok(builder.body);
    }
    let receivers = usize::from(inputs.receiver.is_some());
    if entities.len() != inputs.parameters.len() + receivers {
        return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
    }
    let (receiver_entities, entities) = entities.split_at(receivers);
    if let (Some(receiver), [entity]) = (inputs.receiver, receiver_entities) {
        let ty = builder.ssa_type(receiver.ty, inputs.span)?;
        let slot = match (entity, loan_kind(receiver.mode)) {
            (crate::ssa::model::EntityId::Value(_), None) => {
                builder.body.slot(EntityType::Value(ty))
            }
            (crate::ssa::model::EntityId::Loan(_), Some(kind)) => {
                builder.body.slot(EntityType::Loan { kind, target: ty })
            }
            _ => return Err(error(LoweringErrorKind::InvalidModel, inputs.span)),
        };
        builder.body.parameters.push(slot);
        builder.receiver = Some(slot);
    }
    builder.bind_parameters(entities)?;
    let function_item = inputs
        .function_item
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, inputs.span))?;
    builder.drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
        inputs.source_unit,
        function_item,
    )))?;
    match &inputs.body {
        RefineBody::Expression(expression) => {
            if inputs.borrow_return.is_some() {
                builder.lower_borrow_return(*expression, *expression, inputs.span)?;
            } else if inputs.carrier_return.is_some() {
                builder.lower_carrier_return(*expression, *expression, inputs.span)?;
            } else {
                let result = match builder.expression(*expression)? {
                    Lowered::Diverged => None,
                    Lowered::Unit => Some(None),
                    Lowered::Value(value) => Some(Some(builder.adapt(
                        value,
                        *expression,
                        inputs.return_type,
                    )?)),
                };
                if let Some(result) = result {
                    builder.deliver(*expression);
                    builder.drops(UnitDropPoint::ControlTransfer(builder.unit(*expression)))?;
                    builder.finish_return(result, unit_return, inputs.span)?;
                }
            }
        }
        RefineBody::Block(statement) => {
            if builder.statement(*statement)? != Lowered::Diverged {
                builder.finish_return(None, unit_return, inputs.span)?;
            }
        }
        RefineBody::Lambda(_) => unreachable!(),
    }
    builder.check_all_drops_consumed()?;
    Ok(builder.body)
}

/// The loan a parameter of `mode` arrives as; `None` for a Value parameter.
const fn loan_kind(mode: ParameterMode) -> Option<crate::ssa::model::LoanKind> {
    match mode {
        ParameterMode::Value => None,
        ParameterMode::Borrow => Some(crate::ssa::model::LoanKind::Shared),
        ParameterMode::Inout => Some(crate::ssa::model::LoanKind::Exclusive),
    }
}

impl Builder<'_, '_> {
    // ---- facts and types ----

    fn unit(&self, expression: ExpressionId) -> UnitExpressionId {
        UnitExpressionId::new(self.inputs.source_unit, expression)
    }

    fn expression_type(&self, expression: ExpressionId) -> Result<UnitTypeId, LoweringError> {
        self.expression_type_of(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.span(expression)))
    }

    /// Typed facts carry template types; a generic instance reads them concretely.
    pub(super) fn expression_type_of(&self, id: UnitExpressionId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .expression_type(id)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    pub(super) fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    /// The concrete identity of a type carried by a typed descriptor; an unresolvable type
    /// stays as it is and has no SSA identity, so its use fails loudly.
    pub(super) fn c(&self, ty: UnitTypeId) -> UnitTypeId {
        self.inputs.concrete(ty).unwrap_or(ty)
    }

    pub(super) fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        self.inputs.typed.copyability(self.c(ty))
    }

    /// The SSA type of a storage type; a Borrow function type is its callable view target.
    fn ssa_type(&self, ty: UnitTypeId, span: Span) -> Result<SsaTypeId, LoweringError> {
        self.types
            .type_ids
            .get(&ty)
            .or_else(|| self.inputs.views.get(&ty))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn symbol_ssa_type(
        &self,
        symbol: UnitSymbolId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.ssa_type(ty, span)
    }

    fn is_unit(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.inputs.typed.types().get(ty),
            Some(UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Unit
            ))
        )
    }

    fn is_nothing(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.inputs.typed.types().get(ty),
            Some(UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Nothing
            ))
        )
    }

    fn move_only(&self, ty: UnitTypeId) -> bool {
        self.copyability_of(ty) == Copyability::MoveOnly
    }

    fn span(&self, expression: ExpressionId) -> Span {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_or(self.inputs.span, |node| node.span())
    }

    fn node(&self, expression: ExpressionId) -> Result<(Expression, Span), LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        Ok((node.payload().clone(), node.span()))
    }

    fn reference(&self, span: Span) -> Result<UnitSymbolId, LoweringError> {
        self.inputs
            .references
            .get(&(span.start(), span.end()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    // ---- CFG construction ----

    fn current(&self, span: Span) -> Result<BlockRef, LoweringError> {
        self.block
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
    }

    /// Loads the dead set of the block now being filled.
    fn sync_dead(&mut self) {
        if self.dead_block != self.block {
            self.dead = self
                .block
                .and_then(|block| self.entry_dead.get(&block).cloned())
                .unwrap_or_default();
            self.this_dead = self
                .block
                .is_some_and(|block| self.entry_this_dead.contains(&block));
            self.dead_block = self.block;
        }
    }

    fn push(&mut self, op: ROp, results: Vec<Slot>, span: Span) -> Result<(), LoweringError> {
        self.sync_dead();
        let block = self.current(span)?;
        self.body.blocks[block.0]
            .statements
            .push(RStmt { op, results, span });
        Ok(())
    }

    /// Appends `op` producing one fresh slot of `ty`.
    fn value(&mut self, op: ROp, ty: EntityType, span: Span) -> Result<Slot, LoweringError> {
        let slot = self.body.slot(ty);
        self.push(op, vec![slot], span)?;
        Ok(slot)
    }

    fn terminate(&mut self, terminator: RTerm, span: Span) -> Result<(), LoweringError> {
        self.sync_dead();
        let block = self.current(span)?;
        for successor in terminator.successors() {
            self.entry_dead
                .entry(successor)
                .or_default()
                .extend(self.dead.iter().copied());
            if self.this_dead {
                self.entry_this_dead.insert(successor);
            }
        }
        self.body.blocks[block.0].terminator = Some((terminator, span));
        self.block = None;
        Ok(())
    }

    fn goto(&mut self, target: BlockRef, span: Span) -> Result<(), LoweringError> {
        if self.block.is_some() {
            self.terminate(RTerm::Goto(target), span)?;
        }
        Ok(())
    }

    pub(super) fn bind_parameters(
        &mut self,
        entities: &[crate::ssa::model::EntityId],
    ) -> Result<(), LoweringError> {
        if self.inputs.modes.len() != self.inputs.parameters.len() {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        for ((symbol, entity), mode) in self
            .inputs
            .parameters
            .iter()
            .zip(entities)
            .zip(&self.inputs.modes)
        {
            let ty = self.symbol_ssa_type(*symbol, self.inputs.span)?;
            match (entity, loan_kind(*mode)) {
                (crate::ssa::model::EntityId::Value(_), None) => {
                    let slot = self.body.slot(EntityType::Value(ty));
                    self.body.parameters.push(slot);
                    self.bindings.insert(*symbol, slot);
                }
                (crate::ssa::model::EntityId::Loan(_), Some(kind)) => {
                    let slot = self.body.slot(EntityType::Loan { kind, target: ty });
                    self.body.parameters.push(slot);
                    self.borrowed.insert(*symbol, slot);
                }
                _ => return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span)),
            }
        }
        Ok(())
    }

    fn finish_return(
        &mut self,
        result: Option<Slot>,
        unit_return: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let result = if unit_return {
            None
        } else {
            Some(result.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?)
        };
        for loan in self.capture_loans.clone().into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.terminate(RTerm::Return(result), span)
    }

    // ---- ownership facts ----

    /// Consumes every drop fact published at `point`, in publication order.
    fn drops(&mut self, point: UnitDropPoint) -> Result<(), LoweringError> {
        self.end_result_bindings(point)?;
        if self
            .loops
            .iter()
            .any(|frame| frame.seq_for.is_some() || frame.map_for.is_some())
            && self.iteration_cleanup(point)?
        {
            return Ok(());
        }
        let facts = self.collect_drops(point);
        self.emit_drops(&facts)?;
        if let UnitDropPoint::ControlTransfer(_) = point {
            // A jump abandons the pending temporaries of the call it leaves; the normal
            // path still delivers them to that call.
            self.consumed_drops
                .extend(facts.iter().map(|(index, _, _)| *index));
        } else {
            self.finish_drops(&facts);
        }
        Ok(())
    }

    /// The not-yet-consumed facts at `point`.
    pub(super) fn collect_drops(&self, point: UnitDropPoint) -> Vec<(usize, UnitDropTarget, Span)> {
        let mut facts: Vec<_> = self
            .inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(index, fact)| fact.point() == point && !self.consumed_drops.contains(index))
            .map(|(index, fact)| (index, fact.target(), fact.value_origin()))
            .collect();
        if let Some(cond) = self
            .inputs
            .owned
            .conditional_receiver_drops()
            .iter()
            .find(|fact| fact.point() == point)
            && self
                .inputs
                .receiver
                .is_some_and(|r| r.owner == cond.owner() && self.move_only(r.ty))
        {
            let pos = cond.preceding_drops().min(facts.len());
            facts.insert(
                pos,
                (
                    usize::MAX,
                    UnitDropTarget::This(cond.owner()),
                    cond.value_origin(),
                ),
            );
        }
        facts
    }

    /// Emits the drops on the current path; a point reached by several edges emits once
    /// per edge before [`Self::finish_drops`] consumes the facts.
    pub(super) fn emit_drops(
        &mut self,
        facts: &[(usize, UnitDropTarget, Span)],
    ) -> Result<(), LoweringError> {
        self.sync_dead();
        for (_, target, span) in facts {
            let slot = match target {
                UnitDropTarget::Named(symbol) => {
                    if let Some(ty) = self.symbol_type(*symbol)
                        && !self.move_only(ty)
                    {
                        continue;
                    }
                    if !self.dead.insert(*symbol) {
                        // This path already dropped the owner (inside a loop it left).
                        continue;
                    }
                    self.end_views_of(*symbol, *span)?;
                    self.bindings.get(symbol).copied()
                }
                // ADR-0009: a closure's drop glue releases its owned captures; the fact is
                // satisfied by the closure's own drop at the same point.
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::This(owner) => {
                    if self.this_dead {
                        continue;
                    }
                    self.this_dead = true;
                    (self.inputs.receiver.map(|receiver| receiver.owner) == Some(*owner))
                        .then_some(self.receiver)
                        .flatten()
                }
                UnitDropTarget::Temporary(expression) => {
                    if let Some(ty) = self.expression_type_of(*expression)
                        && !self.move_only(ty)
                    {
                        continue;
                    }
                    // A moved named argument: its owner is gone on this path.
                    if let Some(symbol) = self.pending_symbol(*expression) {
                        self.dead.insert(symbol);
                    }
                    self.temporaries.get(expression).copied()
                }
                _ => None,
            }
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, *span))?;
            self.push(ROp::Drop { owner: slot }, Vec::new(), *span)?;
        }
        Ok(())
    }

    pub(super) fn finish_drops(&mut self, facts: &[(usize, UnitDropTarget, Span)]) {
        for (index, target, _) in facts {
            if let UnitDropTarget::Temporary(expression) = target {
                self.temporaries.remove(expression);
            }
            self.consumed_drops.insert(*index);
        }
    }

    /// Enters a branch block: views proven for it become active on this path.
    pub(super) fn enter(&mut self, block: BlockRef) -> usize {
        self.block = Some(block);
        let mark = self.active_views.len();
        let (entered, pending): (Vec<View>, Vec<View>) = self
            .pending_views
            .drain(..)
            .partition(|view| view.target == block);
        self.pending_views = pending;
        self.active_views.extend(entered);
        mark
    }

    /// Leaves a branch normally: ends the views it activated that are still live.
    pub(super) fn leave(&mut self, mark: usize, span: Span) -> Result<(), LoweringError> {
        let mark = mark.min(self.active_views.len());
        while self.active_views.len() > mark {
            let view = self.active_views.pop().expect("length checked");
            if self.block.is_some() {
                self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
            }
        }
        Ok(())
    }

    /// Views must not outlive their owner: end them before it is dropped or moved.
    pub(super) fn end_views_of(
        &mut self,
        symbol: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut ended = Vec::new();
        let mut kept = Vec::new();
        for view in self.active_views.drain(..) {
            if view.symbol == symbol {
                ended.push(view);
                for frame in &mut self.loops {
                    if frame.views > kept.len() {
                        frame.views -= 1;
                    }
                }
            } else {
                kept.push(view);
            }
        }
        self.active_views = kept;
        for view in ended {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        Ok(())
    }

    /// A control transfer leaves every active view; they end before the terminator.
    pub(super) fn end_all_views(&mut self, span: Span) -> Result<(), LoweringError> {
        for view in std::mem::take(&mut self.active_views).into_iter().rev() {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        Ok(())
    }

    /// Blocks that never receive a view (loop exits, joins) must not have pending ones.
    pub(super) fn leave_unclaimed(&mut self, mark: usize, span: Span) -> Result<(), LoweringError> {
        if self.active_views.len() != mark || !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    /// The view proving `symbol` non-null on the current path, if any.
    pub(super) fn active_view(&self, symbol: UnitSymbolId) -> Option<Slot> {
        self.active_views
            .iter()
            .rev()
            .find(|view| view.symbol == symbol)
            .map(|view| view.slot)
    }

    /// Every drop fact whose point lies in this function must have been consumed.
    fn check_all_drops_consumed(&self) -> Result<(), LoweringError> {
        let item = self
            .inputs
            .scope_span()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        for (index, fact) in self.inputs.owned.drops().iter().enumerate() {
            if self.consumed_drops.contains(&index)
                || matches!(
                    (&self.inputs.body, fact.target()),
                    (RefineBody::Lambda(thunk), UnitDropTarget::Captured { closure, .. })
                        if thunk.expression == closure
                )
            {
                continue;
            }
            let origin = fact.value_origin();
            if self.point_in_unit(fact.point())
                && origin.source_id() == item.source_id()
                && item.start() <= origin.start()
                && origin.end() <= item.end()
                && (!matches!(self.inputs.body, RefineBody::Lambda(_)) || origin != item)
                && !self.inside_lambda(origin)
            {
                return Err(error(LoweringErrorKind::MissingFact, origin));
            }
        }
        Ok(())
    }

    fn point_in_unit(&self, point: UnitDropPoint) -> bool {
        let unit = self.inputs.source_unit;
        match point {
            UnitDropPoint::AfterExpression(expression)
            | UnitDropPoint::AfterBinaryOperands(expression)
            | UnitDropPoint::CallReturn(expression)
            | UnitDropPoint::ControlTransfer(expression)
            | UnitDropPoint::LambdaEntry(expression)
            | UnitDropPoint::BeforeReplacement(expression)
            | UnitDropPoint::AfterReplacement(expression)
            | UnitDropPoint::BranchExit {
                control: expression,
                ..
            }
            | UnitDropPoint::WhenComparisonCompleted {
                control: expression,
                ..
            }
            | UnitDropPoint::WhenAlternativeNoMatch {
                control: expression,
                ..
            }
            | UnitDropPoint::WhenAlternativeMatch {
                control: expression,
                ..
            } => expression.source_unit() == unit,
            UnitDropPoint::AfterStatement(statement) | UnitDropPoint::LoopExit(statement) => {
                statement.source_unit() == unit
            }
            UnitDropPoint::FunctionEntry(item) => item.source_unit() == unit,
        }
    }

    /// The named owner a pending argument value was moved out of.
    fn pending_symbol(&self, expression: UnitExpressionId) -> Option<UnitSymbolId> {
        if expression.source_unit() != self.inputs.source_unit
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(expression).is_some())
        {
            return None;
        }
        let (payload, span) = self.node(expression.expression()).ok()?;
        matches!(payload, Expression::Name)
            .then(|| self.reference(span).ok())
            .flatten()
    }

    /// Records a MoveOnly temporary so its published drop or delivery can find it; a
    /// value a jump may abandon before its call (a pending argument) is recorded too.
    fn register_temporary(
        &mut self,
        expression: ExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        let ty = self.expression_type(expression)?;
        let unit = self.unit(expression);
        let pending = self.inputs.owned.drops().iter().any(|fact| {
            matches!(fact.point(), UnitDropPoint::ControlTransfer(_))
                && fact.target() == UnitDropTarget::Temporary(unit)
        });
        let is_const_temp = self
            .inputs
            .constant_owned
            .is_some_and(|owned| owned.materialization_at(unit).is_some());
        if self.move_only(ty)
            && (pending
                || is_const_temp
                || self.inputs.typed.expression_category(unit)
                    == Some(lang_frontend::type_checking::ExpressionCategory::Temporary))
        {
            self.temporaries.insert(self.unit(expression), slot);
        }
        Ok(())
    }

    /// A recorded temporary now lives in `slot` (its adapted value).
    fn repoint_temporary(&mut self, mut expression: ExpressionId, slot: Slot) {
        if let Some(recorded) = self.temporaries.get_mut(&self.unit(expression)) {
            *recorded = slot;
        }
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(expression) {
            expression = inner;
            if let Some(recorded) = self.temporaries.get_mut(&self.unit(expression)) {
                *recorded = slot;
            }
        }
    }

    /// The temporary is delivered by value; its owner obligation moves with it. A
    /// delivered MoveOnly named owner is gone on this path: a later exit drop skips it.
    fn deliver(&mut self, expression: ExpressionId) {
        self.temporaries.remove(&self.unit(expression));
        let mut root = expression;
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(root) {
            root = inner;
            self.temporaries.remove(&self.unit(root));
        }
        if matches!(self.node(root), Ok((Expression::This, _)))
            && self
                .inputs
                .receiver
                .is_some_and(|r| r.mode == ParameterMode::Value && self.move_only(r.ty))
        {
            self.deliver_this();
        }
        if let Some(symbol) = self.pending_symbol(self.unit(root))
            && self.bindings.contains_key(&symbol)
            && self
                .expression_type(root)
                .is_ok_and(|ty| self.move_only(ty))
        {
            self.sync_dead();
            self.dead.insert(symbol);
        }
    }

    pub(super) fn deliver_this(&mut self) {
        self.sync_dead();
        self.this_dead = true;
    }
}

impl Builder<'_, '_> {
    // ---- statements ----

    pub(super) fn statement(&mut self, statement: StatementId) -> Result<Lowered, LoweringError> {
        let result = self.statement_inner(statement)?;
        if result != Lowered::Diverged {
            self.drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
                self.inputs.source_unit,
                statement,
            )))?;
        }
        Ok(result)
    }

    fn statement_inner(&mut self, statement: StatementId) -> Result<Lowered, LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        let span = node.span();
        match node.payload().clone() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                let mut last = Lowered::Unit;
                for element in elements {
                    last = self.statement(element)?;
                    if last == Lowered::Diverged {
                        return Ok(Lowered::Diverged);
                    }
                }
                Ok(last)
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration, span),
            Statement::While {
                condition, body, ..
            } => self.while_loop(statement, condition, body, span),
            Statement::Loop { body, .. } => self.loop_statement(statement, body, span),
            Statement::For { source, body, .. } => {
                self.for_statement(statement, source, body, span)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.local_destructuring(statement, initializer, span)
            }
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
}
