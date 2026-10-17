//! Bounded nullable entries: same-root While retests and direct Map-size If statements.
use super::super::unwrap_modified;
use super::super::{ExpressionCategory, Statement, nullable_proof::NullableProofScope, span_key};
use super::*;
use crate::ssa::model::{LoanId, SsaTypeKind};
use lang_frontend::{
    parser::{BinaryOperator, FunctionBody, FunctionForm, Item},
    type_checking::{IntrinsicTypeConstructor, UnitCallableTarget, UnitTypeKind},
};

#[derive(Clone, Copy)]
pub(super) struct NullableIfEntry {
    pub(super) root: UnitSymbolId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) scope: UnitExpressionId,
    pub(super) non_null_when_true: bool,
}

impl UnitExpressionLowerer<'_> {
    /// Existing standalone ifs and existing live while views keep their old path.
    pub(super) fn prepare_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        result_required: bool,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        if result_required || !self.nullable_proofs.is_empty() {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::If { condition: source, else_branch: None, .. }
            if *source == condition)
        {
            return Ok(None);
        }
        let Some(while_scope) = self.current_while_proof_scope() else {
            return Ok(None);
        };
        if control.source_unit() != self.source_unit
            || while_scope.source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let while_node = self
            .parsed
            .ast()
            .statements()
            .get(while_scope.statement())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::While {
            condition: while_condition,
            ..
        } = while_node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some((outer, _)) = self.direct_nullable_while_condition(*while_condition, span)? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some((descriptor, mut operand)) =
            self.direct_nullable_while_condition(condition, span)?
        else {
            return Ok(None);
        };
        if descriptor.symbol() != outer.symbol() {
            return Ok(None);
        }
        // Even the empty source-call-frame exception belongs only to While.
        if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (_, inner) = self.direct_nullable_while_entry(descriptor, span)?;
        let declared = descriptor.nullable_type();
        if self.typed.copyability(resolve_concrete_type(
            self.typed,
            declared,
            self.substitutions,
            self.static_self,
            span,
        )?) != Copyability::MoveOnly
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if operand.source_unit() != self.source_unit
                || self.typed.expression_type(operand) != Some(declared)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(operand).is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression)
                }
                Expression::Name
                    if self.references.get(&span_key(node.span()))
                        == Some(&descriptor.symbol()) =>
                {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        self.ensure_no_while_proof_carriers(span)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&descriptor.symbol()).copied()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        Ok(Some(NullableIfEntry {
            root: descriptor.symbol(),
            owner,
            inner,
            scope: control,
            non_null_when_true: true,
        }))
    }

    /// This Map-only ordinary If entry is separate from the existing While retest.
    pub(super) fn prepare_map_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        result_required: bool,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        if result_required || !self.nullable_proofs.is_empty() {
            return Ok(None);
        }
        let Some(descriptor) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let storage = descriptor.nullable_type();
        if !matches!(self.typed.types().get(storage), Some(UnitTypeKind::Nullable(inner))
        if matches!(self.typed.types().get(*inner), Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap, ..
        }))) {
            return Ok(None);
        }
        if !self.is_direct_function_body_map_if(control, span)? {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            condition: actual,
            then_branch,
            else_branch,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if actual != condition {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let comparison = self.ungroup_map_if_expression(condition, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(comparison)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let polarity = match operator {
            BinaryOperator::NotEqual => true,
            BinaryOperator::Equal => false,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let is_null = |operand| -> Result<bool, LoweringError> {
            let operand = self.ungroup_map_if_expression(operand, span)?;
            Ok(self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .is_ok_and(|node| matches!(node.payload(), Expression::Literal(LiteralKind::Null))))
        };
        let mut subject = match (is_null(left)?, is_null(right)?) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let root = descriptor.symbol();
        if control.source_unit() != self.source_unit
            || root.source_unit() != self.source_unit
            || descriptor.expression() != UnitExpressionId::new(self.source_unit, comparison)
            || descriptor.non_null_when_true() != polarity
            || self.typed.symbol_type(root) != Some(storage)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let id = UnitExpressionId::new(self.source_unit, subject);
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(subject)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(id) != Some(storage)
                || self.typed.expression_category(id) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(id).is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => subject = *expression,
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        let concrete = resolve_concrete_type(
            self.typed,
            storage,
            self.substitutions,
            self.static_self,
            span,
        )?;
        let nullable = *self
            .type_ids
            .get(&concrete)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let inner = *self
            .nullable_maps
            .get(&nullable)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.typed.copyability(concrete) != Copyability::MoveOnly
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::MapContainer { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (non_null_body, null_body) = if descriptor.non_null_when_true() {
            (Some(then_branch), else_branch)
        } else {
            (else_branch, Some(then_branch))
        };
        let Some(non_null_body) = non_null_body else {
            return Ok(None);
        };
        if !self.map_if_has_only_size_reads(non_null_body, root, nullable, inner, span)?
            || !self.map_if_body_is_empty(null_body, span)?
        {
            return Ok(None);
        }
        // Do not inherit the While-only empty-call-frame or Boolean-provider exceptions.
        self.ensure_no_while_proof_carriers(span)?;
        if !self.pending_call_frames.is_empty()
            || !self.pending_operands.is_empty()
            || !self.temporaries.is_empty()
            || !self.borrow_bindings.is_empty()
            || !self.closure_bindings.is_empty()
            || self.closure_binding_context
            || self.thunk_expression.is_some()
            || self.consumed_receiver.is_some()
            || !self.loops.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|data| data.ty)
            != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(NullableIfEntry {
            root,
            owner,
            inner,
            scope: control,
            non_null_when_true: descriptor.non_null_when_true(),
        }))
    }

    /// Follow the actual current source instance to its own top-level block elements.
    fn is_direct_function_body_map_if(
        &self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let key = self
            .source_plan
            .source(self.source_token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.function_ids.get(key) != Some(&self.function.id) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if key.deinit_owner().is_some() || self.thunk_expression.is_some() {
            return Ok(false);
        }
        let UnitCallableTarget::Declaration(target) = key.target() else {
            return Ok(false);
        };
        let index = self.names.names().index();
        let declaration = index
            .declarations()
            .get(target.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let source = index
            .source_units()
            .get(self.source_unit.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if control.source_unit() != self.source_unit
            || declaration.id() != target
            || declaration.source_unit() != self.source_unit
            || source.id() != self.source_unit
            || source.source_id() != self.parsed.source_id()
            || declaration.name_span().source_id() != self.parsed.source_id()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, item, _) = unwrap_modified(self.parsed, declaration.root())?;
        let Item::Function { form, .. } = item else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => body,
            _ => return Ok(false),
        };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let mut hits = 0;
        for &statement in elements {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Statement::Expression { expression } = node.payload()
                && self.ungroup_map_if_expression(*expression, span)? == control.expression()
            {
                hits += 1;
            }
        }
        if hits > 1 {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(hits == 1)
    }

    fn ungroup_map_if_expression(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                _ => return Ok(expression),
            }
        }
    }

    fn map_if_body_is_empty(
        &self,
        body: Option<StatementId>,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(body) = body else { return Ok(true) };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(matches!(node.payload(), Statement::ControlBody { elements } if elements.is_empty()))
    }

    fn map_if_has_only_size_reads(
        &self,
        body: StatementId,
        root: UnitSymbolId,
        nullable: SsaTypeId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = node.payload() else {
            return Ok(false);
        };
        if elements.is_empty() {
            return Ok(false);
        }
        for &statement in elements {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Statement::LocalVariable { declaration } = node.payload() else {
                return Ok(false);
            };
            let (_, item, _) = unwrap_modified(self.parsed, *declaration)?;
            let Item::Variable { initializer, .. } = item else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let read = self.ungroup_map_if_expression(initializer, span)?;
            let id = UnitExpressionId::new(self.source_unit, read);
            let Some(descriptor) = self.typed.map_size(id) else {
                return Ok(false);
            };
            if self.qualified_nullable_map_size_read(id, descriptor, span)?
                != Some((root, nullable, inner))
                || self.expression_builtin_type(read, span)? != Some(BuiltinType::Int)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter_nullable_if_branch(
        &mut self,
        entry: NullableIfEntry,
        header: BlockId,
        block: BlockId,
        view: LoanId,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        carried: &[super::super::cfg::CarriedBinding],
        loans: &[super::super::cfg::CarriedAccess],
        condition: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.block = block;
        self.rebind_map_borrow_control(baseline, block, carried, loans, view, entry.inner, span)?;
        let column = self.nullable_owner_column(carried, entry.root, entry.owner, span)?;
        self.introduce_nullable_proof(
            header,
            column,
            entry.root,
            entry.inner,
            NullableProofScope::If(entry.scope),
            span,
        )?;
        self.emit_direct_nullable_condition_drops(condition, span)
    }

    /// Preflight every retirement against its own complete live exit before emission.
    /// Same-root origin disagreements remain errors, even if another root is absent.
    pub(super) fn retire_if_exit_proofs(
        &mut self,
        control: UnitExpressionId,
        exits: &mut [BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut identities = BTreeMap::new();
        for exit in exits.iter() {
            self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)?;
            for (&root, proof) in &exit.nullable_proofs {
                if identities
                    .insert(root, (proof.origin, proof.inner))
                    .is_some_and(|previous| previous != (proof.origin, proof.inner))
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
        }
        let retirements = exits
            .iter()
            .map(|exit| {
                exit.nullable_proofs
                    .iter()
                    .filter_map(|(&root, proof)| {
                        (proof.if_scope() == Some(control)
                            || exits
                                .iter()
                                .any(|other| !other.nullable_proofs.contains_key(&root)))
                        .then_some(root)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (exit, roots) in exits.iter().zip(&retirements) {
            if !roots.is_empty()
                && self
                    .function
                    .block(exit.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            for root in roots {
                let proof = exit
                    .nullable_proofs
                    .get(root)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if proof.while_scope().is_none() && proof.if_scope() != Some(control) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            self.restore_loop_state(exit);
            for &root in roots {
                self.validate_nullable_proof_retirement(root, span)?;
            }
        }
        for (exit, roots) in exits.iter_mut().zip(retirements) {
            self.restore_loop_state(exit);
            for root in roots {
                self.end_nullable_proof(root, span)?;
            }
            // BorrowEnd changes only the proof map. Owners and all pending state stay exact.
            exit.nullable_proofs.clone_from(&self.nullable_proofs);
        }
        Ok(())
    }
}
