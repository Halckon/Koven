//! AST + typed + ownership facts → slot CFG. Ownership decisions come only from facts:
//! drops at their published points, loans per Borrow argument, moves per Value delivery.
mod condition;
mod expression;
mod field;
mod heap;
mod inout;
mod place;
mod receiver;
mod tagged;
mod when;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, Item, Statement},
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
    dead_block: Option<BlockRef>,
    entry_dead: BTreeMap<BlockRef, BTreeSet<UnitSymbolId>>,
}

/// Jump targets of one loop; views above `views` belong to its iteration.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopFrame {
    pub(super) statement: UnitStatementId,
    pub(super) continue_target: BlockRef,
    pub(super) exit: BlockRef,
    pub(super) views: usize,
    pub(super) broken: bool,
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
        receiver: None,
        narrowed: BTreeMap::new(),
        temporaries: BTreeMap::new(),
        consumed_drops: BTreeSet::new(),
        pending_views: Vec::new(),
        active_views: Vec::new(),
        loops: Vec::new(),
        dead: BTreeSet::new(),
        dead_block: None,
        entry_dead: BTreeMap::new(),
    };
    let entry = builder.body.block();
    builder.block = Some(entry);
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
    if inputs.modes.len() != inputs.parameters.len() {
        return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
    }
    for ((symbol, entity), mode) in inputs.parameters.iter().zip(entities).zip(&inputs.modes) {
        let ty = builder.symbol_ssa_type(*symbol, inputs.span)?;
        match (entity, loan_kind(*mode)) {
            (crate::ssa::model::EntityId::Value(_), None) => {
                let slot = builder.body.slot(EntityType::Value(ty));
                builder.body.parameters.push(slot);
                builder.bindings.insert(*symbol, slot);
            }
            (crate::ssa::model::EntityId::Loan(_), Some(kind)) => {
                let slot = builder.body.slot(EntityType::Loan { kind, target: ty });
                builder.body.parameters.push(slot);
                builder.borrowed.insert(*symbol, slot);
            }
            _ => return Err(error(LoweringErrorKind::InvalidModel, inputs.span)),
        }
    }
    builder.drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
        inputs.source_unit,
        inputs.function_item,
    )))?;
    let unit_return = builder.is_unit(inputs.return_type);
    match inputs.body {
        RefineBody::Expression(expression) => {
            let result = match builder.expression(expression)? {
                Lowered::Diverged => None,
                Lowered::Unit => Some(None),
                Lowered::Value(value) => Some(Some(builder.adapt(
                    value,
                    expression,
                    inputs.return_type,
                )?)),
            };
            if let Some(result) = result {
                builder.deliver(expression);
                builder.drops(UnitDropPoint::ControlTransfer(builder.unit(expression)))?;
                builder.finish_return(result, unit_return, inputs.span)?;
            }
        }
        RefineBody::Block(statement) => {
            if builder.statement(statement)? != Lowered::Diverged {
                builder.finish_return(None, unit_return, inputs.span)?;
            }
        }
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
        self.inputs
            .typed
            .expression_type(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.span(expression)))
    }

    fn ssa_type(&self, ty: UnitTypeId, span: Span) -> Result<SsaTypeId, LoweringError> {
        self.types
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn symbol_ssa_type(
        &self,
        symbol: UnitSymbolId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .inputs
            .typed
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
        self.inputs.typed.copyability(ty) == Copyability::MoveOnly
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
        self.terminate(RTerm::Return(result), span)
    }

    // ---- ownership facts ----

    /// Consumes every drop fact published at `point`, in publication order.
    fn drops(&mut self, point: UnitDropPoint) -> Result<(), LoweringError> {
        let facts = self.collect_drops(point);
        self.emit_drops(&facts)?;
        self.finish_drops(&facts);
        Ok(())
    }

    /// The not-yet-consumed facts at `point`.
    pub(super) fn collect_drops(&self, point: UnitDropPoint) -> Vec<(usize, UnitDropTarget, Span)> {
        self.inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(index, fact)| fact.point() == point && !self.consumed_drops.contains(index))
            .map(|(index, fact)| (index, fact.target(), fact.value_origin()))
            .collect()
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
                    if !self.dead.insert(*symbol) {
                        // This path already dropped the owner (inside a loop it left).
                        continue;
                    }
                    self.end_views_of(*symbol, *span)?;
                    self.bindings.get(symbol).copied()
                }
                UnitDropTarget::Temporary(expression) => self.temporaries.get(expression).copied(),
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
        let (ended, kept): (Vec<View>, Vec<View>) = self
            .active_views
            .drain(..)
            .partition(|view| view.symbol == symbol);
        self.active_views = kept;
        for view in ended {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        Ok(())
    }

    /// A control transfer leaves every active view; they end before the terminator.
    pub(super) fn end_all_views(&mut self, span: Span) -> Result<(), LoweringError> {
        for view in self.active_views.clone().into_iter().rev() {
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
            .parsed
            .ast()
            .items()
            .get(self.inputs.function_item)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?
            .span();
        for (index, fact) in self.inputs.owned.drops().iter().enumerate() {
            if self.consumed_drops.contains(&index) {
                continue;
            }
            let origin = fact.value_origin();
            if self.point_in_unit(fact.point())
                && origin.source_id() == item.source_id()
                && item.start() <= origin.start()
                && origin.end() <= item.end()
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

    /// Records a MoveOnly temporary so its published drop or delivery can find it.
    fn register_temporary(
        &mut self,
        expression: ExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        let ty = self.expression_type(expression)?;
        if self.move_only(ty)
            && self.inputs.typed.expression_category(self.unit(expression))
                == Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
        {
            self.temporaries.insert(self.unit(expression), slot);
        }
        Ok(())
    }

    /// The temporary is delivered by value; its owner obligation moves with it.
    fn deliver(&mut self, expression: ExpressionId) {
        self.temporaries.remove(&self.unit(expression));
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
            Statement::Block { elements } | Statement::ControlBody { elements } => {
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
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn local_variable(
        &mut self,
        declaration: lang_frontend::ast::ItemId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .items()
            .get(declaration)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Item::Variable {
            name, initializer, ..
        } = node.payload().clone()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let lang_frontend::parser::NameMarker::Present(name_span) = name else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let symbol = self.declared_symbol(name_span)?;
        let Lowered::Value(value) = self.expression(initializer)? else {
            return Ok(Lowered::Diverged);
        };
        let declared = self
            .inputs
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let value = self.adapt(value, initializer, declared)?;
        self.deliver(initializer);
        let ty = self.ssa_type(declared, span)?;
        let binding = self.value(ROp::Alias { source: value }, EntityType::Value(ty), span)?;
        self.bindings.insert(symbol, binding);
        Ok(Lowered::Unit)
    }

    fn declared_symbol(&self, name_span: Span) -> Result<UnitSymbolId, LoweringError> {
        self.inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == name_span
                        && symbol.kind() == lang_frontend::name_resolution::SymbolKind::Variable
                })
            })
            .and_then(|local| {
                self.inputs
                    .typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .find(|symbol| {
                        symbol.source_unit() == self.inputs.source_unit
                            && symbol.symbol() == local.id()
                    })
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))
    }

    fn while_loop(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement = UnitStatementId::new(self.inputs.source_unit, statement);
        let header = self.body.block();
        let body_block = self.body.block();
        let finished = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;
        self.block = Some(header);
        self.condition(condition, body_block, finished)?;
        let mark = self.enter(body_block);
        self.loops.push(LoopFrame {
            statement,
            continue_target: header,
            exit,
            views: mark,
            broken: false,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.loops.pop();
        self.leave(mark, span)?;
        if reached {
            self.goto(header, span)?;
        }
        // The condition-false edge drops at `LoopExit` on its own path; each `break`
        // dropped on its path before jumping to `exit`.
        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement));
        let mark = self.enter(finished);
        self.leave_unclaimed(mark, span)?;
        self.emit_drops(&facts)?;
        self.goto(exit, span)?;
        self.finish_drops(&facts);
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    /// `loop { body }`: its exit is reached only through `break`.
    fn loop_statement(
        &mut self,
        statement: StatementId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement = UnitStatementId::new(self.inputs.source_unit, statement);
        let header = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;
        let mark = self.enter(header);
        self.loops.push(LoopFrame {
            statement,
            continue_target: header,
            exit,
            views: mark,
            broken: false,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        let frame = self.loops.pop().expect("loop frame pushed above");
        self.leave(mark, span)?;
        if reached {
            self.goto(header, span)?;
        }
        // Every `break` already dropped at `LoopExit` on its own path.
        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement));
        self.finish_drops(&facts);
        if !frame.broken {
            self.block = None;
            return Ok(Lowered::Diverged);
        }
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    /// `break` / `continue`: the jump's drops, then the views of the loop's iteration end.
    pub(super) fn loop_jump(
        &mut self,
        expression: ExpressionId,
        is_break: bool,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        let frame = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        frame.broken |= is_break;
        let frame = *frame;
        let iteration: Vec<View> = self.active_views[frame.views..]
            .iter()
            .rev()
            .copied()
            .collect();
        for view in iteration {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        if is_break {
            // This path leaves the loop: owners dying at `LoopExit` drop here, ending any
            // outer view of theirs; the views stay registered for the other paths.
            for (_, target, fact_span) in
                self.collect_drops(UnitDropPoint::LoopExit(frame.statement))
            {
                let UnitDropTarget::Named(symbol) = target else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, fact_span));
                };
                if !self.dead.insert(symbol) {
                    continue;
                }
                let outer: Vec<View> = self.active_views[..frame.views]
                    .iter()
                    .rev()
                    .copied()
                    .collect();
                for view in outer {
                    if view.symbol == symbol {
                        self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), fact_span)?;
                    }
                }
                let owner = *self
                    .bindings
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact_span))?;
                self.push(ROp::Drop { owner }, Vec::new(), fact_span)?;
            }
        }
        let target = if is_break {
            frame.exit
        } else {
            frame.continue_target
        };
        self.terminate(RTerm::Goto(target), span)?;
        Ok(Lowered::Diverged)
    }
}
