//! compilation-unit `if` 的 owner-aware CFG 与 branch-state 合流。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{BuiltinType, Copyability, UnitExpressionId, UnitStatementId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, cfg::carried_edge, lowering_error,
    resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Edge, EntityId, Origin, TerminatorKind},
};

struct BranchExit {
    block: BlockId,
    result: LoweredValue,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.lower_conditional(expression, condition, then_branch, else_branch, 0, 1, span)
    }

    pub(super) fn lower_boolean_when(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let subject =
            subject.ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let subject_type = self
            .typed
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, subject))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let subject_type =
            resolve_concrete_type(self.typed, subject_type, self.substitutions, span)?;
        if builtin_type(self.typed, subject_type) != Some(BuiltinType::Boolean) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut when_true = None;
        let mut when_false = None;
        for (index, entry) in entries.iter().enumerate() {
            let [WhenCondition::Expression(condition)] = entry.conditions.as_slice() else {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    entry.span,
                ));
            };
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(*condition)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, entry.span))?;
            let slot = match node.payload() {
                Expression::Literal(LiteralKind::Boolean(true)) => &mut when_true,
                Expression::Literal(LiteralKind::Boolean(false)) => &mut when_false,
                _ => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        node.span(),
                    ));
                }
            };
            if slot.replace((entry.body, index)).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, entry.span));
            }
        }
        let (true_body, true_index) =
            when_true.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (false_body, false_index) =
            when_false.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.lower_conditional(
            expression,
            subject,
            true_body,
            Some(false_body),
            true_index,
            false_index,
            span,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_conditional(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        then_index: usize,
        else_index: usize,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let control = UnitExpressionId::new(self.source_unit, expression);
        let expression_type = self
            .typed
            .types()
            .expression_type(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_type =
            resolve_concrete_type(self.typed, expression_type, self.substitutions, span)?;
        let result_required = builtin_type(self.typed, concrete_type) != Some(BuiltinType::Unit);
        if !self.temporaries.is_empty()
            || (result_required
                && (builtin_type(self.typed, concrete_type).is_none()
                    || self.typed.types().copyability(concrete_type) != Copyability::Copyable))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let condition = self.require_expression_value(condition)?;
        let baseline = self.bindings.clone();
        let carried = self.carried_bindings(&baseline, span)?;
        let then_block = self.add_carried_block(&carried, self.statement_span(then_branch)?)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_carried_block(&carried, else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: carried_edge(then_block, &carried),
                    when_false: carried_edge(else_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        let then_baseline = self.rebind_carried(&baseline, then_block, &carried, span)?;
        let else_baseline = self.rebind_carried(&baseline, else_block, &carried, span)?;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_baseline,
            UnitDropPoint::BranchExit {
                control,
                branch: then_index,
            },
            result_required,
        )? {
            exits.push(exit);
        }
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_if_branch(
                else_block,
                else_branch,
                else_baseline,
                UnitDropPoint::BranchExit {
                    control,
                    branch: else_index,
                },
                result_required,
            )? {
                exits.push(exit);
            }
        } else {
            if result_required {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.block = else_block;
            self.bindings = else_baseline;
            self.temporaries.clear();
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: else_index,
            })?;
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                bindings: self.bindings.clone(),
            });
        }
        self.merge_unit_exits(exits, span)
    }

    fn lower_if_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        bindings: BTreeMap<UnitSymbolId, LoweredValue>,
        drop_point: UnitDropPoint,
        result_required: bool,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        let entry_symbols = bindings.keys().copied().collect();
        self.bindings = bindings;
        self.temporaries.clear();
        let result = if result_required {
            self.lower_control_body(statement)?
        } else {
            self.lower_statement(statement)?
        };
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        let expected_result = if result_required {
            matches!(result, LoweredValue::Value(_))
        } else {
            result == LoweredValue::Unit
        };
        if !expected_result || !self.temporaries.is_empty() {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                self.statement_span(statement)?,
            ));
        }
        self.emit_drops(drop_point)?;
        self.discard_non_entry_bindings(&entry_symbols, self.statement_span(statement)?)?;
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
        let lang_frontend::parser::Statement::ControlBody { elements } = node.payload() else {
            return self.lower_statement(statement);
        };
        let elements = elements.clone();
        let Some((&last, prefix)) = elements.split_last() else {
            return Ok(LoweredValue::Unit);
        };
        for &element in prefix {
            if self.lower_statement(element)? == LoweredValue::Diverged {
                return Ok(LoweredValue::Diverged);
            }
        }
        let result = self.lower_statement(last)?;
        if result != LoweredValue::Diverged {
            self.emit_drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
                self.source_unit,
                statement,
            )))?;
        }
        Ok(result)
    }

    fn merge_unit_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            self.bindings.clear();
            self.temporaries.clear();
            return Ok(LoweredValue::Diverged);
        };
        if exits.len() == 1 {
            self.block = first.block;
            self.bindings = first.bindings.clone();
            self.temporaries.clear();
            return Ok(first.result);
        }
        let symbols = first.bindings.keys().copied().collect::<Vec<_>>();
        if exits.iter().any(|exit| {
            exit.bindings.keys().copied().collect::<Vec<_>>() != symbols
                || std::mem::discriminant(&exit.result) != std::mem::discriminant(&first.result)
                || symbols.iter().any(|symbol| {
                    std::mem::discriminant(&exit.bindings[symbol])
                        != std::mem::discriminant(&first.bindings[symbol])
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let result_type = match first.result {
            LoweredValue::Unit => None,
            LoweredValue::Value(value) => Some(
                self.function
                    .entity(EntityId::Value(value))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?,
            ),
            LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
        };
        if let Some(expected) = result_type
            && exits.iter().any(|exit| match exit.result {
                LoweredValue::Value(value) => self
                    .function
                    .entity(EntityId::Value(value))
                    .is_none_or(|entity| entity.ty != expected),
                LoweredValue::Unit | LoweredValue::Diverged => true,
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let value_symbols = symbols
            .iter()
            .copied()
            .filter(|symbol| matches!(first.bindings[symbol], LoweredValue::Value(_)))
            .collect::<Vec<_>>();
        let mut parameter_types = result_type.into_iter().collect::<Vec<_>>();
        parameter_types.extend(
            value_symbols
                .iter()
                .map(|symbol| match first.bindings[symbol] {
                    LoweredValue::Value(value) => self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|entity| entity.ty)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span)),
                    LoweredValue::Unit | LoweredValue::Diverged => {
                        Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for exit in &exits {
            let mut arguments = match exit.result {
                LoweredValue::Unit => Vec::new(),
                LoweredValue::Value(value) => vec![EntityId::Value(value)],
                LoweredValue::Diverged => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
            };
            arguments.extend(
                value_symbols
                    .iter()
                    .map(|symbol| match exit.bindings[symbol] {
                        LoweredValue::Value(value) => Ok(EntityId::Value(value)),
                        LoweredValue::Unit | LoweredValue::Diverged => {
                            Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            self.function
                .set_terminator(
                    exit.block,
                    TerminatorKind::Branch(Edge {
                        target: merge,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        let parameters = self
            .function
            .block(merge)
            .expect("new merge block exists")
            .parameters
            .clone();
        let mut parameters = parameters.into_iter();
        let result = if result_type.is_some() {
            let EntityId::Value(value) = parameters
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            LoweredValue::Value(value)
        } else {
            LoweredValue::Unit
        };
        let mut bindings = first.bindings.clone();
        for (symbol, parameter) in value_symbols.into_iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            bindings.insert(symbol, LoweredValue::Value(value));
        }
        self.block = merge;
        self.bindings = bindings;
        self.temporaries.clear();
        Ok(result)
    }

    pub(super) fn statement_span(&self, statement: StatementId) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|statement| statement.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }
}
