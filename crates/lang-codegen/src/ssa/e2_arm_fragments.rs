//! Detached closed-arm helpers only; these are not the original enclosing function or loop.
use super::{e2_callee_leaves, e2_two_source_plan::Plan, model::*};
use lang_frontend::{
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::ValidatedCompilationUnitTypes,
};
pub(super) struct EmittedArms {
    pub(super) leaves: e2_callee_leaves::EmittedLeaves,
    pub(super) arms: [FunctionId; 2],
    pub(super) columns: [usize; 2],
}
pub(super) fn emit(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedArms> {
    let leaves = e2_callee_leaves::emit_for_fact(experiment, fact, typed)?;
    append(experiment, fact, typed, leaves)
}
fn append(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
    mut leaves: e2_callee_leaves::EmittedLeaves,
) -> Option<EmittedArms> {
    let plan = Plan::new(experiment, fact, typed)?;
    let spans = experiment.arm_spans(fact)?;
    // The helper has only the two root parameters, ordered by their original signature roles.
    let columns = if plan.source_roles()[0] < plan.source_roles()[1] {
        [0, 1]
    } else {
        [1, 0]
    };
    let module = leaves.program.modules.first_mut()?;
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    let mut arms = Vec::new();
    for arm_index in 0..2 {
        let source = &spans[arm_index];
        let calls = &fact.calls()[arm_index];
        if calls.len() != source.calls().len() || calls.len() < 2 {
            return None;
        }
        let origin = Origin::Synthetic {
            anchor: source.body(),
            reason: "detached verified E2 arm".to_owned(),
        };
        let id = module
            .add_function(
                format!("e2_arm_{}_{}", module.functions.len(), arm_index),
                vec![],
                origin.clone(),
            )
            .ok()?;
        let function = module.function_mut(id)?;
        let block = function
            .add_block(vec![shared, shared], origin.clone())
            .ok()?;
        let EntityId::Loan(parent) = function.block(block)?.parameters[columns[arm_index]] else {
            return None;
        };
        let callee = |key| {
            leaves
                .functions
                .iter()
                .find(|(instance, _)| instance == key)
                .map(|(_, id)| *id)
        };
        let (_, results) = function
            .append_instruction(
                block,
                Operation::BorrowCall {
                    callee: callee(&calls[0].1)?,
                    arguments: vec![EntityId::Loan(parent)],
                    source: parent,
                },
                vec![shared],
                Origin::Source(source.calls()[0]),
            )
            .ok()?;
        let [EntityId::Loan(child)] = results.as_slice() else {
            return None;
        };
        for (position, (_, key)) in calls.iter().enumerate().skip(1) {
            function
                .append_instruction(
                    block,
                    Operation::DirectCall {
                        callee: callee(key)?,
                        receiver: None,
                        arguments: vec![EntityId::Loan(*child)],
                    },
                    vec![],
                    Origin::Source(source.calls()[position]),
                )
                .ok()?;
        }
        function
            .append_instruction(
                block,
                Operation::BorrowEnd { loan: *child },
                vec![],
                Origin::Source(source.end()),
            )
            .ok()?;
        function
            .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
            .ok()?;
        arms.push(id);
    }
    super::verify::verify_program(&leaves.program).ok()?;
    Some(EmittedArms {
        leaves,
        arms: arms.try_into().ok()?,
        columns,
    })
}

/// One common leaf/helper Program, not a caller/entry publication.
pub(super) struct EmittedArmSet<'a> {
    leaves: e2_callee_leaves::EmittedLeaves,
    experiment: &'a TwoSourceExperiment,
    locators: Vec<ArmLocator<'a>>,
}
pub(super) struct ArmLocator<'a> {
    fact: &'a TwoSourceCounterFact,
    arms: [FunctionId; 2],
    columns: [usize; 2],
}
impl ArmLocator<'_> {
    pub(super) fn arms(&self) -> [FunctionId; 2] {
        self.arms
    }
    pub(super) fn columns(&self) -> [usize; 2] {
        self.columns
    }
    fn matches(&self, fact: &TwoSourceCounterFact) -> bool {
        self.fact
            .arms()
            .iter()
            .zip(fact.arms())
            .all(|(a, b)| a.same_identity(b))
    }
}
impl EmittedArmSet<'_> {
    pub(super) fn program(&self) -> &Program {
        &self.leaves.program
    }
    pub(super) fn leaf_count(&self) -> usize {
        self.leaves.functions.len()
    }

    pub(super) fn len(&self) -> usize {
        self.locators.len()
    }
    /// Borrowed locator is bound to this Program and original analysis, never its position.
    pub(super) fn locator(
        &self,
        fact: &TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<&ArmLocator<'_>> {
        if !self.experiment.matches_types(typed) || !self.experiment.owns(fact) {
            return None;
        }
        let mut found = self.locators.iter().filter(|locator| locator.matches(fact));
        let locator = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(locator)
    }
}
pub(super) fn emit_for_facts<'a>(
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    experiment: &'a TwoSourceExperiment,
    facts: &[&'a TwoSourceCounterFact],
) -> Option<EmittedArmSet<'a>> {
    let typed = handoff.types();
    let mut leaves = e2_callee_leaves::emit_for_facts(handoff, experiment, facts)?;
    let mut locators = Vec::new();
    for fact in facts {
        let emitted = append(experiment, fact, typed, leaves)?;
        locators.push(ArmLocator {
            fact,
            arms: emitted.arms,
            columns: emitted.columns,
        });
        leaves = emitted.leaves;
    }
    let [module] = leaves.program.modules.as_slice() else {
        return None;
    };
    if locators.len() != facts.len()
        || module.functions.len() != leaves.functions.len() + 2 * facts.len()
    {
        return None;
    }
    let mut used: Vec<_> = leaves.functions.iter().map(|(_, id)| *id).collect();
    for fact in facts {
        let mut matches = locators.iter().filter(|locator| locator.matches(fact));
        let locator = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        for id in locator.arms {
            if used.contains(&id) {
                return None;
            }
            used.push(id);
        }
        super::e2_replay::replay_arms(
            module,
            &locator.arms,
            locator.columns,
            &leaves.functions,
            experiment,
            fact,
            typed,
        )?;
    }
    super::verify::verify_program(&leaves.program).ok()?;
    Some(EmittedArmSet {
        leaves,
        experiment,
        locators,
    })
}

