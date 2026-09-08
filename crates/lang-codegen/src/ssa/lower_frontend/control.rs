//! `if` 与短路 Boolean 表达式的 CFG 和 block-parameter lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    parser::{
        BinaryOperator, Expression, LiteralKind, Statement, TypeRef, WhenCondition, WhenEntry,
    },
    source::Span,
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error, value,
};
use crate::ssa::model::{
    BlockId, ComparisonOperator, Edge, EntityId, EntityType, Operation, Origin, ScalarConstant,
    TerminatorKind, ValueId,
};

pub(super) struct BranchExit {
    pub(super) block: BlockId,
    pub(super) result: LoweredValue,
    pub(super) bindings: BTreeMap<SymbolId, LoweredValue>,
}

#[derive(Clone, Copy)]
enum MergeSlot {
    Result,
    Binding(SymbolId),
}

#[derive(Clone, Copy)]
struct LinearBindingSlot {
    symbol: SymbolId,
    source: ValueId,
    ty: EntityType,
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
        if let Some(descriptor) = self.typed.null_comparison(condition) {
            return self.lower_nullable_if(expression, descriptor, then_branch, else_branch, span);
        }
        let condition = self.require_value(condition)?;
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let then_block =
            self.add_linear_binding_block(&carried, self.statement_span(then_branch)?)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_linear_binding_block(&carried, else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: linear_binding_edge(then_block, &carried),
                    when_false: linear_binding_edge(else_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let then_baseline = self.rebind_linear_bindings(&baseline, then_block, &carried, span)?;
        let else_baseline = self.rebind_linear_bindings(&baseline, else_block, &carried, span)?;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_control_branch(then_block, then_branch, &then_baseline)? {
            exits.push(exit);
        }
        if let Some(else_branch) = else_branch {
            if let Some(exit) =
                self.lower_control_branch(else_block, else_branch, &else_baseline)?
            {
                exits.push(exit);
            }
        } else {
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                bindings: else_baseline,
            });
        }
        if self.expression_is_unit(expression, span)? {
            discard_exit_results(&mut exits);
        }
        self.merge_exits(exits, &baseline, span)
    }

    fn lower_nullable_if(
        &mut self,
        expression: ExpressionId,
        descriptor: lang_frontend::type_checking::NullComparisonDescriptor,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let symbol = descriptor.symbol();
        let owner = match self.bindings.get(&symbol).copied() {
            Some(LoweredValue::Value(owner)) => owner,
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let nullable = *self
            .type_ids
            .get(&self.resolve_type(descriptor.nullable_type(), span)?)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let inner = self
            .typed
            .types()
            .get(descriptor.nullable_type())
            .and_then(|kind| match kind {
                lang_frontend::type_checking::TypeKind::Nullable(inner) => Some(*inner),
                _ => None,
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let inner = *self
            .type_ids
            .get(&self.resolve_type(inner, span)?)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let branch_types = |proven| {
            let mut types = vec![EntityType::Value(nullable)];
            if proven {
                types.push(EntityType::Loan {
                    kind: crate::ssa::model::LoanKind::Shared,
                    target: inner,
                });
            }
            types
        };
        let then_block = self
            .function
            .add_block(
                branch_types(descriptor.non_null_when_true()),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let else_block = self
            .function
            .add_block(
                branch_types(!descriptor.non_null_when_true()),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let non_null_block = if descriptor.non_null_when_true() {
            then_block
        } else {
            else_block
        };
        let view = self
            .function
            .block(non_null_block)
            .expect("new block exists")
            .parameters[1];
        let EntityId::Loan(view) = view else {
            unreachable!("loan parameter requested")
        };
        let make_edge = |target| Edge {
            target,
            arguments: vec![EntityId::Value(owner)],
        };
        let (when_null, when_non_null) = if descriptor.non_null_when_true() {
            (make_edge(else_block), make_edge(then_block))
        } else {
            (make_edge(then_block), make_edge(else_block))
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null,
                    when_non_null,
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let baseline = self.bindings.clone();
        let outer_non_null = self.non_null_bindings.clone();
        let mut exits = Vec::new();
        for (branch, block, statement, proven) in [
            (
                0,
                then_block,
                Some(then_branch),
                descriptor.non_null_when_true(),
            ),
            (1, else_block, else_branch, !descriptor.non_null_when_true()),
        ] {
            self.block = block;
            self.bindings.clone_from(&baseline);
            let parameter = self
                .function
                .block(block)
                .expect("branch block exists")
                .parameters[0];
            self.bindings
                .insert(symbol, LoweredValue::Value(value(parameter)));
            self.non_null_bindings.clone_from(&outer_non_null);
            if proven {
                self.non_null_bindings.insert(symbol, view);
            }
            let result = match statement {
                Some(statement) => self.lower_control_body(statement)?,
                None => LoweredValue::Unit,
            };
            if !matches!(result, LoweredValue::Diverged) {
                self.emit_drops(lang_frontend::ownership_checking::DropPoint::BranchExit {
                    control: expression,
                    branch,
                })?;
                if proven && self.non_null_bindings.remove(&symbol).is_some() {
                    self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                }
                exits.push(BranchExit {
                    block: self.block,
                    result,
                    bindings: self.bindings.clone(),
                });
            }
        }
        self.non_null_bindings = outer_non_null;
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
        let carried = self.linear_binding_slots(&baseline, span)?;
        let right_span = self.expression_span(right)?;
        let right_block = self.add_linear_binding_block(&carried, right_span)?;
        let short_block = self.add_linear_binding_block(&carried, span)?;
        let (when_true, when_false, short_value) = match operator {
            BinaryOperator::LogicalAnd => (
                linear_binding_edge(right_block, &carried),
                linear_binding_edge(short_block, &carried),
                false,
            ),
            BinaryOperator::LogicalOr => (
                linear_binding_edge(short_block, &carried),
                linear_binding_edge(right_block, &carried),
                true,
            ),
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
        self.bindings = self.rebind_linear_bindings(&baseline, right_block, &carried, span)?;
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
        self.bindings = self.rebind_linear_bindings(&baseline, short_block, &carried, span)?;
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
                let ty = self.resolve_type(ty, span)?;
                let ssa_type = self.expression_ssa_type(subject, self.expression_span(subject)?)?;
                let is_boolean = builtin_type(self.typed, ty)
                    == Some(lang_frontend::type_checking::BuiltinType::Boolean);
                let is_tagged = self.enum_payloads.keys().any(|(root, _)| *root == ssa_type);
                if !is_boolean && !is_tagged {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        self.expression_span(subject)?,
                    ));
                }
                Some((self.require_value(subject)?, is_tagged.then_some(ssa_type)))
            }
            None => None,
        };
        let baseline = self.bindings.clone();
        if let Some((subject, None)) = subject
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
        for (entry_index, entry) in entries.iter().enumerate() {
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
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                self.block = unmatched_block;
                self.bindings.clone_from(&unmatched_bindings);
                let condition = self.lower_when_condition(subject, condition, entry.span)?;
                let after_condition = self.bindings.clone();
                let carried = self.linear_binding_slots(&after_condition, entry.span)?;
                let matched = self.add_linear_binding_block(&carried, entry.span)?;
                let next = self.add_linear_binding_block(&carried, entry.span)?;
                self.function
                    .set_terminator(
                        self.block,
                        TerminatorKind::Conditional {
                            condition,
                            when_true: linear_binding_edge(matched, &carried),
                            when_false: linear_binding_edge(next, &carried),
                        },
                        Origin::Source(entry.span),
                    )
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, entry.span))?;
                self.block = matched;
                self.bindings =
                    self.rebind_linear_bindings(&after_condition, matched, &carried, entry.span)?;
                self.emit_drops(
                    lang_frontend::ownership_checking::DropPoint::WhenAlternativeMatch {
                        control: expression,
                        entry: entry_index,
                        alternative,
                    },
                )?;
                matches.push(BranchExit {
                    block: matched,
                    result: LoweredValue::Unit,
                    bindings: self.bindings.clone(),
                });
                unmatched_block = next;
                unmatched_bindings =
                    self.rebind_linear_bindings(&after_condition, next, &carried, entry.span)?;
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
                if subject.is_some_and(|(_, tagged)| tagged.is_some()) {
                    self.function
                        .set_terminator(
                            unmatched_block,
                            TerminatorKind::Abort,
                            Origin::Source(span),
                        )
                        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                } else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            } else {
                exits.push(BranchExit {
                    block: unmatched_block,
                    result: LoweredValue::Unit,
                    bindings: unmatched_bindings,
                });
            }
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
        subject: Option<(ValueId, Option<crate::ssa::model::SsaTypeId>)>,
        condition: &WhenCondition,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        if let WhenCondition::TypeTest {
            negated, type_ref, ..
        } = condition
        {
            let Some((owner, Some(tagged))) = subject else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let node = self
                .parsed
                .ast()
                .type_refs()
                .get(*type_ref)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let TypeRef::Qualified { segments, .. } = node.payload() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
            };
            let symbol = segments
                .last()
                .and_then(|segment| self.references.get(&super::span_key(segment.name_span)))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            let case = self
                .typed
                .enum_cases()
                .iter()
                .find(|case| case.type_symbol() == symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            let (variant, _) = self
                .enum_payloads
                .get(&(tagged, case.id()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            let integer = self.ssa_builtin(lang_frontend::type_checking::BuiltinType::Int, span)?;
            let boolean =
                self.ssa_builtin(lang_frontend::type_checking::BuiltinType::Boolean, span)?;
            let (_, tag) = self.append(
                Operation::TaggedDiscriminant { owner },
                vec![EntityType::Value(integer)],
                span,
            )?;
            let (_, expected) = self.append(
                Operation::Constant(ScalarConstant::Integer(variant as i128)),
                vec![EntityType::Value(integer)],
                span,
            )?;
            let (_, matches) = self.append(
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: value(tag[0]),
                    right: value(expected[0]),
                },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            let matches = value(matches[0]);
            if !negated {
                return Ok(matches);
            }
            let (_, result) = self.append(
                Operation::BooleanNot { operand: matches },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            return Ok(value(result[0]));
        }
        let WhenCondition::Expression(expression) = condition else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let candidate = self.require_value(*expression)?;
        let Some((subject, None)) = subject else {
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

    fn ssa_builtin(
        &self,
        builtin: lang_frontend::type_checking::BuiltinType,
        span: Span,
    ) -> Result<crate::ssa::model::SsaTypeId, LoweringError> {
        self.type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(builtin)).then_some(*ssa)
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
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

    pub(super) fn merge_exits(
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
            let values = exits
                .iter()
                .map(|exit| exit.bindings.get(&symbol).copied())
                .collect::<Vec<_>>();
            if values.iter().all(Option::is_none) {
                continue;
            }
            let values = values
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let first_value = values[0];
            if values.iter().copied().any(|value| value != first_value) {
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
            .filter_map(|symbol| {
                first
                    .bindings
                    .get(symbol)
                    .copied()
                    .map(|binding| (*symbol, binding))
            })
            .collect::<BTreeMap<_, _>>();
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

    pub(super) fn add_empty_block(&mut self, span: Span) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(Vec::new(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    fn linear_binding_slots(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Vec<LinearBindingSlot>, LoweringError> {
        let mut carried = Vec::new();
        for (&symbol, &binding) in bindings {
            let declared = self
                .typed
                .symbol_type(symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let concrete = self.resolve_type(declared, span)?;
            if self.typed.copyability(concrete)
                != Some(lang_frontend::type_checking::Copyability::MoveOnly)
            {
                continue;
            }
            let LoweredValue::Value(source) = binding else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let ty = self
                .function
                .entity(EntityId::Value(source))
                .map(|entity| entity.ty)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if !matches!(ty, EntityType::Value(_)) {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            carried.push(LinearBindingSlot { symbol, source, ty });
        }
        Ok(carried)
    }

    fn add_linear_binding_block(
        &mut self,
        carried: &[LinearBindingSlot],
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                carried.iter().map(|slot| slot.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    fn rebind_linear_bindings(
        &self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        block: BlockId,
        carried: &[LinearBindingSlot],
        span: Span,
    ) -> Result<BTreeMap<SymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != carried.len() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let mut bindings = baseline.clone();
        for (slot, &parameter) in carried.iter().zip(parameters) {
            bindings.insert(slot.symbol, LoweredValue::Value(value(parameter)));
        }
        Ok(bindings)
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
        let ty = self.resolve_type(ty, span)?;
        Ok(builtin_type(self.typed, ty) == Some(lang_frontend::type_checking::BuiltinType::Unit))
    }
}

fn empty_edge(target: BlockId) -> Edge {
    Edge {
        target,
        arguments: Vec::new(),
    }
}

fn linear_binding_edge(target: BlockId, carried: &[LinearBindingSlot]) -> Edge {
    Edge {
        target,
        arguments: carried
            .iter()
            .map(|slot| EntityId::Value(slot.source))
            .collect(),
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
