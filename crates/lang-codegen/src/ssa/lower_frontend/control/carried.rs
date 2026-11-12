//! Construct and restore explicit linear CFG storage slots, and retire while views on non-common if exits.
use super::*;
use crate::ssa::carry::{
    self, AliasRole, AliasSlot, CaptureLoan, ResultSourceLoan, SourceSlot, Temporary, TypeAtMiss,
};
use crate::ssa::model::LoanId;
use std::collections::BTreeSet;

impl SourceSlot for LinearBindingSlot {
    type Source = EntityId;

    fn source(&self) -> EntityId {
        self.source
    }

    fn entity(source: EntityId) -> EntityId {
        source
    }

    fn ty(&self) -> EntityType {
        self.ty
    }

    fn empty(source: EntityId, ty: EntityType) -> Self {
        Self::new(None, source, ty)
    }
}

impl AliasSlot<Temporary, usize> for LinearBindingSlot {
    fn alias_source(source: ValueId) -> EntityId {
        Temporary::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<usize> {
        &mut self.temporaries
    }
}

impl AliasSlot<CaptureLoan, (usize, usize)> for LinearBindingSlot {
    fn alias_source(source: LoanId) -> EntityId {
        CaptureLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(usize, usize)> {
        &mut self.captures
    }
}

impl AliasSlot<ResultSourceLoan, (SymbolId, usize)> for LinearBindingSlot {
    fn alias_source(source: LoanId) -> EntityId {
        ResultSourceLoan::entity(source)
    }

    fn keys_mut(&mut self) -> &mut Vec<(SymbolId, usize)> {
        &mut self.result_sources
    }
}

impl ExpressionLowerer<'_> {
    pub(in super::super) fn linear_binding_slots(
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
            carry::seed_slot(
                &mut carried,
                LinearBindingSlot::new(Some(symbol), EntityId::Value(source), ty),
            );
        }
        carry::collect_aliases::<Temporary, _, _>(
            self.function,
            self.temporaries.iter().map(|(&key, &id)| (key, id)),
            &mut carried,
            error(LoweringErrorKind::InvalidModel, span),
        )?;
        for (&key, &loan) in self.pending_call_loans.iter() {
            let Some(loan) = loan else {
                continue;
            };
            let source = EntityId::Loan(loan);
            let choice = carry::associate_source(
                self.function,
                &mut carried,
                source,
                TypeAtMiss::Query,
                error(LoweringErrorKind::InvalidModel, span),
            )?;
            carried[choice.column().index()].loans.push(key);
        }
        carry::collect_aliases::<CaptureLoan, _, _>(
            self.function,
            self.capture_loans.iter().map(|(&key, &id)| (key, id)),
            &mut carried,
            error(LoweringErrorKind::InvalidModel, span),
        )?;
        for (&symbol, &proof) in &self.non_null_bindings {
            self.validate_non_null_proof(symbol, proof, bindings, span)?;
            let source = EntityId::Loan(proof.loan);
            let choice = carry::associate_source(
                self.function,
                &mut carried,
                source,
                TypeAtMiss::Query,
                error(LoweringErrorKind::InvalidModel, span),
            )?;
            carried[choice.column().index()].views.push((symbol, proof));
        }
        for (&symbol, &loan) in &self.borrow_bindings {
            if self.is_entry_loan(loan) {
                continue;
            }
            let source = EntityId::Loan(loan);
            let choice = carry::associate_source(
                self.function,
                &mut carried,
                source,
                TypeAtMiss::Query,
                error(LoweringErrorKind::InvalidModel, span),
            )?;
            carried[choice.column().index()].borrow_symbols.push(symbol);
        }
        carry::collect_aliases::<ResultSourceLoan, _, _>(
            self.function,
            carry::indexed_aliases(&self.result_source_loans),
            &mut carried,
            error(LoweringErrorKind::MissingFact, span),
        )?;
        for context in &self.loops {
            let Some(ref for_data) = context.for_loop else {
                continue;
            };
            let stmt = for_data.statement.index();
            let body_source_loan = for_data.body_source;
            if !self.is_entry_loan(body_source_loan) {
                let source = EntityId::Loan(body_source_loan);
                let choice = carry::associate_source(
                    self.function,
                    &mut carried,
                    source,
                    TypeAtMiss::Query,
                    error(LoweringErrorKind::InvalidModel, span),
                )?;
                carried[choice.column().index()].for_sources.push(stmt);
            }
            let element_loan = for_data.guarded_element.loan();
            if !self.is_entry_loan(element_loan) {
                let source = EntityId::Loan(element_loan);
                let choice = carry::associate_source(
                    self.function,
                    &mut carried,
                    source,
                    TypeAtMiss::Query,
                    error(LoweringErrorKind::InvalidModel, span),
                )?;
                carried[choice.column().index()].for_elements.push(stmt);
            }
        }
        let map_paths = self.map_path_snapshots();
        self.carry_map_paths(&map_paths, bindings, &mut carried, span)?;
        self.bind_pending_field_columns(bindings, &mut carried, span)?;
        Ok(LinearBindings {
            slots: carried,
            map_paths,
            forwarded_loans: self
                .pending_call_loans
                .iter()
                .filter_map(|(&key, loan)| loan.is_none().then_some(key))
                .collect(),
        })
    }

