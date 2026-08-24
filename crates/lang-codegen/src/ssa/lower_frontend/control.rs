//! `if` 与短路 Boolean 表达式的 CFG 和 block-parameter lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    parser::{BinaryOperator, Expression, LiteralKind, Statement, WhenCondition, WhenEntry},
    source::Span,
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error, value,
};
use crate::ssa::model::{
    BlockId, ComparisonOperator, Edge, EntityId, EntityType, Operation, Origin, ScalarConstant,
    TerminatorKind, ValueId,
};

struct BranchExit {
    block: BlockId,
    result: LoweredValue,
    bindings: BTreeMap<SymbolId, LoweredValue>,
}

#[derive(Clone, Copy)]
enum MergeSlot {
    Result,
    Binding(SymbolId),
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let condition = self.require_value(condition)?;
        let baseline = self.bindings.clone();
        let then_block = self.add_empty_block(self.statement_span(then_branch)?)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_empty_block(else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: empty_edge(then_block),
                    when_false: empty_edge(else_block),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_control_branch(then_block, then_branch, &baseline)? {
            exits.push(exit);
        }
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_control_branch(else_block, else_branch, &baseline)? {
                exits.push(exit);
            }
        } else {
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                bindings: baseline.clone(),
            });
        }
        if self.expression_is_unit(expression, span)? {
            discard_exit_results(&mut exits);
        }
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_short_circuit(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let left = self.require_value(left)?;
        let baseline = self.bindings.clone();
        let right_span = self.expression_span(right)?;
        let right_block = self.add_empty_block(right_span)?;
        let short_block = self.add_empty_block(span)?;
        let (when_true, when_false, short_value) = match operator {
            BinaryOperator::LogicalAnd => (empty_edge(right_block), empty_edge(short_block), false),
            BinaryOperator::LogicalOr => (empty_edge(short_block), empty_edge(right_block), true),
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: left,
                    when_true,
                    when_false,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        self.block = right_block;
        self.bindings.clone_from(&baseline);
        let right_result = self.lower(right)?;
        let mut exits = Vec::with_capacity(2);
        if !matches!(right_result, LoweredValue::Diverged) {
            if !matches!(right_result, LoweredValue::Value(_)) {
                return Err(error(LoweringErrorKind::MissingFact, right_span));
            }
            exits.push(BranchExit {
                block: self.block,
                result: right_result,
                bindings: self.bindings.clone(),
            });
        }

        self.block = short_block;
        self.bindings.clone_from(&baseline);
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            crate::ssa::model::Operation::Constant(ScalarConstant::Boolean(short_value)),
            vec![EntityType::Value(ty)],
            span,
        )?;
        exits.push(BranchExit {
            block: self.block,
            result: LoweredValue::Value(value(results[0])),
            bindings: self.bindings.clone(),
        });
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_when(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let discard_result = self.expression_is_unit(expression, span)?;
        let subject = match subject {
            Some(subject) => {
                let ty = self
                    .typed
                    .expression_type(subject)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if builtin_type(self.typed, ty)
                    != Some(lang_frontend::type_checking::BuiltinType::Boolean)
                {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        self.expression_span(subject)?,
                    ));
                }
                Some(self.require_value(subject)?)
            }
            None => None,
        };
        let baseline = self.bindings.clone();
        if let Some(subject) = subject
            && let Some((when_true, when_false)) = self.boolean_literal_entries(entries)?
        {
            return self.lower_boolean_literal_when(
                subject,
                when_true,
                when_false,
                &baseline,
                discard_result,
                span,
            );
        }

        let mut unmatched_block = self.block;
        let mut unmatched_bindings = baseline.clone();
        let mut exits = Vec::new();
        let mut has_else = false;
        for entry in entries {
            if entry.else_span.is_some() {
                has_else = true;
                if let Some(exit) =
                    self.lower_control_branch(unmatched_block, entry.body, &unmatched_bindings)?
                {
                    exits.push(exit);
                }
                break;
            }
            if entry.conditions.is_empty() {
                return Err(error(LoweringErrorKind::MissingFact, entry.span));
            }
            let entry_baseline = unmatched_bindings.clone();
            let mut matches = Vec::new();
            for condition in &entry.conditions {
                self.block = unmatched_block;
                self.bindings.clone_from(&unmatched_bindings);
                let condition = self.lower_when_condition(subject, condition, entry.span)?;
                let matched = self.add_empty_block(entry.span)?;
                let next = self.add_empty_block(entry.span)?;
                self.function
                    .set_terminator(
                        self.block,
                        TerminatorKind::Conditional {
                            condition,
                            when_true: empty_edge(matched),
                            when_false: empty_edge(next),
                        },
                        Origin::Source(entry.span),
                    )
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, entry.span))?;
                let after_condition = self.bindings.clone();
                matches.push(BranchExit {
                    block: matched,
                    result: LoweredValue::Unit,
                    bindings: after_condition.clone(),
                });
                unmatched_block = next;
                unmatched_bindings = after_condition;
            }

            self.merge_exits(matches, &entry_baseline, entry.span)?;
            let result = self.lower_control_body(entry.body)?;
            if !matches!(result, LoweredValue::Diverged) {
                exits.push(BranchExit {
                    block: self.block,
                    result,
                    bindings: self.bindings.clone(),
                });
            }
        }

        if !has_else {
            if !discard_result {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            exits.push(BranchExit {
                block: unmatched_block,
                result: LoweredValue::Unit,
                bindings: unmatched_bindings,
            });
        }
        if discard_result {
            discard_exit_results(&mut exits);
        }
        self.merge_exits(exits, &baseline, span)
    }

    fn lower_boolean_literal_when(
        &mut self,
        subject: ValueId,
        when_true: StatementId,
        when_false: StatementId,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        discard_result: bool,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let true_block = self.add_empty_block(self.statement_span(when_true)?)?;
        let false_block = if when_false == when_true {
            true_block
        } else {
            self.add_empty_block(self.statement_span(when_false)?)?
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: subject,
                    when_true: empty_edge(true_block),
                    when_false: empty_edge(false_block),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_control_branch(true_block, when_true, baseline)? {
            exits.push(exit);
        }
        if when_false != when_true
            && let Some(exit) = self.lower_control_branch(false_block, when_false, baseline)?
        {
            exits.push(exit);
        }
        if discard_result {
            discard_exit_results(&mut exits);
        }
        self.merge_exits(exits, baseline, span)
    }

    fn boolean_literal_entries(
        &self,
        entries: &[WhenEntry],
    ) -> Result<Option<(StatementId, StatementId)>, LoweringError> {
        if entries.iter().any(|entry| entry.else_span.is_some()) {
            return Ok(None);
        }
        let mut when_true = None;
        let mut when_false = None;
        for entry in entries {
            for condition in &entry.conditions {
                let WhenCondition::Expression(expression) = condition else {
                    return Ok(None);
                };
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(*expression)
                    .map_err(|_| LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
                match node.payload() {
                    Expression::Literal(LiteralKind::Boolean(true)) => when_true = Some(entry.body),
                    Expression::Literal(LiteralKind::Boolean(false)) => {
                        when_false = Some(entry.body)
                    }
                    _ => return Ok(None),
                }
            }
        }
        Ok(when_true.zip(when_false))
    }

    fn lower_when_condition(
        &mut self,
        subject: Option<ValueId>,
        condition: &WhenCondition,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let WhenCondition::Expression(expression) = condition else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let candidate = self.require_value(*expression)?;
        let Some(subject) = subject else {
            return Ok(candidate);
        };
        let ty = self.expression_ssa_type(*expression, span)?;
        let (_, results) = self.append(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: subject,
                right: candidate,
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(value(results[0]))
    }

    fn lower_control_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        self.bindings.clone_from(baseline);
        let result = self.lower_control_body(statement)?;
        if matches!(result, LoweredValue::Diverged) {
            return Ok(None);
        }
        Ok(Some(BranchExit {
            block: self.block,
            result,
            bindings: self.bindings.clone(),
        }))
    }

    fn lower_control_body(
        &mut self,
        statement: StatementId,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let Statement::ControlBody { elements } = node.payload() else {
            return self.lower_statement(statement);
        };
        let Some((&last, prefix)) = elements.split_last() else {
            return Ok(LoweredValue::Unit);
        };
        for &element in prefix {
            if matches!(self.lower_statement(element)?, LoweredValue::Diverged) {
                return Ok(LoweredValue::Diverged);
            }
        }
        self.lower_statement(last)
    }

    fn merge_exits(
        &mut self,
        exits: Vec<BranchExit>,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(LoweredValue::Diverged);
        };
        if exits.len() == 1 {
            self.block = first.block;
            self.bindings = first.bindings.clone();
            return Ok(first.result);
        }

        let mut slots = Vec::new();
        let mut parameter_types = Vec::new();
        let first_result = first.result;
        if exits.iter().any(|exit| exit.result != first_result) {
            parameter_types
                .push(self.merged_value_type(exits.iter().map(|exit| exit.result), span)?);
            slots.push(MergeSlot::Result);
        }
        for &symbol in baseline.keys() {
            let first_value = first
                .bindings
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if exits
                .iter()
                .any(|exit| exit.bindings.get(&symbol).copied() != Some(first_value))
            {
                let values = exits
                    .iter()
                    .map(|exit| {
                        exit.bindings
                            .get(&symbol)
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                parameter_types.push(self.merged_value_type(values.into_iter(), span)?);
                slots.push(MergeSlot::Binding(symbol));
            }
        }

        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        for exit in &exits {
            let arguments = slots
                .iter()
                .map(|slot| match slot {
                    MergeSlot::Result => lowered_entity(exit.result, span),
                    MergeSlot::Binding(symbol) => lowered_entity(
                        exit.bindings
                            .get(symbol)
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                        span,
                    ),
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.function
                .set_terminator(
                    exit.block,
                    TerminatorKind::Branch(Edge {
                        target: merge,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        }

        let parameters = self
            .function
            .block(merge)
            .expect("new merge block must exist")
            .parameters
            .clone();
        let mut result = first_result;
        let mut bindings = baseline
            .keys()
            .map(|symbol| {
                first
                    .bindings
                    .get(symbol)
                    .copied()
                    .map(|binding| (*symbol, binding))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        for (slot, parameter) in slots.into_iter().zip(parameters) {
            let parameter = LoweredValue::Value(value(parameter));
            match slot {
                MergeSlot::Result => result = parameter,
                MergeSlot::Binding(symbol) => {
                    bindings.insert(symbol, parameter);
                }
            }
        }
        self.block = merge;
        self.bindings = bindings;
        Ok(result)
    }

    fn merged_value_type(
        &self,
        values: impl Iterator<Item = LoweredValue>,
        span: Span,
    ) -> Result<EntityType, LoweringError> {
        let mut ty = None;
        for value in values {
            let entity = lowered_entity(value, span)?;
            let current = self
                .function
                .entity(entity)
                .map(|data| data.ty)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if ty.is_some_and(|expected| expected != current) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            ty = Some(current);
        }
        ty.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn add_empty_block(&mut self, span: Span) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(Vec::new(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    fn expression_span(&self, expression: ExpressionId) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .expressions()
            .get(expression)
            .map(|node| node.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }

    fn statement_span(&self, statement: StatementId) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|node| node.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }

    fn expression_is_unit(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        Ok(builtin_type(self.typed, ty) == Some(lang_frontend::type_checking::BuiltinType::Unit))
    }
}

fn empty_edge(target: BlockId) -> Edge {
    Edge {
        target,
        arguments: Vec::new(),
    }
}

fn lowered_entity(value: LoweredValue, span: Span) -> Result<EntityId, LoweringError> {
    match value {
        LoweredValue::Value(value) => Ok(EntityId::Value(value)),
        LoweredValue::Unit | LoweredValue::Diverged => {
            Err(error(LoweringErrorKind::MissingFact, span))
        }
    }
}

fn discard_exit_results(exits: &mut [BranchExit]) {
    for exit in exits {
        exit.result = LoweredValue::Unit;
    }
}
