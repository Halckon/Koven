//! `if` 与短路 Boolean 表达式的 CFG 和 block-parameter lowering。

mod carried;
#[path = "control/merge.rs"]
mod merge;
use merge::MergeSlot;
pub(in crate::ssa) mod map_frames;

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
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, NonNullProof, builtin_type,
    error, value,
};
use crate::ssa::carry::{self, IncomingColumns, IncomingRole};
use crate::ssa::model::{
    BlockId, ComparisonOperator, Edge, EntityId, EntityType, Operation, Origin, ScalarConstant,
    TerminatorKind, ValueId,
};

pub(in crate::ssa) struct BranchExit {
    pub(in crate::ssa) block: BlockId,
    pub(in crate::ssa) result: LoweredValue,
    pub(in crate::ssa) bindings: BTreeMap<SymbolId, LoweredValue>,
    pub(in crate::ssa) temporaries: BTreeMap<usize, ValueId>,
    pub(in crate::ssa) loans: super::PendingCallLoans,
    pub(in crate::ssa) capture_loans: BTreeMap<(usize, usize), super::LoanId>,
    pub(in crate::ssa) views: BTreeMap<SymbolId, NonNullProof>,
    pub(in crate::ssa) borrow_bindings: BTreeMap<SymbolId, super::LoanId>,
    pub(in crate::ssa) result_source_loans: BTreeMap<SymbolId, Vec<super::LoanId>>,
    pub(in crate::ssa) for_sources: BTreeMap<usize, super::LoanId>,
    pub(in crate::ssa) for_elements: BTreeMap<usize, super::LoanId>,
    pub(in crate::ssa) map_paths: map_frames::Paths,
}

#[derive(Clone)]
pub(in crate::ssa) struct LinearBindings {
    pub(in crate::ssa) slots: Vec<LinearBindingSlot>,
    forwarded_loans: Vec<(usize, usize)>,
    pub(in crate::ssa) map_paths: map_frames::Paths,
}

#[derive(Clone)]
pub(in crate::ssa) struct LinearBindingSlot {
    pub(in crate::ssa) symbol: Option<SymbolId>,
    pub(in crate::ssa) source: EntityId,
    pub(in crate::ssa) temporaries: Vec<usize>,
    pub(in crate::ssa) loans: Vec<(usize, usize)>,
    pub(in crate::ssa) field_child: Option<super::pending_call::PendingFieldCarry>,
    pub(in crate::ssa) captures: Vec<(usize, usize)>,
    pub(in crate::ssa) views: Vec<(SymbolId, NonNullProof)>,
    pub(in crate::ssa) borrow_symbols: Vec<SymbolId>,
    pub(in crate::ssa) result_sources: Vec<(SymbolId, usize)>,
    pub(in crate::ssa) for_sources: Vec<usize>,
    pub(in crate::ssa) for_elements: Vec<usize>,
    map_sources: Vec<usize>,
    map_cursors: Vec<usize>,
    pub(in crate::ssa) ty: EntityType,
}

impl LinearBindingSlot {
    pub(in crate::ssa) fn has_map_role(&self) -> bool {
        !self.map_sources.is_empty() || !self.map_cursors.is_empty()
    }

