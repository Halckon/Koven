//! Synchronous Borrow of one owned remove result; reuse the existing nullable proof ABI.
use super::*;
use crate::ssa::model::{Definition, Edge, InstructionId, SsaTypeKind};
use lang_frontend::{
    ownership_checking::UnitLoanTarget,
    type_checking::{CallableResultSource, Copyability, UnitNullComparisonDescriptor},
};
use std::collections::BTreeSet;

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_borrow_condition(
        &self,
        mut condition: ExpressionId,
        span: Span,
    ) -> Result<Option<UnitNullComparisonDescriptor>, LoweringError> {
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(condition)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            condition = *expression;
        }
        Ok(self
            .typed
            .null_comparison(UnitExpressionId::new(self.source_unit, condition)))
    }

    // This only checks the frozen source-position boundary. All ownership and
    // non-null meaning still comes from typed/loan facts below. In particular,
    // a statement inside an argument block is not a top-level synchronous call:
    // an enclosing adapter may hold an unregistered receiver in a Rust local.
    fn is_direct_map_borrow_statement(
        &self,
        call: UnitExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if call.source_unit() != self.source_unit {
            return Ok(false);
        }
        let nodes = self.parsed.ast().expressions();
        let call_span = nodes
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        if nodes.iter().any(|(id, node)| {
            id != call.expression()
                && node.span().start() <= call_span.start()
                && call_span.end() <= node.span().end()
                && !matches!(
                    node.payload(),
                    Expression::Group { .. } | Expression::If { .. }
                )
        }) {
            return Ok(false);
        }
        for (_, statement) in self.parsed.ast().statements().iter() {
            let Statement::Expression { expression } = statement.payload() else {
                continue;
            };
            let mut expression = *expression;
            while let Expression::Group { expression: inner } = nodes
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                .payload()
            {
                expression = *inner;
            }
            if expression == call.expression() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn lower_owned_map_borrow(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let UnitLoanTarget::Place(root) = loan_target else {
            return Ok(None);
        };
        if !root.is_root() {
            return Ok(None);
        }
        let Some(proof) = self
            .typed
            .non_null_use(UnitExpressionId::new(self.source_unit, argument))
        else {
            return Ok(None);
        };
        let Some(guard) = self.map_borrow_guards.get(&root.root()).copied() else {
            return Ok(None);
        };
        let declared = resolve_concrete_type(
            self.typed,
            proof.declared_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let narrowed = resolve_concrete_type(
            self.typed,
            proof.narrowed_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.copyability(narrowed) == Copyability::Copyable {
            return Ok(None);
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root.root()).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let actual = self
            .function
            .entity(EntityId::Value(owner))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .ty
            .semantic_type();
        if proof.symbol() != root.root()
            || guard.symbol() != root.root()
            || resolve_concrete_type(
                self.typed,
                guard.nullable_type(),
                self.substitutions,
                self.static_self,
                span,
            )? != declared
            || self.type_ids.get(&declared) != Some(&actual)
            || self.type_ids.get(&narrowed) != Some(&target)
            || !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner)) if *inner == narrowed)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.nullable_maps.get(&actual) != Some(&target)
            && self.map_results.get(&actual) != Some(&target)
            && !matches!(self.ssa_types.get(actual.index()), Some(SsaTypeKind::NullableHandle { inner }) if *inner == target)
        {
            return Ok(None);
        }
        let Some(remove) = owned_remove_instruction(self.function, owner) else {
            return Ok(None);
        };
        // Only ordinary synchronous arguments use pending slots before later operands.
        // Map key/receiver adapters and callable prefixes with an unregistered receiver
        // are intentionally outside this slice.
        if !self
            .pending_call_frames
            .last()
            .is_some_and(|frame| frame.call == call && !frame.receiver)
            || self.typed.call(call).is_none_or(|descriptor| {
                descriptor.receiver().is_some()
                    || descriptor.result_source() != CallableResultSource::Owned
            })
            || !self.is_direct_map_borrow_statement(call, span)?
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let iteration =
            self.qualify_map_borrow_iteration(root.root(), owner, remove, narrowed, target, span)?;
        let baseline = self.bindings.clone();
        let mut carried = self.carried_bindings(&baseline, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let null = self.add_carried_control_block(&carried, &loans, span)?;
        let (non_null, view) = self.add_map_borrow_block(&carried, &loans, target, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: cfg::carried_control_edge(null, &carried, &loans),
                    when_non_null: cfg::carried_control_edge(non_null, &carried, &loans),
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        // The typed use and branch guard prove this edge unreachable for valid source.
        // No owner is extracted; Abort is the existing non-unwinding failure boundary.
        self.function
            .set_terminator(null, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.rebind_map_borrow_control(&baseline, non_null, &carried, &loans, view, target, span)?;
        self.block = non_null;
        if let Some(iteration) = iteration {
            self.validate_map_borrow_iteration_transport(iteration, span)?;
        }
        Ok(Some(view))
    }
}

// Follow actual SSA owner transport, not a type match or another remove in the function.
pub(super) fn owned_remove_origin(function: &Function, owner: ValueId) -> bool {
    owned_remove_instruction(function, owner).is_some()
}

fn owned_remove_instruction(function: &Function, owner: ValueId) -> Option<InstructionId> {
    let mut pending = vec![owner];
    let mut seen = BTreeSet::new();
    let mut origin: Option<InstructionId> = None;
    while let Some(value) = pending.pop() {
        if !seen.insert(value) {
            continue;
        }
        let data = function.entity(EntityId::Value(value))?;
        match data.definition {
            Definition::InstructionResult { instruction, index } => {
                let result = function.instruction(instruction)?;
                if index != 1
                    || !matches!(result.operation, Operation::MapRemove { .. })
                    || origin.is_some_and(|prior| prior != instruction)
                {
                    return None;
                }
                origin = Some(instruction);
            }
            Definition::BlockParameter { block, index } => {
                let mut incoming = 0;
                for predecessor in &function.blocks {
                    let Some(terminator) = &predecessor.terminator else {
                        continue;
                    };
                    for edge in edges(&terminator.kind) {
                        if edge.target != block {
                            continue;
                        }
                        let Some(EntityId::Value(value)) = edge.arguments.get(index) else {
                            return None;
                        };
                        incoming += 1;
                        pending.push(*value);
                    }
                }
                if incoming == 0 {
                    return None;
                }
            }
        }
    }
    origin
}

fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}