/// Owned common Program with original callers; no process entries or publication receipt.
pub(super) struct EmittedCallerSet<'a> {
    arms: EmittedArmSet<'a>,
    callers: Vec<(&'a TwoSourceCounterFact, FunctionId)>,
}
impl EmittedCallerSet<'_> {
    pub(super) fn program(&self) -> &Program {
        self.arms.program()
    }
    pub(super) fn caller(
        &self,
        fact: &TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<FunctionId> {
        self.arms.locator(fact, typed)?;
        let mut matches = self.callers.iter().filter(|(original, _)| {
            original
                .arms()
                .iter()
                .zip(fact.arms())
                .all(|(a, b)| a.same_identity(b))
        });
        let (_, id) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(*id)
    }
}
impl<'a> EmittedArmSet<'a> {
    /// Consumes the actual Program. There is no input of caller-supplied function IDs.
    pub(super) fn append_callers(
        mut self,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<EmittedCallerSet<'a>> {
        if !self.experiment.matches_types(typed) {
            return None;
        }
        let mut callers = Vec::new();
        for locator in &self.locators {
            let shell = self.experiment.function_shell(locator.fact, typed)?;
            let spans = self.experiment.arm_spans(locator.fact)?;
            let id = super::e2_function::append_caller(
                self.leaves.program.modules.first_mut()?,
                shell,
                locator.arms,
                locator.columns,
                spans,
            )?;
            callers.push((locator.fact, id));
        }
        super::verify::verify_program(&self.leaves.program).ok()?;
        let [module] = self.leaves.program.modules.as_slice() else {
            return None;
        };
        if callers.len() != self.locators.len()
            || module.functions.len() != self.leaves.functions.len() + 3 * self.locators.len()
        {
            return None;
        }
        let mut used: Vec<_> = self.leaves.functions.iter().map(|(_, id)| *id).collect();
        for locator in &self.locators {
            used.extend(locator.arms);
        }
        for (fact, id) in &callers {
            if used.contains(id) {
                return None;
            }
            used.push(*id);
            let locator = self.locator(fact, typed)?;
            super::e2_replay::replay_arms(
                module,
                &locator.arms,
                locator.columns,
                &self.leaves.functions,
                self.experiment,
                fact,
                typed,
            )?;
            super::e2_replay::replay_caller(
                module,
                *id,
                locator.arms,
                locator.columns,
                self.experiment.function_shell(fact, typed)?,
                self.experiment.arm_spans(fact)?,
            )?;
        }
        Some(EmittedCallerSet {
            arms: self,
            callers,
        })
    }
}