    pub(in crate::ssa) fn new(symbol: Option<SymbolId>, source: EntityId, ty: EntityType) -> Self {
        Self {
            symbol,
            source,
            temporaries: Vec::new(),
            loans: Vec::new(),
            field_child: None,
            captures: Vec::new(),
            views: Vec::new(),
            borrow_symbols: Vec::new(),
            result_sources: Vec::new(),
            for_sources: Vec::new(),
            for_elements: Vec::new(),
            map_sources: Vec::new(),
            map_cursors: Vec::new(),
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

    pub(in crate::ssa) fn is_entry_loan(&self, loan: super::LoanId) -> bool {
        self.entry_loans().contains(&loan)
    }

    pub(in crate::ssa) fn branch_exit(&self, result: LoweredValue) -> BranchExit {
        BranchExit {
            block: self.block,
            result,
            bindings: self.bindings.clone(),
            temporaries: self.temporaries.clone(),
            loans: self.pending_call_loans.clone(),
            capture_loans: self.capture_loans.clone(),
            views: self.non_null_bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            result_source_loans: self.result_source_loans.clone(),
            map_paths: self.map_path_snapshots(),
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
        let mut proof_condition = condition;
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(proof_condition)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            proof_condition = *expression;
        }
        if let Some(descriptor) = self.typed.null_comparison(proof_condition)
            && !self.borrow_bindings.contains_key(&descriptor.symbol())
            && self.typed.copyability(descriptor.nullable_type())
                != Some(lang_frontend::type_checking::Copyability::Copyable)
        {
            return self.lower_nullable_if(
                expression,
                condition,
                descriptor,
                then_branch,
                else_branch,
                span,
            );
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
        self.retire_non_common_if_while_proofs(&mut exits, span)?;
        self.merge_exits(exits, &baseline, span)
    }

    fn lower_nullable_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        descriptor: lang_frontend::type_checking::NullComparisonDescriptor,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let symbol = descriptor.symbol();
        self.reject_live_while_proof(symbol, span)?;
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
        let choice = carry::associate_source(
            self.function,
            &mut carried.slots,
            EntityId::Value(owner),
            carry::TypeAtMiss::Known(EntityType::Value(nullable)),
            error(LoweringErrorKind::MissingFact, span),
        )?;
        if let carry::ColumnMatch::Added(column) = choice {
            carried.slots[column.index()].symbol = Some(symbol);
        }
        let owner_slot = choice.column().index();
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
                let owner = value(
                    self.function
                        .block(block)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .parameters[owner_slot],
                );
                let proof = NonNullProof::introduced(
                    view,
                    owner,
                    inner,
                    super::non_null_proof::ProofScope::If(expression),
                );
                self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
                self.non_null_bindings.insert(symbol, proof);
            }
            self.emit_null_condition_drops(condition)?;
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
                if proven {
                    self.end_non_null_proof(symbol, span)?;
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
            self.emit_drops(lang_frontend::ownership_checking::DropPoint::BranchExit {
                control: expression,
                branch: 0,
            })?;
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
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::BranchExit {
            control: expression,
            branch: 1,
        })?;
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
        let mut unmatched_captures = self.capture_loans.clone();
        let mut unmatched_maps = self.map_path_snapshots();
        let mut unmatched_borrows = self.borrow_bindings.clone();
        let mut exits = Vec::new();
        let mut has_else = false;
        for (entry_index, entry) in entries.iter().enumerate() {
            self.temporaries.clone_from(&unmatched_temporaries);
            self.pending_call_loans.clone_from(&unmatched_loans);
            self.non_null_bindings.clone_from(&unmatched_views);
            self.capture_loans.clone_from(&unmatched_captures);
            self.borrow_bindings.clone_from(&unmatched_borrows);
            if !unmatched_maps.is_empty() {
                self.restore_map_paths(
                    &unmatched_maps,
                    &unmatched_bindings,
                    unmatched_block,
                    entry.span,
                )?;
            }
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
                unmatched_captures = self.capture_loans.clone();
                unmatched_maps = self.map_path_snapshots();
                unmatched_borrows = self.borrow_bindings.clone();
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
                    capture_loans: unmatched_captures,
                    views: unmatched_views,
                    borrow_bindings: unmatched_borrows,
                    map_paths: unmatched_maps,
                    result_source_loans: self.result_source_loans.clone(),
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
            return self.lower_enum_tag_test(owner, variant, *negated, span);
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

    pub(in crate::ssa) fn ssa_builtin(
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

    pub(in crate::ssa) fn lower_control_body(
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
        crate::ssa::statement_core::lower_tail_sequence(self, statement, elements)
    }

    pub(in crate::ssa) fn expression_span(
        &self,
        expression: ExpressionId,
    ) -> Result<Span, LoweringError> {
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

    pub(in crate::ssa) fn expression_is_unit(
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
