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
    pub(super) temporaries: BTreeMap<usize, ValueId>,
    pub(super) loans: BTreeMap<(usize, usize), Option<super::LoanId>>,
    pub(super) views: BTreeMap<SymbolId, super::LoanId>,
    pub(super) borrow_bindings: BTreeMap<SymbolId, super::LoanId>,
    pub(super) for_sources: BTreeMap<usize, super::LoanId>,
    pub(super) for_elements: BTreeMap<usize, super::LoanId>,
}

#[derive(Clone, Copy)]
enum MergeSlot {
    Result,
    Binding(SymbolId),
    Temporary(usize),
    PendingLoan((usize, usize)),
    NonNullView(SymbolId),
    BorrowBinding(SymbolId),
    ForSource(usize),
    ForElement(usize),
}

#[derive(Clone)]
pub(super) struct LinearBindings {
    pub(super) slots: Vec<LinearBindingSlot>,
    forwarded_loans: Vec<(usize, usize)>,
}

#[derive(Clone)]
pub(super) struct LinearBindingSlot {
    pub(super) symbol: Option<SymbolId>,
    pub(super) source: EntityId,
    pub(super) temporaries: Vec<usize>,
    pub(super) loans: Vec<(usize, usize)>,
    pub(super) views: Vec<SymbolId>,
    pub(super) borrow_symbols: Vec<SymbolId>,
    pub(super) for_sources: Vec<usize>,
    pub(super) for_elements: Vec<usize>,
    pub(super) ty: EntityType,
}

