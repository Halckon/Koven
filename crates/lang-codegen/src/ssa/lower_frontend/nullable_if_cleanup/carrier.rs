//! Check the finite, already-emitted CFG. No saved selector or owner-version sidecar.
use super::super::non_null_proof::ProofScope;
use super::*;
use crate::ssa::model::{Definition, SsaTypeKind};

impl ExpressionLowerer<'_> {
    pub(super) fn nullable_if_cleanup_owner(&self, plan: &GroupPlan) -> Option<ValueId> {
        let function = &*self.function;
        if self.thunk_expression.is_some()
            || function.origin != Origin::Source(plan.function_span)
            || function.receiver.is_some()
            || self.loops.len() != 1
        {
            return None;
        }
        let context = self.loops.first()?;
        if context.for_loop.is_some()
            || context.numeric_loop.is_some()
            || !context.continues.is_empty()
            || !context.breaks.is_empty()
            || !context.entry_views.is_empty()
        {
            return None;
        }
        let nullable = *self.type_ids.get(&plan.storage)?;
        let SsaTypeKind::NullableHandle { inner } = self.ssa_types.get(nullable.index())? else {
            return None;
        };
        let inner = *inner;
        if !matches!(
            self.ssa_types.get(inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        ) {
            return None;
        }
        let Some(LoweredValue::Value(current)) = self.bindings.get(&plan.root).copied() else {
            return None;
        };
        let (current_block, current_column) = parameter(function, EntityId::Value(current))?;
        if current_block != self.block
            || function.block(self.block)?.terminator.is_some()
            || !value_type(function, current, nullable)
        {
            return None;
        }
        let current_incoming = incoming(function, self.block);
        let [(join, selected)] = current_incoming.as_slice() else {
            return None;
        };
        let term = function.block(*join)?.terminator.as_ref()?;
        let TerminatorKind::NullableBranch {
            owner: merged,
            when_null,
            when_non_null,
            view,
        } = &term.kind
        else {
            return None;
        };
        if term.origin != Origin::Source(self.expression_span(plan.second_if).ok()?)
            || when_null.target == when_non_null.target
            || *selected
                != if plan.arm == 0 {
                    when_non_null
                } else {
                    when_null
                }
            || selected.arguments.get(current_column) != Some(&EntityId::Value(*merged))
            || selected
                .arguments
                .iter()
                .filter(|entity| **entity == EntityId::Value(*merged))
                .count()
                != 1
            || !value_type(function, *merged, nullable)
        {
            return None;
        }
        if plan.arm == 0 {
            let expected =
                NonNullProof::introduced(*view, current, inner, ProofScope::If(plan.second_if));
            if self.non_null_bindings.len() != 1
                || self.non_null_bindings.get(&plan.root) != Some(&expected)
                || parameter(function, EntityId::Loan(*view))?.0 != self.block
                || self
                    .validate_non_null_proof(
                        plan.root,
                        expected,
                        &self.bindings,
                        plan.members[0].value_origin(),
                    )
                    .is_err()
            {
                return None;
            }
        } else if !self.non_null_bindings.is_empty() {
            return None;
        }

        let (merge_block, merge_column) = parameter(function, EntityId::Value(*merged))?;
        if merge_block != *join {
            return None;
        }
        let first_span = self.expression_span(plan.first_if).ok()?;
        let (body, first) = unique_terminator(function, first_span)?;
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &first.kind
        else {
            return None;
        };
        if when_true.target == when_false.target {
            return None;
        }
        let join_incoming = incoming(function, *join);
        if join_incoming.len() != 2 {
            return None;
        }
        let true_join = branch_to(function, when_true.target, *join, first_span)?;
        let false_join = branch_to(function, when_false.target, *join, first_span)?;
        if !join_incoming.contains(&(when_true.target, true_join))
            || !join_incoming.contains(&(when_false.target, false_join))
            || incoming(function, when_true.target).as_slice() != [(body, when_true)]
            || incoming(function, when_false.target).as_slice() != [(body, when_false)]
        {
            return None;
        }
        let replacement = as_value(*true_join.arguments.get(merge_column)?)?;
        let inherited = as_value(*false_join.arguments.get(merge_column)?)?;
        let (inherited_block, owner_column) = parameter(function, EntityId::Value(inherited))?;
        if inherited_block != when_false.target {
            return None;
        }
        let body_owner = as_value(*when_false.arguments.get(owner_column)?)?;
        let old_owner = as_value(
            *function
                .block(when_true.target)?
                .parameters
                .get(owner_column)?,
        )?;
        if when_true.arguments.get(owner_column) != Some(&EntityId::Value(body_owner))
            || parameter(function, EntityId::Value(old_owner))? != (when_true.target, owner_column)
            || ![replacement, inherited, body_owner, old_owner]
                .into_iter()
                .all(|value| value_type(function, value, nullable))
        {
            return None;
        }

        // Anchor the CURRENT iteration owner to the real nullable-while header column.
        // The initial parameter is only the source of that column, never the current ID.
        let (body_block, body_column) = parameter(function, EntityId::Value(body_owner))?;
        if body_block != body {
            return None;
        }
        let while_span = self
            .parsed
            .ast()
            .statements()
            .get(plan.while_statement)
            .ok()?
            .span();
        let header_term = function.block(context.header)?.terminator.as_ref()?;
        let TerminatorKind::NullableBranch {
            owner: header_owner,
            when_non_null: body_edge,
            when_null: outer_false,
            view: outer_view,
        } = &header_term.kind
        else {
            return None;
        };
        if header_term.origin != Origin::Source(while_span)
            || body_edge.target != body
            || outer_false.target == body
            || body_edge.arguments.get(body_column) != Some(&EntityId::Value(*header_owner))
            || incoming(function, body).as_slice() != [(context.header, body_edge)]
            || parameter(function, EntityId::Loan(*outer_view))?.0 != body
            || !value_type(function, *header_owner, nullable)
            || function.entity(EntityId::Loan(*outer_view))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
        {
            return None;
        }
        let (header_block, header_column) = parameter(function, EntityId::Value(*header_owner))?;
        let slot = context.carried.slots.get(header_column)?;
        if header_block != context.header
            || slot.symbol != Some(plan.root)
            || slot.ty != EntityType::Value(nullable)
            || context
                .carried
                .slots
                .iter()
                .filter(|slot| slot.symbol == Some(plan.root))
                .count()
                != 1
        {
            return None;
        }
        let entry = function.entry_block()?;
        let parameters = &function.block(entry)?.parameters;
        let seed = *parameters.get(plan.seed_parameter)?;
        let preheader = incoming(function, context.header);
        let [(preheader_block, preheader_edge)] = preheader.as_slice() else {
            return None;
        };
        if parameters.len() != plan.parameter_count
            || slot.source != seed
            || parameter(function, seed)? != (entry, plan.seed_parameter)
            || *preheader_block != entry
            || !matches!(
                function.block(entry)?.terminator.as_ref()?.kind,
                TerminatorKind::Branch(_)
            )
            || preheader_edge.arguments.get(header_column) != Some(&seed)
            || !value_type(function, as_value(seed)?, nullable)
        {
            return None;
        }

        // Both ordinary-if arms retire the real inherited view. The true arm drops
        // its old owner exactly once AFTER creating null and ending that view.
        let true_view = forwarded_loan(function, when_true, *outer_view)?;
        let false_view = forwarded_loan(function, when_false, *outer_view)?;
        if [true_view, false_view].into_iter().any(|loan| {
            function
                .entity(EntityId::Loan(loan))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
        }) {
            return None;
        }
        let true_instructions = &function.block(when_true.target)?.instructions;
        let [null_id, end_id, drop_id] = true_instructions.as_slice() else {
            return None;
        };
        let null = function.instruction(*null_id)?;
        let end = function.instruction(*end_id)?;
        let drop = function.instruction(*drop_id)?;
        if null.block != when_true.target
            || null.origin != Origin::Source(self.expression_span(plan.replacement).ok()?)
            || !matches!(null.operation, Operation::NullableNull { nullable: actual } if actual == nullable)
            || null.results.as_slice() != [EntityId::Value(replacement)]
            || function.entity(EntityId::Value(replacement))?.definition
                != (Definition::InstructionResult {
                    instruction: *null_id,
                    index: 0,
                })
            || end.block != when_true.target
            || end.origin != Origin::Source(plan.old_drop.value_origin())
            || !matches!(end.operation, Operation::BorrowEnd { loan } if loan == true_view)
            || drop.block != when_true.target
            || drop.origin != Origin::Source(plan.old_drop.value_origin())
            || !matches!(drop.operation, Operation::Drop { owner } if owner == old_owner)
            || true_join.arguments.contains(&EntityId::Value(old_owner))
        {
            return None;
        }
        let [false_end] = function.block(when_false.target)?.instructions.as_slice() else {
            return None;
        };
        let false_end = function.instruction(*false_end)?;
        if false_end.block != when_false.target
            || false_end.origin != Origin::Source(first_span)
            || !matches!(false_end.operation, Operation::BorrowEnd { loan } if loan == false_view)
        {
            return None;
        }
        Some(current)
    }
}