    pub(in super::super) fn add_linear_binding_block(
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

    pub(in super::super) fn rebind_linear_bindings(
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
        self.capture_loans.clear();
        self.non_null_bindings.clear();
        let entry_loans = self.entry_loans();
        self.borrow_bindings
            .retain(|_, loan| entry_loans.contains(loan));
        self.pending_call_loans = carried
            .forwarded_loans
            .iter()
            .map(|&key| (key, None))
            .collect();
        self.result_source_loans.clear();
        let mut bindings = baseline.clone();
        let mut owners = BTreeMap::new();
        // Finish the complete owner mapping before rebuilding any proof pair.
        for (slot, &parameter) in carried.slots.iter().zip(parameters) {
            if let Some(symbol) = slot.symbol {
                let (EntityId::Value(source), EntityId::Value(owner)) = (slot.source, parameter)
                else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                if owners.insert(symbol, (source, owner)).is_some() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                bindings.insert(symbol, LoweredValue::Value(owner));
            }
        }
        for (slot, &parameter) in carried.slots.iter().zip(parameters) {
            if !slot.temporaries.is_empty() {
                carry::apply_alias_column(
                    &slot.temporaries,
                    value(parameter),
                    &mut self.temporaries,
                );
            }
            if !slot.captures.is_empty() {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                carry::apply_alias_column(&slot.captures, loan, &mut self.capture_loans);
            }
            for &(symbol, source_proof) in &slot.views {
                self.validate_non_null_proof(symbol, source_proof, baseline, span)?;
                if slot.source != EntityId::Loan(source_proof.loan) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let (source_owner, owner) = owners
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if source_owner != source_proof.owner {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let proof = NonNullProof {
                    loan,
                    owner,
                    ..source_proof
                };
                self.validate_non_null_proof(symbol, proof, &bindings, span)?;
                self.non_null_bindings.insert(symbol, proof);
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
            if !slot.result_sources.is_empty() {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                carry::apply_indexed_alias_column(
                    &slot.result_sources,
                    loan,
                    &mut self.result_source_loans,
                );
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
        let parameters = parameters.clone();
        self.restore_pending_field_columns(&bindings, &carried.slots, &parameters, span)?;
        self.rebind_map_paths(
            &carried.map_paths,
            &bindings,
            &carried.slots,
            &parameters,
            block,
            span,
        )?;
        Ok(bindings)
    }

    pub(super) fn retire_non_common_if_while_proofs(
        &mut self,
        exits: &mut [BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        if exits.len() < 2 {
            return Ok(());
        }
        let roots = exits
            .iter()
            .flat_map(|exit| exit.views.keys().copied())
            .collect::<BTreeSet<_>>();
        let mut ending = vec![Vec::new(); exits.len()];
        for root in roots {
            let present = exits
                .iter()
                .enumerate()
                .filter_map(|(index, exit)| exit.views.get(&root).map(|proof| (index, *proof)))
                .collect::<Vec<_>>();
            let first = present[0].1;
            for &(index, proof) in &present {
                self.validate_non_null_proof(root, proof, &exits[index].bindings, span)?;
                if proof.origin != first.origin || proof.inner != first.inner {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            if present.len() == exits.len() {
                continue;
            }
            for (index, proof) in present {
                // Other proof scopes retain their original strict merge contract.
                if !proof.is_while_origin() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                ending[index].push(root);
            }
        }
        // Validate every actual exit before appending any BorrowEnd. In particular,
        // absence on one path is not evidence that a child on another path ended.
        for (exit, roots) in exits.iter().zip(&ending) {
            if roots.is_empty() {
                continue;
            }
            if !exit.for_sources.is_empty()
                || !exit.for_elements.is_empty()
                || self
                    .function
                    .block(exit.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.restore_if_proof_exit(exit);
            self.ensure_no_unresolved_while_proof_carriers(span)?;
            for root in roots {
                self.validate_non_null_proof(
                    *root,
                    self.non_null_bindings[root],
                    &self.bindings,
                    span,
                )?;
            }
        }
        for (exit, roots) in exits.iter_mut().zip(ending) {
            if roots.is_empty() {
                continue;
            }
            self.restore_if_proof_exit(exit);
            for root in roots {
                self.end_non_null_proof(root, span)?;
            }
            // Owner and all other exit records remain exactly as published by the
            // branch. The strict merge consumes only these surviving views.
            exit.views.clone_from(&self.non_null_bindings);
        }
        Ok(())
    }

    fn restore_if_proof_exit(&mut self, exit: &BranchExit) {
        self.block = exit.block;
        self.bindings.clone_from(&exit.bindings);
        self.temporaries.clone_from(&exit.temporaries);
        self.pending_call_loans.clone_from(&exit.loans);
        self.capture_loans.clone_from(&exit.capture_loans);
        self.non_null_bindings.clone_from(&exit.views);
        self.borrow_bindings.clone_from(&exit.borrow_bindings);
        self.result_source_loans
            .clone_from(&exit.result_source_loans);
    }
}

mod enum_type_test {
    //! 普通 enum 类型测试与无 payload Copyable enum case 条件只比较已登记 case tag。

    use lang_frontend::{
        ast::ExpressionId,
        ownership_checking::{DropPoint, DropTarget},
        parser::{Expression, TypeRef},
        source::Span,
        type_checking::{
            BuiltinType, ConstructionTarget, Copyability, ExpressionCategory, NominalKind, TypeKind,
        },
    };

    use crate::ssa::lower_frontend::{
        ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
        span_key, value,
    };
    use crate::ssa::model::{
        ComparisonOperator, EntityId, EntityType, Operation, ScalarConstant, SsaTypeId,
        SsaTypeKind, ValueId,
    };

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa::lower_frontend) fn lower_copyable_enum_case_condition(
            &mut self,
            owner: ValueId,
            tagged: SsaTypeId,
            expression: ExpressionId,
        ) -> Result<ValueId, LoweringError> {
            let span = self.expression_span(expression)?;
            let descriptor = self
                .typed
                .construction(expression)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let ConstructionTarget::EnumCase(case_id) = descriptor.target() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if !descriptor.arguments().is_empty()
                || self.typed.copyability(descriptor.result_type()) != Some(Copyability::Copyable)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let case = self
                .typed
                .enum_cases()
                .iter()
                .find(|case| case.id() == case_id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if !case.payloads().is_empty() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if descriptor.expression() != expression
                || self.typed.expression_type(expression) != Some(descriptor.result_type())
                || !matches!(self.typed.types().get(descriptor.result_type()),
                    Some(TypeKind::Nominal { nominal, .. }) if *nominal == case.root())
                || self.expression_ssa_type(expression, span)? != tagged
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let plan = self
                .owned
                .construction_plan(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if plan.construction() != expression
                || plan.target() != descriptor.target()
                || !plan.deliveries().is_empty()
                || plan.terminating_operand().is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_construction_root(
                descriptor.result_type(),
                descriptor.target(),
                plan,
                span,
            )?;
            let (variant, _) = self
                .enum_payloads
                .get(&(tagged, case_id))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let integer = self.ssa_builtin(BuiltinType::Int, span)?;
            let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
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
            let (_, result) = self.append(
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: value(tag[0]),
                    right: value(expected[0]),
                },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            Ok(value(result[0]))
        }

        pub(in crate::ssa::lower_frontend) fn lower_enum_type_test(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::TypeTest {
                expression: operand,
                type_ref,
                negated,
                ..
            } = *node.payload()
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let result_type = self
                .typed
                .expression_type(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if builtin_type(self.typed, result_type) != Some(BuiltinType::Boolean)
                || self.expression_ssa_type(expression, span)?
                    != self.ssa_builtin(BuiltinType::Boolean, span)?
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }

            let operand_type = self
                .typed
                .expression_type(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let operand_root = match self.typed.types().get(operand_type) {
                Some(TypeKind::EnumCase { root, .. }) => *root,
                _ => operand_type,
            };
            let operand_root = self.resolve_type(operand_root, span)?;
            let Some(TypeKind::Nominal { nominal, .. }) = self.typed.types().get(operand_root)
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if !self.typed.nominals().iter().any(|descriptor| {
                descriptor.id() == *nominal && descriptor.kind() == NominalKind::EnumClass
            }) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let tagged = self
                .type_ids
                .get(&operand_root)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(
                self.ssa_types.get(tagged.index()),
                Some(SsaTypeKind::TaggedUnion { .. })
            ) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }

            let target = self
                .typed
                .type_ref_type(type_ref)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(TypeKind::EnumCase { case, root }) = self.typed.types().get(target) else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let target_root = self.resolve_type(*root, span)?;
            if target_root != operand_root || self.type_ids.get(&target_root) != Some(&tagged) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let case = self
                .typed
                .enum_cases()
                .iter()
                .find(|descriptor| descriptor.id() == *case && descriptor.root() == *nominal)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let type_node = self
                .parsed
                .ast()
                .type_refs()
                .get(type_ref)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let TypeRef::Qualified { segments, .. } = type_node.payload() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, type_node.span()));
            };
            let symbol = segments
                .last()
                .and_then(|segment| self.references.get(&span_key(segment.name_span)))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, type_node.span()))?;
            if symbol != case.type_symbol() {
                return Err(error(LoweringErrorKind::MissingFact, type_node.span()));
            }
            let (variant, _) = self
                .enum_payloads
                .get(&(tagged, case.id()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, type_node.span()))?;

            // The final Read of a named owner can have an ASAP drop on the leaf.
            // Evaluate once, retaining transparent aliases, and defer that cleanup
            // until the discriminator has read the owner.
            let mut operands = vec![operand];
            let mut evaluated = operand;
            loop {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(evaluated)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                let Expression::Group { expression } = *node.payload() else {
                    break;
                };
                evaluated = expression;
                operands.push(evaluated);
            }
            // TypeTest currently publishes named Read cleanup, not a general
            // temporary-result cleanup plan. Never invent the missing drop.
            let temporary = self.typed.copyability(operand_root) == Some(Copyability::MoveOnly)
                && (self.typed.expression_category(evaluated)
                    == Some(ExpressionCategory::Temporary)
                    || self.typed.construction(evaluated).is_some());
            if temporary
                && !self.owned.drops().iter().any(|fact| {
                    let DropTarget::Temporary(target) = fact.target() else {
                        return false;
                    };
                    let DropPoint::AfterExpression(point) = fact.point() else {
                        return false;
                    };
                    operands.contains(&target) && (point == expression || operands.contains(&point))
                })
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let owner = match self.lower_expression(evaluated)? {
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Value(value) => value,
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            for &operand in operands.iter().rev() {
                if self.typed.expression_category(operand) == Some(ExpressionCategory::Temporary)
                    && self
                        .typed
                        .expression_type(operand)
                        .and_then(|ty| self.typed.copyability(ty))
                        == Some(Copyability::MoveOnly)
                {
                    self.temporaries.insert(operand.index(), owner);
                }
            }
            if self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(tagged))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let result = self.lower_enum_tag_test(owner, variant, negated, span)?;
            for operand in operands.into_iter().rev() {
                self.emit_drops(DropPoint::AfterExpression(operand))?;
            }
            Ok(LoweredValue::Value(result))
        }

        pub(in crate::ssa::lower_frontend) fn lower_enum_tag_test(
            &mut self,
            owner: ValueId,
            variant: usize,
            negated: bool,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            let integer = self.ssa_builtin(BuiltinType::Int, span)?;
            let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
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
            Ok(value(result[0]))
        }
    }
}