impl LinearBindingSlot {
    pub(super) fn new(symbol: Option<SymbolId>, source: EntityId, ty: EntityType) -> Self {
        Self {
            symbol,
            source,
            temporaries: Vec::new(),
            loans: Vec::new(),
            views: Vec::new(),
            borrow_symbols: Vec::new(),
            for_sources: Vec::new(),
            for_elements: Vec::new(),
            ty,
        }
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn entry_loans(&self) -> Vec<super::LoanId> {
        self.function
            .entry_block()
            .and_then(|entry| self.function.block(entry))
            .map(|block| {
                block
                    .parameters
                    .iter()
                    .filter_map(|e| match e {
                        EntityId::Loan(loan) => Some(*loan),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn is_entry_loan(&self, loan: super::LoanId) -> bool {
        self.entry_loans().contains(&loan)
    }

    pub(super) fn branch_exit(&self, result: LoweredValue) -> BranchExit {
        BranchExit {
            block: self.block,
            result,
            bindings: self.bindings.clone(),
            temporaries: self.temporaries.clone(),
            loans: self.pending_call_loans.clone(),
            views: self.non_null_bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            for_sources: self
                .loops
                .iter()
                .filter_map(|c| {
                    c.for_loop
                        .as_ref()
                        .map(|f| (f.statement.index(), f.body_source))
                })
                .collect(),
            for_elements: self
                .loops
                .iter()
                .filter_map(|c| {
                    c.for_loop
                        .as_ref()
                        .map(|f| (f.statement.index(), f.guarded_element.loan()))
                })
                .collect(),
        }
    }

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

        let mut exits = Vec::with_capacity(2);
        for (branch, block, statement) in [
            (0, then_block, Some(then_branch)),
            (1, else_block, else_branch),
        ] {
            self.block = block;
            self.bindings = self.rebind_linear_bindings(&baseline, block, &carried, span)?;
            let result = match statement {
                Some(statement) => self.lower_control_body(statement)?,
                None => LoweredValue::Unit,
            };
            if matches!(result, LoweredValue::Diverged) {
                continue;
            }
            self.transfer_branch_result(expression, result);
            self.emit_drops(lang_frontend::ownership_checking::DropPoint::BranchExit {
                control: expression,
                branch,
            })?;
            exits.push(self.branch_exit(result));
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
        let baseline = self.bindings.clone();
        let mut carried = self.linear_binding_slots(&baseline, span)?;
        // Copyable nullable subjects also need a parameter for the proof owner.
        if !carried
            .slots
            .iter()
            .any(|slot| slot.source == EntityId::Value(owner))
        {
            carried.slots.push(LinearBindingSlot::new(
                Some(symbol),
                EntityId::Value(owner),
                EntityType::Value(nullable),
            ));
        }
        let branch_types = |proven| {
            let mut types: Vec<_> = carried.slots.iter().map(|slot| slot.ty).collect();
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
            .parameters[carried.slots.len()];
        let EntityId::Loan(view) = view else {
            unreachable!("loan parameter requested")
        };
        let make_edge = |target| Edge {
            target,
            arguments: carried.slots.iter().map(|slot| slot.source).collect(),
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
            self.bindings = self.rebind_linear_bindings(&baseline, block, &carried, span)?;
            if proven {
                self.non_null_bindings.insert(symbol, view);
            }
            self.emit_null_condition_drops(descriptor.expression())?;
            let result = match statement {
                Some(statement) => self.lower_control_body(statement)?,
                None => LoweredValue::Unit,
            };
            if !matches!(result, LoweredValue::Diverged) {
                self.transfer_branch_result(expression, result);
                self.emit_drops(lang_frontend::ownership_checking::DropPoint::BranchExit {
                    control: expression,
                    branch,
                })?;
                if proven && let Some(view) = self.non_null_bindings.remove(&symbol) {
                    self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                }
                exits.push(self.branch_exit(result));
            }
        }
        if self.expression_is_unit(expression, span)? {
            discard_exit_results(&mut exits);
        }
        self.merge_exits(exits, &baseline, span)
    }

    // The result slot owns a moved branch value; source aliases cannot also cross the join.
    fn transfer_branch_result(&mut self, expression: ExpressionId, result: LoweredValue) {
        let move_result = self
            .typed
            .expression_type(expression)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(lang_frontend::type_checking::Copyability::MoveOnly);
        if move_result && let LoweredValue::Value(owner) = result {
            self.bindings.retain(
                |_, binding| !matches!(binding, LoweredValue::Value(value) if *value == owner),
            );
            self.temporaries.retain(|_, value| *value != owner);
        }
    }

    // The discriminator replaces condition evaluation, but its ASAP cleanup still
    // belongs after the read, on each runtime branch, before the branch body.
    fn emit_null_condition_drops(&mut self, expression: ExpressionId) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload().clone() {
            Expression::Binary { left, right, .. } => {
                self.emit_null_condition_drops(left)?;
                self.emit_null_condition_drops(right)?;
            }
            Expression::Group { expression } => self.emit_null_condition_drops(expression)?,
            _ => {}
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::AfterExpression(expression))
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
            exits.push(self.branch_exit(right_result));
        }

        self.block = short_block;
        self.bindings = self.rebind_linear_bindings(&baseline, short_block, &carried, span)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            crate::ssa::model::Operation::Constant(ScalarConstant::Boolean(short_value)),
            vec![EntityType::Value(ty)],
            span,
        )?;
        exits.push(self.branch_exit(LoweredValue::Value(value(results[0]))));
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_when(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self.typed.nullable_when(expression).is_some() {
            return self.lower_nullable_when(expression, entries, span);
        }
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
        let mut unmatched_temporaries = self.temporaries.clone();
        let mut unmatched_loans = self.pending_call_loans.clone();
        let mut unmatched_views = self.non_null_bindings.clone();
        let mut exits = Vec::new();
        let mut has_else = false;
        for (entry_index, entry) in entries.iter().enumerate() {
            self.temporaries.clone_from(&unmatched_temporaries);
            self.pending_call_loans.clone_from(&unmatched_loans);
            self.non_null_bindings.clone_from(&unmatched_views);
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
                matches.push(self.branch_exit(LoweredValue::Unit));
                unmatched_block = next;
                unmatched_bindings =
                    self.rebind_linear_bindings(&after_condition, next, &carried, entry.span)?;
                unmatched_temporaries = self.temporaries.clone();
                unmatched_loans = self.pending_call_loans.clone();
                unmatched_views = self.non_null_bindings.clone();
            }

            self.merge_exits(matches, &entry_baseline, entry.span)?;
            let result = self.lower_control_body(entry.body)?;
            if !matches!(result, LoweredValue::Diverged) {
                exits.push(self.branch_exit(result));
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
                    temporaries: unmatched_temporaries,
                    loans: unmatched_loans,
                    views: unmatched_views,
                    borrow_bindings: self.borrow_bindings.clone(),
                    for_sources: self
                        .loops
                        .iter()
                        .filter_map(|c| {
                            c.for_loop
                                .as_ref()
                                .map(|f| (f.statement.index(), f.body_source))
                        })
                        .collect(),
                    for_elements: self
                        .loops
                        .iter()
                        .filter_map(|c| {
                            c.for_loop
                                .as_ref()
                                .map(|f| (f.statement.index(), f.guarded_element.loan()))
                        })
                        .collect(),
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
        let carried = self.linear_binding_slots(baseline, span)?;
        let true_block =
            self.add_linear_binding_block(&carried, self.statement_span(when_true)?)?;
        let false_block = if when_false == when_true {
            true_block
        } else {
            self.add_linear_binding_block(&carried, self.statement_span(when_false)?)?
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: subject,
                    when_true: linear_binding_edge(true_block, &carried),
                    when_false: linear_binding_edge(false_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::with_capacity(2);
        let true_baseline = self.rebind_linear_bindings(baseline, true_block, &carried, span)?;
        if let Some(exit) = self.lower_control_branch(true_block, when_true, &true_baseline)? {
            exits.push(exit);
        }
        if when_false != when_true {
            let false_baseline =
                self.rebind_linear_bindings(baseline, false_block, &carried, span)?;
            if let Some(exit) =
                self.lower_control_branch(false_block, when_false, &false_baseline)?
            {
                exits.push(exit);
            }
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
        if let Some((owner, Some(tagged))) = subject {
            return self.lower_copyable_enum_case_condition(owner, tagged, *expression);
        }
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

    pub(super) fn ssa_builtin(
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
        Ok(Some(self.branch_exit(result)))
    }

    pub(super) fn lower_control_body(
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
        let result = self.lower_statement(last)?;
        if !matches!(result, LoweredValue::Diverged) {
            self.emit_drops(
                lang_frontend::ownership_checking::DropPoint::AfterStatement(statement),
            )?;
        }
        Ok(result)
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
            self.temporaries = first.temporaries.clone();
            self.pending_call_loans = first.loans.clone();
            self.non_null_bindings = first.views.clone();
            self.borrow_bindings = first.borrow_bindings.clone();
            for context in &mut self.loops {
                if let Some(ref mut for_data) = context.for_loop {
                    if let Some(&loan) = first.for_sources.get(&for_data.statement.index()) {
                        for_data.rebind_source(loan);
                    }
                    if let Some(&loan) = first.for_elements.get(&for_data.statement.index()) {
                        for_data.guarded_element.set_loan(loan);
                    }
                }
            }
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

        let mut temporaries = first.temporaries.clone();
        temporaries.retain(|key, _| exits.iter().all(|exit| exit.temporaries.contains_key(key)));
        for (&key, &owner) in &temporaries {
            parameter_types.push(
                self.function
                    .entity(EntityId::Value(owner))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::Temporary(key));
        }
        let mut loans = first.loans.clone();
        loans.retain(|key, _| exits.iter().all(|exit| exit.loans.contains_key(key)));
        for (&key, &loan) in &loans {
            if let Some(loan) = loan {
                parameter_types.push(
                    self.function
                        .entity(EntityId::Loan(loan))
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty,
                );
                slots.push(MergeSlot::PendingLoan(key));
            }
        }

        let mut views = first.views.clone();
        views.retain(|key, _| exits.iter().all(|exit| exit.views.contains_key(key)));
        for (&symbol, &loan) in &views {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::NonNullView(symbol));
        }

        let entry_loans = self.entry_loans();
        let mut borrow_bindings = first.borrow_bindings.clone();
        borrow_bindings.retain(|key, loan| {
            !entry_loans.contains(loan)
                && exits
                    .iter()
                    .all(|exit| exit.borrow_bindings.contains_key(key))
        });
        for (&symbol, &loan) in &borrow_bindings {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::BorrowBinding(symbol));
        }

        let mut for_sources = first.for_sources.clone();
        for_sources.retain(|key, _| exits.iter().all(|exit| exit.for_sources.contains_key(key)));
        for (&stmt, &loan) in &for_sources {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::ForSource(stmt));
        }

        let mut for_elements = first.for_elements.clone();
        for_elements.retain(|key, _| exits.iter().all(|exit| exit.for_elements.contains_key(key)));
        for (&stmt, &loan) in &for_elements {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::ForElement(stmt));
        }

        // Group 等表达式记录可指向同一实体；所有入边都相同时共享一个参数。
        let mut columns = Vec::new();
        let mut unique_types = Vec::new();
        let mut slot_parameters = Vec::with_capacity(slots.len());
        for (slot, ty) in slots.iter().zip(parameter_types) {
            let column = exits
                .iter()
                .map(|exit| match slot {
                    MergeSlot::Result => lowered_entity(exit.result, span),
                    MergeSlot::NonNullView(symbol) => Ok(EntityId::Loan(exit.views[symbol])),
                    MergeSlot::Temporary(key) => Ok(EntityId::Value(exit.temporaries[key])),
                    MergeSlot::PendingLoan(key) => exit.loans[key]
                        .map(EntityId::Loan)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
                    MergeSlot::Binding(symbol) => lowered_entity(
                        exit.bindings
                            .get(symbol)
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                        span,
                    ),
                    MergeSlot::BorrowBinding(symbol) => {
                        Ok(EntityId::Loan(exit.borrow_bindings[symbol]))
                    }
                    MergeSlot::ForSource(stmt) => Ok(EntityId::Loan(exit.for_sources[stmt])),
                    MergeSlot::ForElement(stmt) => Ok(EntityId::Loan(exit.for_elements[stmt])),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let parameter = if let Some(index) = columns.iter().position(|other| *other == column) {
                index
            } else {
                let index = columns.len();
                columns.push(column);
                unique_types.push(ty);
                index
            };
            slot_parameters.push(parameter);
        }
        let merge = self
            .function
            .add_block(unique_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        for (index, exit) in exits.iter().enumerate() {
            let arguments = columns.iter().map(|column| column[index]).collect();
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
        for (slot, index) in slots.into_iter().zip(slot_parameters) {
            let parameter = parameters[index];
            if let MergeSlot::PendingLoan(key) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                loans.insert(key, Some(loan));
                continue;
            }
            if let MergeSlot::NonNullView(symbol) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                views.insert(symbol, loan);
                continue;
            }
            if let MergeSlot::BorrowBinding(symbol) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                borrow_bindings.insert(symbol, loan);
                continue;
            }
            if let MergeSlot::ForSource(stmt) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for_sources.insert(stmt, loan);
                continue;
            }
            if let MergeSlot::ForElement(stmt) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for_elements.insert(stmt, loan);
                continue;
            }
            let parameter = LoweredValue::Value(value(parameter));
            match slot {
                MergeSlot::Result => result = parameter,
                MergeSlot::Temporary(key) => {
                    if let LoweredValue::Value(owner) = parameter {
                        temporaries.insert(key, owner);
                    }
                }
                MergeSlot::PendingLoan(_)
                | MergeSlot::NonNullView(_)
                | MergeSlot::BorrowBinding(_)
                | MergeSlot::ForSource(_)
                | MergeSlot::ForElement(_) => {
                    unreachable!("handled loan parameter")
                }
                MergeSlot::Binding(symbol) => {
                    bindings.insert(symbol, parameter);
                }
            }
        }
        self.block = merge;
        self.bindings = bindings;
        self.temporaries = temporaries;
        self.pending_call_loans = loans;
        self.non_null_bindings = views;
        self.borrow_bindings
            .retain(|_, loan| entry_loans.contains(loan));
        self.borrow_bindings.extend(borrow_bindings);
        for context in &mut self.loops {
            if let Some(ref mut for_data) = context.for_loop {
                if let Some(&loan) = for_sources.get(&for_data.statement.index()) {
                    for_data.rebind_source(loan);
                }
                if let Some(&loan) = for_elements.get(&for_data.statement.index()) {
                    for_data.guarded_element.set_loan(loan);
                }
            }
        }
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

    pub(super) fn linear_binding_slots(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LinearBindings, LoweringError> {
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
            carried.push(LinearBindingSlot::new(
                Some(symbol),
                EntityId::Value(source),
                ty,
            ));
        }
        for (&key, &owner) in &self.temporaries {
            let source = EntityId::Value(owner);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.temporaries.push(key);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.temporaries.push(key);
                carried.push(slot);
            }
        }
        for (&key, &loan) in &self.pending_call_loans {
            let Some(loan) = loan else {
                continue;
            };
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.loans.push(key);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.loans.push(key);
                carried.push(slot);
            }
        }
        for (&symbol, &loan) in &self.non_null_bindings {
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.views.push(symbol);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.views.push(symbol);
                carried.push(slot);
            }
        }
        for (&symbol, &loan) in &self.borrow_bindings {
            if self.is_entry_loan(loan) {
                continue;
            }
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.borrow_symbols.push(symbol);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.borrow_symbols.push(symbol);
                carried.push(slot);
            }
        }
        for context in &self.loops {
            let Some(ref for_data) = context.for_loop else {
                continue;
            };
            let stmt = for_data.statement.index();
            let body_source_loan = for_data.body_source;
            if !self.is_entry_loan(body_source_loan) {
                let source = EntityId::Loan(body_source_loan);
                if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                    slot.for_sources.push(stmt);
                } else {
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    let mut slot = LinearBindingSlot::new(None, source, ty);
                    slot.for_sources.push(stmt);
                    carried.push(slot);
                }
            }
            let element_loan = for_data.guarded_element.loan();
            if !self.is_entry_loan(element_loan) {
                let source = EntityId::Loan(element_loan);
                if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                    slot.for_elements.push(stmt);
                } else {
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    let mut slot = LinearBindingSlot::new(None, source, ty);
                    slot.for_elements.push(stmt);
                    carried.push(slot);
                }
            }
        }
        Ok(LinearBindings {
            slots: carried,
            forwarded_loans: self
                .pending_call_loans
                .iter()
                .filter_map(|(&key, loan)| loan.is_none().then_some(key))
                .collect(),
        })
    }