fn as_value(entity: EntityId) -> Option<ValueId> {
    match entity {
        EntityId::Value(value) => Some(value),
        _ => None,
    }
}

fn value_type(function: &Function, value: ValueId, ty: SsaTypeId) -> bool {
    function
        .entity(EntityId::Value(value))
        .is_some_and(|entity| entity.ty == EntityType::Value(ty))
}

fn parameter(function: &Function, entity: EntityId) -> Option<(BlockId, usize)> {
    let Definition::BlockParameter { block, index } = function.entity(entity)?.definition else {
        return None;
    };
    (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
}

fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Branch(edge)) => vec![edge],
                Some(TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }) => vec![when_true, when_false],
                Some(TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                }) => vec![when_null, when_non_null],
                _ => Vec::new(),
            };
            edges
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

fn unique_terminator(
    function: &Function,
    span: Span,
) -> Option<(BlockId, &crate::ssa::model::Terminator)> {
    let mut terms = function.blocks.iter().filter_map(|block| {
        let term = block.terminator.as_ref()?;
        (term.origin == Origin::Source(span)
            && matches!(term.kind, TerminatorKind::Conditional { .. }))
        .then_some((block.id, term))
    });
    let term = terms.next()?;
    terms.next().is_none().then_some(term)
}

fn branch_to(function: &Function, block: BlockId, target: BlockId, span: Span) -> Option<&Edge> {
    let term = function.block(block)?.terminator.as_ref()?;
    let TerminatorKind::Branch(edge) = &term.kind else {
        return None;
    };
    (edge.target == target && term.origin == Origin::Source(span)).then_some(edge)
}

fn forwarded_loan(function: &Function, edge: &Edge, source: LoanId) -> Option<LoanId> {
    let mut positions = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, entity)| (*entity == EntityId::Loan(source)).then_some(index));
    let index = positions.next()?;
    if positions.next().is_some() {
        return None;
    }
    let EntityId::Loan(loan) = *function.block(edge.target)?.parameters.get(index)? else {
        return None;
    };
    (parameter(function, EntityId::Loan(loan))? == (edge.target, index)).then_some(loan)
}
