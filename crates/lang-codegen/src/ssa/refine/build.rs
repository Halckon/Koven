//! AST + typed + ownership facts → slot CFG. Ownership decisions come only from facts:
//! drops at their published points, loans per Borrow argument, moves per Value delivery.
mod condition;
mod expression;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, Item, Statement},
    source::Span,
    type_checking::{
        Copyability, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    RefineBody, RefineInputs,
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
    type_ids: &'i BTreeMap<UnitTypeId, SsaTypeId>,
    body: RefinedBody,
    /// `None` after a terminator: the rest of the sequence is unreachable.
    block: Option<BlockRef>,
    bindings: BTreeMap<UnitSymbolId, Slot>,
    /// Borrow-mode parameters: the shared loan the caller passed.
    borrowed: BTreeMap<UnitSymbolId, Slot>,
    /// Copyable inner values proven non-null on the current edge.
    narrowed: BTreeMap<UnitSymbolId, Slot>,
    temporaries: BTreeMap<UnitExpressionId, Slot>,
    consumed_drops: BTreeSet<usize>,
}

pub(super) fn build(
    inputs: &RefineInputs<'_>,
    entities: &[crate::ssa::model::EntityId],
    type_ids: &BTreeMap<UnitTypeId, SsaTypeId>,
) -> Result<RefinedBody, LoweringError> {
    let mut builder = Builder {
        inputs,
        type_ids,
        body: RefinedBody::default(),
        block: None,
        bindings: BTreeMap::new(),
        borrowed: BTreeMap::new(),
        narrowed: BTreeMap::new(),
        temporaries: BTreeMap::new(),
        consumed_drops: BTreeSet::new(),
    };
    let entry = builder.body.block();
    builder.block = Some(entry);
    if entities.len() != inputs.parameters.len() {
        return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
    }
    for (symbol, entity) in inputs.parameters.iter().zip(entities) {
        let ty = builder.symbol_ssa_type(*symbol, inputs.span)?;
        match entity {
            crate::ssa::model::EntityId::Value(_) => {
                let slot = builder.body.slot(EntityType::Value(ty));
                builder.body.parameters.push(slot);
                builder.bindings.insert(*symbol, slot);
            }
            crate::ssa::model::EntityId::Loan(_) => {
                let slot = builder.body.slot(EntityType::Loan {
                    kind: crate::ssa::model::LoanKind::Shared,
                    target: ty,
                });
                builder.body.parameters.push(slot);
                builder.borrowed.insert(*symbol, slot);
            }
            crate::ssa::model::EntityId::Place(_) => {
                return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
            }
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
        self.type_ids
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

    fn push(&mut self, op: ROp, results: Vec<Slot>, span: Span) -> Result<(), LoweringError> {
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
        let block = self.current(span)?;
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
        let facts: Vec<_> = self
            .inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(index, fact)| fact.point() == point && !self.consumed_drops.contains(index))
            .map(|(index, fact)| (index, *fact))
            .collect();
        for (index, fact) in facts {
            let span = fact.value_origin();
            let slot = match fact.target() {
                UnitDropTarget::Named(symbol) => self.bindings.get(&symbol).copied(),
                UnitDropTarget::Temporary(expression) => self.temporaries.remove(&expression),
                _ => None,
            }
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.push(ROp::Drop { owner: slot }, Vec::new(), span)?;
            self.consumed_drops.insert(index);
        }
        Ok(())
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
        let header = self.body.block();
        let body_block = self.body.block();
        let exit = self.body.block();
        self.goto(header, span)?;
        self.block = Some(header);
        self.condition(condition, body_block, exit)?;
        self.block = Some(body_block);
        if self.statement(loop_body)? != Lowered::Diverged {
            self.goto(header, span)?;
        }
        self.block = Some(exit);
        self.drops(UnitDropPoint::LoopExit(UnitStatementId::new(
            self.inputs.source_unit,
            statement,
        )))?;
        Ok(Lowered::Unit)
    }
}
