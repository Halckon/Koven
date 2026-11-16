//! Isolated input planning only. No legacy Plan, SSA emission, or publication authority.
use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::{BuiltinType, ParameterMode, UnitTypeKind, ValidatedCompilationUnitTypes},
};

pub(super) struct Plan<'a> {
    fact: &'a TwoSourceCounterFact,
    callable: DeclarationId,
    source_roles: [usize; 2],
}
impl<'a> Plan<'a> {
    pub(super) fn new(
        experiment: &'a TwoSourceExperiment,
        fact: &'a TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<Self> {
        if !experiment.owns(fact) || !experiment.matches_types(typed) {
            return None;
        }
        let sources = [fact.arms()[0].source(), fact.arms()[1].source()];
        if sources[0] == sources[1] {
            return None;
        }
        // Parameters are resolved symbols, never names or inferred distinct storage owners.
        let mut matches =
            typed
                .types()
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| {
                    let signature = declaration.callable()?;
                    if signature.receiver().is_some() {
                        return None;
                    }
                    let role = |source| {
                        let mut found = signature
                            .parameters()
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| p.symbol() == Some(source));
                        let (position, parameter) = found.next()?;
                        (found.next().is_none()
                            && parameter.mode() == ParameterMode::Borrow
                            && typed.types().types().get(parameter.ty())
                                == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
                        .then_some(position)
                    };
                    Some((
                        declaration.declaration(),
                        [role(sources[0])?, role(sources[1])?],
                    ))
                });
        let (callable, source_roles) = matches.next()?;
        if matches.next().is_some()
            || source_roles[0] == source_roles[1]
            || experiment.callable(fact)? != callable
        {
            return None;
        }
        // Re-check every closed arm call against the exact issuing typed product.
        for (arm, calls) in fact.arms().iter().zip(fact.calls()) {
            let expected = std::iter::once(arm.initializer())
                .chain(arm.consumers().iter().map(|(_, call, _)| *call));
            if calls.len() != 1 + arm.consumers().len() {
                return None;
            }
            for ((expression, instance), expected) in calls.iter().zip(expected) {
                if *expression != expected
                    || typed.types().call(*expression)?.instance() != instance
                {
                    return None;
                }
            }
        }
        Some(Self {
            fact,
            callable,
            source_roles,
        })
    }
    pub(super) fn fact(&self) -> &TwoSourceCounterFact {
        self.fact
    }
    pub(super) fn callable(&self) -> DeclarationId {
        self.callable
    }
    pub(super) fn source_roles(&self) -> [usize; 2] {
        self.source_roles
    }
}
