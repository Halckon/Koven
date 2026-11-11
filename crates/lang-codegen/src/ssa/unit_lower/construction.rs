//! compilation-unit construction descriptor 与 ordered ownership plan 的共同消费边界。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionDeliveryKind, ConstructionRootKind},
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, IntrinsicTypeConstructor, NominalKind,
        UnitConstructionDescriptor, UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, ValueId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_rc_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.construction_descriptor(expression, span)?;
        if descriptor.target() != UnitConstructionTarget::IntrinsicRc {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let [argument] = descriptor.arguments() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [type_argument] = descriptor.instance().type_arguments() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments: result_arguments,
        }) = self.typed.types().get(descriptor.result_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [result_payload] = result_arguments.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if *type_argument != argument.parameter_type()
            || *result_payload != argument.parameter_type()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(fields) = self.lower_construction_fields(&descriptor, span)? else {
            return Ok(LoweredValue::Diverged);
        };
        let [payload] = fields.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let owner = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedAllocate {
                    owner,
                    payload: *payload,
                },
                vec![EntityType::Value(owner)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn construction_descriptor(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitConstructionDescriptor, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        self.typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    /// 按源码求值顺序 lower operand，再按声明参数顺序返回已经完成 Value delivery 的字段。
    /// 若 operand 已终止，返回 None。已交付的前缀沿实际控制转移边消费清理事实。
    pub(super) fn lower_construction_fields(
        &mut self,
        descriptor: &UnitConstructionDescriptor,
        span: Span,
    ) -> Result<Option<Vec<ValueId>>, LoweringError> {
        let id = descriptor.expression();
        let mut plans = self
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let plan = plans
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plans.next().is_some()
            || plan.target() != descriptor.target()
            || plan.terminating_operand().is_some()
            || self.typed.expression_type(id) != Some(descriptor.result_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_construction_root(descriptor, plan.root_obligation(), span)?;

        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        if arguments.len() != plan.deliveries().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let pending_start = self.pending_operands.len();
        let mut fields = vec![None; arguments.len()];
        for (evaluation_index, (argument, delivery)) in
            arguments.into_iter().zip(plan.deliveries()).enumerate()
        {
            let expected_delivery = match (
                argument.category(),
                self.typed.copyability(argument.parameter_type()),
            ) {
                (ExpressionCategory::Temporary, _) => ConstructionDeliveryKind::DeliverTemporary,
                (ExpressionCategory::Place, Copyability::Copyable) => {
                    ConstructionDeliveryKind::Copy
                }
                (ExpressionCategory::Place, Copyability::MoveOnly) => {
                    ConstructionDeliveryKind::Move
                }
                (ExpressionCategory::Place, Copyability::Unknown | Copyability::Error) => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            };
            if argument.evaluation_index() != evaluation_index
                || delivery.construction() != id
                || delivery.argument() != argument.argument()
                || delivery.parameter_index() != argument.parameter_index()
                || delivery.parameter_symbol() != argument.parameter_symbol()
                || delivery.evaluation_index() != evaluation_index
                || delivery.kind() != expected_delivery
                || self.typed.expression_category(argument.argument()) != Some(argument.category())
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let Some(field) =
                self.lower_optional_expression_value(argument.argument().expression())?
            else {
                self.pending_operands.truncate(pending_start);
                return Ok(None);
            };
            let (mut field, transferred) = self.adapt_owned_value_to_expected(
                argument.argument().expression(),
                field,
                argument.parameter_type(),
                span,
            )?;
            match delivery.kind() {
                ConstructionDeliveryKind::Copy => {
                    if transferred {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    let ty = self.expression_ssa_type(argument.argument().expression(), span)?;
                    let (_, results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::Copy { source: field },
                            vec![EntityType::Value(ty)],
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    field = require_value(results[0], span)?;
                }
                ConstructionDeliveryKind::Move | ConstructionDeliveryKind::DeliverTemporary => {
                    if !transferred {
                        self.transfer_owned_expression(
                            argument.argument().expression(),
                            field,
                            span,
                        )?;
                    }
                }
            }
            self.retain_pending_construction_operand(argument.argument(), field, span)?;
            let pending_index = self.pending_operands.len();
            self.pending_operands.push(EntityId::Value(field));
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(pending_index).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.commit_construction_operands(fields, pending_start, span)
            .map(Some)
    }

    /// The source owner has already been transferred; retain its cleanup identity until commit.
    pub(super) fn retain_pending_construction_operand(
        &mut self,
        argument: UnitExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let template = self
            .typed
            .expression_type(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete = super::resolve_concrete_type(
            self.typed,
            template,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.copyability(concrete) == Copyability::MoveOnly {
            let origin = self
                .constant_materialization_origin(argument, span)?
                .unwrap_or(argument);
            if self.temporaries.insert(origin, value).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }

    /// Commit the current CFG owners, not the values from before later-operand branches.
    pub(super) fn commit_construction_operands(
        &mut self,
        slots: impl IntoIterator<Item = usize>,
        pending_start: usize,
        span: Span,
    ) -> Result<Vec<ValueId>, LoweringError> {
        let values = slots
            .into_iter()
            .map(|index| {
                let entity = self
                    .pending_operands
                    .get(index)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                require_value(entity, span)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.temporaries.retain(|_, value| !values.contains(value));
        self.pending_operands.truncate(pending_start);
        Ok(values)
    }

    fn validate_construction_root(
        &self,
        descriptor: &UnitConstructionDescriptor,
        actual: Option<lang_frontend::ownership_checking::UnitConstructionRootDropObligation>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expected_kind = match descriptor.target() {
            UnitConstructionTarget::IntrinsicBox => ConstructionRootKind::HeapOwner,
            UnitConstructionTarget::IntrinsicRc => ConstructionRootKind::SharedOwner,
            UnitConstructionTarget::EnumCase(_) => ConstructionRootKind::Inline,
            UnitConstructionTarget::Nominal(declaration) => self
                .typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
        };
        match (self.typed.copyability(descriptor.result_type()), actual) {
            (Copyability::Copyable, None) => Ok(()),
            (Copyability::MoveOnly, Some(root))
                if root.construction() == descriptor.expression()
                    && root.result_type() == descriptor.result_type()
                    && root.kind() == expected_kind =>
            {
                Ok(())
            }
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
}

pub(super) mod nullable_proof {
    //! Private proof identity, distinct from the nullable root's owned storage.
    use super::super::*;
    use crate::ssa::model::SsaTypeKind;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in super::super) enum NullableProofScope {
        If(UnitExpressionId),
        When(super::super::nullable_when::NullableWhenScope),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in super::super) struct NullableProofOrigin {
        scope: NullableProofScope,
        birth: LoanId,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in super::super) struct NullableProof {
        pub(in super::super) loan: LoanId,
        pub(in super::super) owner: ValueId,
        pub(in super::super) inner: SsaTypeId,
        pub(in super::super) origin: NullableProofOrigin,
    }

    impl NullableProof {
        pub(in super::super) fn if_scope(self) -> Option<UnitExpressionId> {
            match self.origin.scope {
                NullableProofScope::If(scope) => Some(scope),
                NullableProofScope::When(_) => None,
            }
        }

        pub(in super::super) fn when_scope(
            self,
        ) -> Option<super::super::nullable_when::NullableWhenScope> {
            match self.origin.scope {
                NullableProofScope::When(scope) => Some(scope),
                NullableProofScope::If(_) => None,
            }
        }
    }

    pub(in super::super) type NullableProofs = BTreeMap<UnitSymbolId, NullableProof>;

    impl UnitExpressionLowerer<'_> {
        /// Entry qualification is minted from actual entry Shared(Boolean) parameters.
        /// The existing named access slots keep that root identity across CFG rebinding.
        pub(in super::super) fn ensure_no_while_proof_carriers(
            &self,
            span: Span,
        ) -> Result<(), LoweringError> {
            if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.ensure_while_proof_non_call_carriers(span)
        }

        pub(in super::super) fn ensure_while_proof_non_call_carriers(
            &self,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self.function.borrow_return.is_some()
                || self.function.carrier_return.is_some()
                || self.current_receiver.is_some()
                || self.has_iteration_context()
                || self.borrow_bindings.iter().any(|(root, &loan)| {
                    let Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }) = self
                        .function
                        .entity(EntityId::Loan(loan))
                        .map(|data| data.ty)
                    else {
                        return true;
                    };
                    !(self.entry_boolean_roots.contains(root)
                        && self.ssa_types.get(target.index())
                            == Some(&crate::ssa::model::SsaTypeKind::Boolean))
                })
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }

        pub(in super::super) fn end_nullable_proof(
            &mut self,
            root: UnitSymbolId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(proof) = self.nullable_proofs.get(&root).copied() else {
                return Ok(());
            };
            self.validate_nullable_proof_retirement(root, span)?;
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan: proof.loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            self.nullable_proofs.remove(&root);
            Ok(())
        }

        pub(in super::super) fn validate_nullable_proof_retirement(
            &self,
            root: UnitSymbolId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let proof = self
                .nullable_proofs
                .get(&root)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if proof.when_scope().is_some() {
                return self.validate_nullable_when_retirement(root, span);
            }
            // All checks precede both BorrowEnd and state removal, in particular an owner Drop.
            self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
            self.ensure_no_while_proof_carriers(span)?;
            if self
                .nullable_proofs
                .iter()
                .any(|(other, candidate)| *other != root && candidate.loan == proof.loan)
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }

        /// If retests currently support only local reads and normal branch completion.
        pub(in super::super) fn reject_live_if_proof(
            &self,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self
                .nullable_proofs
                .values()
                .any(|proof| proof.if_scope().is_some())
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }

        /// Every scope uses the same actual non-null successor and owner-column check.
        pub(in super::super) fn introduce_nullable_proof(
            &mut self,
            header: BlockId,
            owner_column: usize,
            root: UnitSymbolId,
            inner: SsaTypeId,
            scope: NullableProofScope,
            span: Span,
        ) -> Result<(), LoweringError> {
            let (body_owner, view) =
                self.nullable_branch_view_owner(header, owner_column, inner, span)?;
            if self
                .nullable_proofs
                .values()
                .any(|outer| outer.loan == view || outer.origin.scope == scope)
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let proof = NullableProof {
                loan: view,
                owner: body_owner,
                inner,
                origin: NullableProofOrigin { scope, birth: view },
            };
            // Keep only the actual rebound outer candidate validated above. Never
            // reinstall the qualification's preheader owner/loan IDs at this birth.
            let mut proofs = self.nullable_proofs.clone();
            if proofs.insert(root, proof).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_nullable_proofs(&self.bindings, &proofs, span)?;
            self.nullable_proofs = proofs;
            Ok(())
        }

        /// A temporary subject uses this same birth check without inventing a symbol.
        pub(in super::super) fn nullable_branch_view_owner(
            &self,
            header: BlockId,
            owner_column: usize,
            inner: SsaTypeId,
            span: Span,
        ) -> Result<(ValueId, LoanId), LoweringError> {
            let Some(TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            }) = self
                .function
                .block(header)
                .and_then(|block| block.terminator.as_ref())
                .map(|terminator| &terminator.kind)
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let parameters = &self
                .function
                .block(self.block)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                .parameters;
            let Some(EntityId::Value(body_owner)) = parameters.get(owner_column) else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if when_non_null.target != self.block
                || when_null.target == self.block
                || when_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
                || when_non_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
                || parameters.last() != Some(&EntityId::Loan(*view))
                || parameters.len() != when_non_null.arguments.len() + 1
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            if self
                .function
                .entity(EntityId::Loan(*view))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Ok((*body_owner, *view))
        }

        /// Check the current storage identity, not merely two compatible semantic types.
        /// Producers and transport validate provenance before installing these records.
        pub(in super::super) fn validate_nullable_proofs(
            &self,
            bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
            proofs: &NullableProofs,
            span: Span,
        ) -> Result<(), LoweringError> {
            let mut identities = BTreeMap::new();
            for (root, proof) in proofs {
                let owner_type = self
                    .function
                    .entity(EntityId::Value(proof.owner))
                    .map(|entity| entity.ty);
                let valid_storage = matches!(owner_type, Some(EntityType::Value(storage))
                    if matches!(self.ssa_types.get(storage.index()), Some(SsaTypeKind::NullableHandle { inner })
                        if *inner == proof.inner));
                let identity = (proof.owner, proof.inner);
                if identities
                    .insert(proof.loan, identity)
                    .is_some_and(|old| old != identity)
                    || root.source_unit() != self.source_unit
                    || match proof.origin.scope {
                        NullableProofScope::If(scope) => scope.source_unit() != self.source_unit
                            || !self.parsed.ast().expressions().get(scope.expression())
                                .is_ok_and(|node| matches!(node.payload(), Expression::If { .. })),
                        NullableProofScope::When(scope) => {
                            !self.valid_nullable_when_scope(*root, scope, proof.inner)
                        }
                    }
                    || !self.function.blocks.iter().any(|block| {
                        matches!(block.terminator.as_ref().map(|terminator| &terminator.kind),
                            Some(TerminatorKind::NullableBranch { view, .. }) if *view == proof.origin.birth)
                    })
                    || bindings.get(root) != Some(&LoweredValue::Value(proof.owner))
                    || !valid_storage
                    || self
                        .function
                        .entity(EntityId::Loan(proof.loan))
                        .map(|entity| entity.ty)
                        != Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: proof.inner,
                        })
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            Ok(())
        }
    }
}
