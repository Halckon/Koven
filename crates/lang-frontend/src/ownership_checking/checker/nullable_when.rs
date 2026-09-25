//! typed nullable plan 的内部 subject identity 与交付记录。
use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::{
    DropFact, DropPoint, NullableWhenBranchFact, NullableWhenBranchOutcome as Outcome,
    NullableWhenExtractionFact, NullableWhenExtractionKind as Kind, NullableWhenOwnershipPlan,
    NullableWhenProofView,
};
use crate::{
    ast::ExpressionId,
    parser::Expression,
    type_checking::{
        Copyability, NullableWhenSubjectCategory, TypeKind, WhenDomain, WhenDomainAtom,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Proof {
    pub(super) control: ExpressionId,
    entry: usize,
    alternative: Option<usize>,
    view: NullableWhenProofView,
}

impl Checker<'_> {
    pub(super) fn check_nullable_subject(
        &mut self,
        control: ExpressionId,
        subject: ExpressionId,
        mut flows: Flows,
    ) -> Result<Flows, OwnershipCheckingError> {
        let is_place = self.typed.nullable_when(control).is_some_and(|plan| {
            matches!(
                plan.category(),
                NullableWhenSubjectCategory::OrdinaryField
                    | NullableWhenSubjectCategory::ContainerElement
            )
        });
        if !is_place {
            return self.chain_expression(flows, subject, ExpressionUse::Read);
        }
        flows = self.chain_expression(flows, subject, ExpressionUse::Place)?;
        if let Some(state) = flows.next.as_mut() {
            let span = self.parsed.ast().expressions().get(subject)?.span();
            self.access_expression_place(subject, AccessKind::Read, span, state)?;
        }
        Ok(flows)
    }
    pub(super) fn begin_nullable_when(
        &mut self,
        id: ExpressionId,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(plan) = self.typed.nullable_when(id) else {
            return Ok(());
        };
        let place = self.place(plan.subject())?;
        let branches = plan.entries().iter().enumerate().map(|(index, entry)| {
            let view = entry.body_type().map(|inner_type| NullableWhenProofView { subject: plan.subject(), place: place.clone(), stable_symbol: entry.stable_symbol(), inner_type });
            let null = matches!(entry.body_domain(), WhenDomain::Null) || matches!(entry.body_domain(), WhenDomain::Finite(atoms) if atoms == &[WhenDomainAtom::Null]);
            NullableWhenBranchFact { index, view, outcome: if null { Outcome::Null } else { Outcome::Unextracted }, drops: Vec::new() }
        }).collect();
        self.nullable_whens.insert(
            id.index(),
            NullableWhenOwnershipPlan {
                expression: id,
                subject: plan.subject(),
                category: plan.category(),
                branches,
                extractions: Vec::new(),
            },
        );
        Ok(())
    }
    /// Register the single evaluation before any condition can invalidate its root.
    pub(super) fn register_nullable_subject(
        &self,
        id: ExpressionId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(plan) = self.typed.nullable_when(id) else {
            return Ok(());
        };
        let (Some(symbol), Some(TypeKind::Nullable(inner))) = (
            plan.stable_symbol(),
            self.typed.types().get(plan.subject_type()),
        ) else {
            return Ok(());
        };
        state.nullable_views.insert(
            symbol,
            Proof {
                control: id,
                entry: 0,
                alternative: None,
                view: NullableWhenProofView {
                    subject: plan.subject(),
                    place: self.place(plan.subject())?,
                    stable_symbol: Some(symbol),
                    inner_type: *inner,
                },
            },
        );
        Ok(())
    }
    pub(super) fn enter_nullable_edge(
        &self,
        id: ExpressionId,
        entry: usize,
        alternative: Option<usize>,
        state: &mut State,
    ) {
        for proof in state
            .nullable_views
            .values_mut()
            .filter(|proof| proof.control == id)
        {
            proof.entry = entry;
            proof.alternative = alternative;
        }
    }
    pub(super) fn record_nullable_extraction(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        proof: Option<Proof>,
    ) {
        let Some(proof) = proof else {
            return;
        };
        if usage != ExpressionUse::Consume
            || self.typed.expression_type(id) != Some(proof.view.inner_type)
        {
            return;
        }
        let kind = if self.typed.copyability(proof.view.inner_type) == Some(Copyability::MoveOnly) {
            Kind::Consume
        } else {
            Kind::Copy
        };
        if let Some(plan) = self.nullable_whens.get_mut(&proof.control.index()) {
            let fact = NullableWhenExtractionFact {
                expression: id,
                entry: proof.entry,
                alternative: proof.alternative,
                kind,
            };
            if !plan.extractions.contains(&fact) {
                plan.extractions.push(fact);
            }
        }
    }
    pub(super) fn finish_nullable_branch(&mut self, id: ExpressionId, entry: usize, flows: &Flows) {
        if let Some(plan) = self.nullable_whens.get_mut(&id.index()) {
            if flows.next.is_none() {
                plan.branches[entry].outcome = Outcome::Diverging;
            } else if plan
                .extractions
                .iter()
                .any(|fact| fact.entry == entry && fact.kind == Kind::Consume)
            {
                plan.branches[entry].outcome = Outcome::MayConsume;
            }
        }
    }
    pub(super) fn finish_nullable_drops(
        &mut self,
        drops: &[DropFact],
    ) -> Result<(), OwnershipCheckingError> {
        for plan in self.nullable_whens.values_mut() {
            let Expression::When { entries, .. } = self
                .parsed
                .ast()
                .expressions()
                .get(plan.expression)?
                .payload()
            else {
                continue;
            };
            for branch in &mut plan.branches {
                let body = self
                    .parsed
                    .ast()
                    .statements()
                    .get(entries[branch.index].body)?
                    .span();
                for fact in drops {
                    let exact = matches!(fact.point(), DropPoint::BranchExit { control, branch: index } if control == plan.expression && index == branch.index)
                        || matches!(fact.point(), DropPoint::WhenAlternativeMatch { control, entry, .. } if control == plan.expression && entry == branch.index);
                    let span = match fact.point() {
                        DropPoint::AfterExpression(id)
                        | DropPoint::AfterBinaryOperands(id)
                        | DropPoint::CallEntry(id)
                        | DropPoint::CallReturn(id)
                        | DropPoint::ControlTransfer(id)
                        | DropPoint::AfterReplacement(id)
                        | DropPoint::BranchExit { control: id, .. }
                        | DropPoint::WhenAlternativeMatch { control: id, .. } => {
                            Some(self.parsed.ast().expressions().get(id)?.span())
                        }
                        DropPoint::AfterStatement(id) | DropPoint::LoopExit(id) => {
                            Some(self.parsed.ast().statements().get(id)?.span())
                        }
                        DropPoint::FunctionEntry(_) | DropPoint::LambdaEntry(_) => None,
                    };
                    if exact
                        || span.is_some_and(|span| {
                            span.start() >= body.start() && span.end() <= body.end()
                        })
                    {
                        branch.drops.push(*fact);
                    }
                }
            }
        }
        Ok(())
    }
}
