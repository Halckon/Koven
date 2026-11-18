//! Nullable subjects reuse the ordinary When CFG and existing nullable SSA ABI.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::Expression,
    source::Span,
    type_checking::{UnitExpressionId, UnitTypeId},
};

use super::{Builder, Lowered, View, WhenSubject};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::{
        body::{BlockRef, ROp, RTerm, Slot},
        nullable_when::{self, CopyableWhenSelector},
    },
};

impl Builder<'_, '_> {
    pub(super) fn nullable_when_subject(
        &mut self,
        control: UnitExpressionId,
    ) -> Result<Option<WhenSubject>, LoweringError> {
        let inputs = self.inputs;
        let source = nullable_when::source(inputs, control)?;
        let descriptor = source.plan.descriptor();
        let span = self.span(control.expression());
        let leaf = source
            .operands
            .last()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if source.copyable_scalar {
            let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
            return Ok(self
                .required(descriptor.subject().expression())?
                .map(|owner| WhenSubject::CopyableNullable {
                    owner,
                    inner,
                    symbol: descriptor.stable_symbol(),
                    payload_on_miss: None,
                }));
        }
        let nullable = self.ssa_type(self.c(descriptor.nullable_type()), span)?;
        let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
        // The shared subject is a transparent reference to the original loan slot.
        // Owned subjects evaluate their leaf once; both successors consume its drop points.
        let (owner, expected) = if let Some(kind) = source.loan_kind {
            let root = descriptor
                .stable_symbol()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let owner = self
                .borrowed
                .get(&root)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            (
                owner,
                EntityType::Loan {
                    kind,
                    target: nullable,
                },
            )
        } else {
            let owner = match self.expression_inner(leaf.expression())? {
                Lowered::Value(owner) => owner,
                Lowered::Diverged => return Ok(None),
                Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            (owner, EntityType::Value(nullable))
        };
        if self.body.slots.get(owner.0) != Some(&expected) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(root) = descriptor.stable_symbol() {
            if (source.loan_kind.is_none()
                && (self.borrowed.contains_key(&root) || self.bindings.get(&root) != Some(&owner)))
                || self.active_view(root).is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        } else {
            for operand in &source.operands {
                if self
                    .temporaries
                    .get(operand)
                    .is_some_and(|&actual| actual != owner)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                self.temporaries.insert(*operand, owner);
            }
        }
        let proof =
            (!nullable_when::owned_string_presence_only(inputs.typed, source.plan)).then(|| {
                self.body.slot(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
            });
        Ok(Some(WhenSubject::Nullable {
            control,
            owner,
            proof,
        }))
    }

    pub(super) fn nullable_boolean_when_subject(
        &mut self,
        subject: ExpressionId,
        inner: UnitTypeId,
    ) -> Result<Option<WhenSubject>, LoweringError> {
        let leaf = self.ungroup_when(subject)?;
        let (leaf, span) = self.node(leaf)?;
        let symbol = if matches!(leaf, Expression::Name) {
            self.reference(span)
                .ok()
                .filter(|root| self.bindings.contains_key(root) || self.borrowed.contains_key(root))
        } else {
            None
        };
        let inner = self.ssa_type(inner, span)?;
        Ok(self
            .required(subject)?
            .map(|owner| WhenSubject::CopyableNullable {
                owner,
                inner,
                symbol,
                payload_on_miss: None,
            }))
    }

    /// Copy the payload on the non-null edge of the already evaluated subject.
    /// Only a missed null selector carries that exact payload to later selectors.
    pub(super) fn copyable_nullable_when_branch(
        &mut self,
        owner: Slot,
        inner: SsaTypeId,
        symbol: Option<UnitSymbolId>,
        selector: CopyableWhenSelector,
        edges: (BlockRef, BlockRef),
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let (matched, next) = edges;
        if let Some(root) = symbol {
            self.forget_narrowing(root);
        }
        let when_null = match selector {
            CopyableWhenSelector::Null => matched,
            CopyableWhenSelector::Boolean(_) => next,
        };
        let Some(&EntityType::Value(wrapper)) = self.body.slots.get(owner.0) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let present = self.body.block();
        let payload = if self.types.map_results.get(&wrapper) == Some(&inner) {
            let boolean = self.boolean_type(span)?;
            let condition = self.value(
                ROp::AggregateProject {
                    aggregate: owner,
                    field: 0,
                },
                EntityType::Value(boolean),
                span,
            )?;
            self.terminate(
                RTerm::Branch {
                    condition,
                    when_true: present,
                    when_false: when_null,
                },
                span,
            )?;
            self.block = Some(present);
            self.value(
                ROp::MapResultUnwrap { result: owner },
                EntityType::Value(inner),
                span,
            )?
        } else {
            if !matches!(self.types.ssa_types.get(wrapper.index()),
                Some(crate::ssa::model::SsaTypeKind::InlineNullable { inner: actual, .. }) if *actual == inner)
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            let proof = self.body.slot(EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            });
            self.body.blocks[present.0].view = Some(proof);
            self.terminate(
                RTerm::NullBranch {
                    owner,
                    when_null,
                    when_non_null: present,
                },
                span,
            )?;
            self.block = Some(present);
            self.value(
                ROp::NullableTake { owner, proof },
                EntityType::Value(inner),
                span,
            )?
        };
        if let Some(symbol) = symbol {
            self.record_narrowing(symbol, payload, span)?;
        }
        self.branch_copied_selector(selector, payload, edges, span)?;
        Ok(matches!(selector, CopyableWhenSelector::Null).then_some(payload))
    }

    pub(super) fn branch_copied_selector(
        &mut self,
        selector: CopyableWhenSelector,
        condition: Slot,
        (matched, next): (BlockRef, BlockRef),
        span: Span,
    ) -> Result<(), LoweringError> {
        let CopyableWhenSelector::Boolean(expected) = selector else {
            return self.goto(next, span);
        };
        let (when_true, when_false) = if expected {
            (matched, next)
        } else {
            (next, matched)
        };
        self.terminate(
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            },
            span,
        )
    }