    fn add_linear_binding_block(
        &mut self,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                carried.slots.iter().map(|slot| slot.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    pub(super) fn rebind_linear_bindings(
        &mut self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        block: BlockId,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<BTreeMap<SymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() < carried.slots.len() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        // Restore the complete entry state before lowering each sibling branch.
        self.temporaries.clear();
        self.non_null_bindings.clear();
        let entry_loans = self.entry_loans();
        self.borrow_bindings
            .retain(|_, loan| entry_loans.contains(loan));
        self.pending_call_loans = carried
            .forwarded_loans
            .iter()
            .map(|&key| (key, None))
            .collect();
        let mut bindings = baseline.clone();
        for (slot, &parameter) in carried.slots.iter().zip(parameters) {
            if let Some(symbol) = slot.symbol {
                bindings.insert(symbol, LoweredValue::Value(value(parameter)));
            }
            for key in &slot.temporaries {
                self.temporaries.insert(*key, value(parameter));
            }
            for symbol in &slot.views {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.non_null_bindings.insert(*symbol, loan);
            }
            for key in &slot.loans {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.pending_call_loans.insert(*key, Some(loan));
            }
            for symbol in &slot.borrow_symbols {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.borrow_bindings.insert(*symbol, loan);
            }
            for &stmt in &slot.for_sources {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for context in &mut self.loops {
                    if let Some(ref mut for_data) = context.for_loop
                        && for_data.statement.index() == stmt
                    {
                        for_data.rebind_source(loan);
                    }
                }
            }
            for &stmt in &slot.for_elements {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for context in &mut self.loops {
                    if let Some(ref mut for_data) = context.for_loop
                        && for_data.statement.index() == stmt
                    {
                        for_data.guarded_element.set_loan(loan);
                    }
                }
            }
        }
        Ok(bindings)
    }

    pub(super) fn expression_span(&self, expression: ExpressionId) -> Result<Span, LoweringError> {
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

    pub(super) fn expression_is_unit(
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

fn linear_binding_edge(target: BlockId, carried: &LinearBindings) -> Edge {
    Edge {
        target,
        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
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