    pub(super) fn nullable_when_operand_drops(
        &self,
        subject: Option<WhenSubject>,
    ) -> Result<Vec<(usize, UnitDropTarget, Span)>, LoweringError> {
        let Some(WhenSubject::Nullable { control, .. }) = subject else {
            return Ok(Vec::new());
        };
        let source = nullable_when::source(self.inputs, control)?;
        let mut facts = Vec::new();
        for &operand in source.operands.iter().rev() {
            facts.extend(self.collect_drops(UnitDropPoint::AfterExpression(operand)));
        }
        Ok(facts)
    }

    pub(super) fn enter_nullable_when_arm(
        &mut self,
        subject: Option<WhenSubject>,
        index: usize,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(WhenSubject::Nullable {
            control,
            owner,
            proof,
        }) = subject
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let Some(proof) = proof.filter(|_| index == 1) else {
            return Ok(());
        };
        let inputs = self.inputs;
        let source = nullable_when::source(inputs, control)?;
        if let Some(symbol) = source.plan.descriptor().stable_symbol() {
            let slot = if source.loan_kind.is_some() {
                self.borrowed.get(&symbol)
            } else {
                self.bindings.get(&symbol)
            };
            if slot != Some(&owner)
                || self
                    .active_views
                    .iter()
                    .any(|view| view.when_scope == Some((control, owner)))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.active_views.push(View {
                symbol,
                slot: proof,
                target,
                when_scope: Some((control, owner)),
            });
        } else {
            // A temporary has no symbol/use witness. End its unused proof, retaining
            // the actual owner until the published full-expression cleanup point.
            self.push(ROp::BorrowEnd { loan: proof }, Vec::new(), span)?;
        }
        Ok(())
    }
}
