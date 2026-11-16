//! Independent experimental replay. No producer reconstruction and no production authority.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use super::e2_function::EmittedFunction;
use super::model::*;
use lang_frontend::ownership_checking::two_source_experiment::{
    CalleeLeaf, CalleeLeafBody, FunctionShell, TwoSourceCounterFact, TwoSourceExperiment,
};
use lang_frontend::type_checking::ValidatedCompilationUnitTypes;
macro_rules! require {
    ($condition:expr $(, $message:expr)? $(,)?) => {
        if !$condition {
            return None;
        }
    };
}
macro_rules! same {
    ($left:expr, $right:expr $(, $message:expr)? $(,)?) => {
        require!($left == $right);
    };
}
macro_rules! different {
    ($left:expr, $right:expr $(, $message:expr)? $(,)?) => {
        require!($left != $right);
    };
}
macro_rules! value {
    ($entity:expr) => {{
        let EntityId::Value(id) = $entity else {
            return None;
        };
        id
    }};
}
macro_rules! loan {
    ($entity:expr) => {{
        let EntityId::Loan(id) = $entity else {
            return None;
        };
        id
    }};
}
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn replay_caller(
    module: &Module,
    id: FunctionId,
    arms: [FunctionId; 2],
    columns: [usize; 2],
    shell: &FunctionShell,
    spans: &[lang_frontend::ownership_checking::two_source_experiment::ArmSourceSpans; 2],
) -> Option<()> {
    replay_declared_caller(module, id, arms, columns, shell, spans, shell.body())
}
/// The complete-plan checker supplies the original instance declaration span, never an emitter span.
pub(super) fn replay_declared_caller(
    module: &Module,
    id: FunctionId,
    arms: [FunctionId; 2],
    columns: [usize; 2],
    shell: &FunctionShell,
    spans: &[lang_frontend::ownership_checking::two_source_experiment::ArmSourceSpans; 2],
    declaration: lang_frontend::source::Span,
) -> Option<()> {
    let f = module.function(id)?;
    require!(f.receiver.is_none() && f.carrier_return.is_none() && !f.never_returns);
    for block in &f.blocks {
        let term = block.terminator.as_ref()?;
        if !matches!(term.kind, TerminatorKind::Abort) {
            require!(term.abort.is_none());
        }
    }
    same!(f.origin, Origin::Source(declaration));
    require!(f.return_types.is_empty());
    same!(f.borrow_return, None);
    let [entry, header, split, a, b, join, success, exit, abort] = f.blocks.as_slice() else {
        return None;
    };
    let synthetic = Origin::Synthetic {
        anchor: shell.body(),
        reason: "bounded E2 caller CFG".to_owned(),
    };
    same!(entry.origin, Origin::Source(declaration));
    for block in [header, split, a, b, join, success, exit, abort] {
        same!(block.origin, synthetic);
    }
    for block in [entry, a, b, exit] {
        same!(block.terminator.as_ref()?.origin, synthetic);
    }
    let roles = shell.roles();
    same!(entry.parameters.len(), 4);
    for (role, expected) in roles.into_iter().zip([
        SsaTypeKind::StringOwner,
        SsaTypeKind::StringOwner,
        SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        SsaTypeKind::Boolean,
    ]) {
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = f.entity(entry.parameters[role])?.ty
        else {
            return None;
        };
        same!(module.types[target.index()], expected);
    }
    for block in [header, split, a, b, join, success, exit] {
        same!(block.parameters.len(), 5);
        for (actual, expected) in block.parameters[..4].iter().zip(&entry.parameters) {
            different!(actual, expected, "per-block identities must rebind");
            same!(f.entity(*actual)?.ty, f.entity(*expected)?.ty);
        }
    }
    let instructions = |block: &Block| {
        block
            .instructions
            .iter()
            .map(|id| f.instruction(*id))
            .collect::<Option<Vec<_>>>()
    };
    let branch = |block: &Block, target: &Block, arguments: Vec<EntityId>| {
        same!(
            block.terminator.as_ref()?.kind,
            TerminatorKind::Branch(Edge {
                target: target.id,
                arguments
            })
        );
        Some(())
    };
    let [init_span, condition_span, choose_span, decrement_span] = shell.control_spans();
    let [zero_span, one_span] = shell.literals();
    same!(
        header.terminator.as_ref()?.origin,
        Origin::Source(condition_span)
    );
    same!(
        split.terminator.as_ref()?.origin,
        Origin::Source(choose_span)
    );
    for block in [join, success, abort] {
        same!(
            block.terminator.as_ref()?.origin,
            Origin::Source(decrement_span)
        );
    }
    let i = instructions(entry)?;
    same!(i.len(), 1);
    same!(
        i[0].operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan!(entry.parameters[roles[2]]))
        }
    );
    same!(i[0].origin, Origin::Source(init_span));
    let mut args = entry.parameters.clone();
    args.push(i[0].results[0]);
    branch(entry, header, args)?;
    let i = instructions(header)?;
    same!(i.len(), 2);
    same!(
        i[0].operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    same!(i[0].origin, Origin::Source(zero_span));
    same!(
        i[1].operation,
        Operation::Compare {
            operator: ComparisonOperator::GreaterThan,
            left: value!(header.parameters[4]),
            right: value!(i[0].results[0])
        }
    );
    same!(i[1].origin, Origin::Source(condition_span));
    same!(
        header.terminator.as_ref()?.kind,
        TerminatorKind::Conditional {
            condition: value!(i[1].results[0]),
            when_true: Edge {
                target: split.id,
                arguments: header.parameters.clone()
            },
            when_false: Edge {
                target: exit.id,
                arguments: header.parameters.clone()
            }
        }
    );
    let i = instructions(split)?;
    same!(i.len(), 1);
    same!(
        i[0].operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan!(split.parameters[roles[3]]))
        }
    );
    same!(i[0].origin, Origin::Source(choose_span));
    same!(
        split.terminator.as_ref()?.kind,
        TerminatorKind::Conditional {
            condition: value!(i[0].results[0]),
            when_true: Edge {
                target: a.id,
                arguments: split.parameters.clone()
            },
            when_false: Edge {
                target: b.id,
                arguments: split.parameters.clone()
            }
        }
    );
    for (index, arm) in [a, b].into_iter().enumerate() {
        let i = instructions(arm)?;
        same!(
            i.first()?.origin,
            Origin::Synthetic {
                anchor: spans[index].body(),
                reason: "outline verified E2 arm".to_owned()
            }
        );
        same!(i.len(), 1);
        let mut args = vec![arm.parameters[roles[0]]; 2];
        args[columns[0]] = arm.parameters[roles[0]];
        args[columns[1]] = arm.parameters[roles[1]];
        same!(
            i[0].operation,
            Operation::DirectCall {
                callee: arms[index],
                receiver: None,
                arguments: args
            }
        );
        require!(i[0].results.is_empty());
        branch(arm, join, arm.parameters.clone())?;
    }
    let i = instructions(join)?;
    same!(i.len(), 2);
    same!(
        i[0].operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    same!(i[0].origin, Origin::Source(one_span));
    same!(
        i[1].operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            left: value!(join.parameters[4]),
            right: value!(i[0].results[0])
        }
    );
    same!(i[1].origin, Origin::Source(decrement_span));
    let mut success_args = join.parameters.clone();
    success_args[4] = i[1].results[0];
    let mut abort_args = join.parameters.clone();
    abort_args.push(i[0].results[0]);
    same!(
        join.terminator.as_ref()?.kind,
        TerminatorKind::Conditional {
            condition: value!(i[1].results[1]),
            when_true: Edge {
                target: abort.id,
                arguments: abort_args
            },
            when_false: Edge {
                target: success.id,
                arguments: success_args
            }
        }
    );
    require!(success.instructions.is_empty());
    branch(success, header, success.parameters.clone())?;
    require!(exit.instructions.is_empty());
    same!(
        exit.terminator.as_ref()?.kind,
        TerminatorKind::Return { values: vec![] }
    );
    require!(abort.instructions.is_empty());
    same!(abort.parameters.len(), 6);
    let term = abort.terminator.as_ref()?;
    same!(term.kind, TerminatorKind::Abort);
    same!(
        term.abort,
        Some(AbortReason::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            divisor: value!(abort.parameters[5])
        })
    );
    same!(term.origin, Origin::Source(decrement_span));
    require!(
        f.instructions
            .iter()
            .all(|i| !matches!(i.operation, Operation::BorrowEnd { .. })),
        "caller does not consume Shared inputs"
    );
    Some(())
}

// IDs supplied by the builder are locators only: each located body is replayed against FE data.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn replay(
    emitted: &EmittedFunction,
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<()> {
    replay_closure(emitted, experiment, fact, typed, None)
}
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn replay_closure(
    emitted: &EmittedFunction,
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
    additional_entry: Option<FunctionId>,
) -> Option<()> {
    require!(emitted.issued_for(experiment, fact, typed));
    let shell = experiment.function_shell(fact, typed)?;
    let program = &emitted.fragments.leaves.program;
    // Structural gate first makes all subsequent bounded positional queries well-formed.
    super::verify::verify_program(program).ok()?;
    let [module] = program.modules.as_slice() else {
        return None;
    };
    for function in &module.functions {
        require!(
            function.receiver.is_none()
                && function.carrier_return.is_none()
                && !function.never_returns
        );
        for block in &function.blocks {
            let term = block.terminator.as_ref()?;
            if !matches!(term.kind, TerminatorKind::Abort) {
                require!(term.abort.is_none());
            }
        }
    }
    // Independently derive the selected fact's source-call requirement. Other facts'
    // leaf definitions must not silently enlarge this publication's coverage.
    let required = required_keys(experiment, fact, typed)?;
    let leaves: Vec<_> = experiment
        .callee_leaves_for(fact, typed)?
        .iter()
        .filter(|leaf| required.contains(leaf.instance()))
        .collect();
    same!(leaves.len(), required.len());
    same!(leaves.len(), emitted.fragments.leaves.functions.len());
    same!(
        module.functions.len(),
        leaves.len() + 3 + usize::from(additional_entry.is_some())
    );
    let mut used = vec![emitted.caller];
    if let Some(id) = additional_entry {
        module.function(id)?;
        require!(!used.contains(&id));
        used.push(id);
    }
    for (leaf, (key, id)) in leaves.iter().zip(&emitted.fragments.leaves.functions) {
        require!(experiment.owns_leaf(leaf));
        same!(key, leaf.instance());
        require!(!used.contains(id));
        used.push(*id);
        replay_leaf(module, *id, leaf, &emitted.fragments.leaves.functions)?;
    }
    for id in emitted.fragments.arms {
        require!(!used.contains(&id));
        used.push(id);
    }
    replay_arms(
        module,
        &emitted.fragments.arms,
        emitted.fragments.columns,
        &emitted.fragments.leaves.functions,
        experiment,
        fact,
        typed,
    )?;
    replay_caller(
        module,
        emitted.caller,
        emitted.fragments.arms,
        emitted.fragments.columns,
        shell,
        experiment.arm_spans(fact)?,
    )
}
fn shared_string(module: &Module, f: &Function, entity: EntityId) -> Option<SsaTypeId> {
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = f.entity(entity)?.ty
    else {
        return None;
    };
    same!(
        module.types.get(target.index()),
        Some(&SsaTypeKind::StringOwner)
    );
    Some(target)
}

/// Reconstruct the bounded transitive source graph from original callsites, never emitted IDs.
pub(super) fn required_keys(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<Vec<lang_frontend::type_checking::UnitCallableInstanceKey>> {
    if !experiment.owns(fact) || !experiment.matches_types(typed) {
        return None;
    }
    let leaves = experiment.callee_leaves_for(fact, typed)?;
    let mut required = Vec::new();
    for (call, recorded) in fact.calls().iter().flatten() {
        let actual = typed.types().call(*call)?.instance();
        if actual != recorded {
            return None;
        }
        if !required.contains(actual) {
            required.push(actual.clone());
        }
    }
    let mut cursor = 0;
    while cursor < required.len() {
        let mut found = leaves
            .iter()
            .filter(|leaf| leaf.instance() == &required[cursor]);
        let leaf = found.next()?;
        if found.next().is_some() || !experiment.owns_leaf(leaf) {
            return None;
        }
        if let Some((call, recorded)) = leaf.source_call() {
            let actual = typed.types().call(call)?.instance();
            if actual != recorded {
                return None;
            }
            if !required.contains(actual) {
                required.push(actual.clone());
            }
        }
        cursor += 1;
    }
    (required.len() == leaves.len()).then_some(required)
}

/// Independent body/ABI replay shared by single-closure and joint-leaf experiments.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn replay_leaf(
    module: &Module,
    id: FunctionId,
    leaf: &CalleeLeaf,
    functions: &[(
        lang_frontend::type_checking::UnitCallableInstanceKey,
        FunctionId,
    )],
) -> Option<()> {
    replay_declared_leaf(module, id, leaf, functions, leaf.span())
}
pub(super) fn replay_declared_leaf(
    module: &Module,
    id: FunctionId,
    leaf: &CalleeLeaf,
    functions: &[(
        lang_frontend::type_checking::UnitCallableInstanceKey,
        FunctionId,
    )],
    declaration: lang_frontend::source::Span,
) -> Option<()> {
    let f = module.function(id)?;
    require!(f.receiver.is_none() && f.carrier_return.is_none() && !f.never_returns);
    same!(f.origin, Origin::Source(declaration));
    let [block] = f.blocks.as_slice() else {
        return None;
    };
    same!(block.origin, Origin::Source(declaration));
    let [parameter] = block.parameters.as_slice() else {
        return None;
    };
    let ty = shared_string(module, f, *parameter)?;
    let term = block.terminator.as_ref()?;
    require!(term.abort.is_none());
    same!(term.origin, Origin::Source(declaration));
    match leaf.body() {
        CalleeLeafBody::SharedIdentity => {
            require!(f.instructions.is_empty() && leaf.input_call_loan().is_none());
            same!(f.borrow_return, Some(0));
            same!(f.return_types, vec![ty]);
            same!(
                term.kind,
                TerminatorKind::BorrowReturn {
                    loan: loan!(*parameter)
                }
            );
        }
        CalleeLeafBody::EmptyUnit
        | CalleeLeafBody::PrintSharedParameter
        | CalleeLeafBody::ForwardSharedParameter => {
            if leaf.body() == CalleeLeafBody::PrintSharedParameter {
                let [instruction] = f.instructions.as_slice() else {
                    return None;
                };
                same!(
                    instruction.operation,
                    Operation::PrintString {
                        value: loan!(*parameter)
                    }
                );
                same!(
                    instruction.origin,
                    Origin::Source(leaf.input_call_loan()?.end_span())
                );
                require!(instruction.results.is_empty());
                same!(block.instructions, vec![instruction.id]);
            } else if leaf.body() == CalleeLeafBody::ForwardSharedParameter {
                let [instruction] = f.instructions.as_slice() else {
                    return None;
                };
                let (_, key) = leaf.source_call()?;
                let mut targets = functions.iter().filter(|(candidate, _)| candidate == key);
                let (_, callee) = targets.next()?;
                require!(targets.next().is_none() && *callee != id);
                same!(
                    instruction.operation,
                    Operation::DirectCall {
                        callee: *callee,
                        receiver: None,
                        arguments: vec![*parameter]
                    }
                );
                same!(
                    instruction.origin,
                    Origin::Source(leaf.input_call_loan()?.end_span())
                );
                require!(instruction.results.is_empty());
                same!(block.instructions, vec![instruction.id]);
            } else {
                require!(f.instructions.is_empty() && leaf.input_call_loan().is_none());
            }
            same!(f.borrow_return, None);
            require!(f.return_types.is_empty());
            same!(term.kind, TerminatorKind::Return { values: vec![] });
        }
    }
    Some(())
}

/// Replay exact source arm bodies against shared concrete-instance locators.
pub(super) fn replay_arms(
    module: &Module,
    arms: &[FunctionId; 2],
    columns: [usize; 2],
    functions: &[(
        lang_frontend::type_checking::UnitCallableInstanceKey,
        FunctionId,
    )],
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<()> {
    require!(experiment.owns(fact) && experiment.matches_types(typed));
    let roles = experiment.function_shell(fact, typed)?.roles();
    same!(columns, if roles[0] < roles[1] { [0, 1] } else { [1, 0] });
    let spans = experiment.arm_spans(fact)?;
    for (index, id) in arms.iter().enumerate() {
        let f = module.function(*id)?;
        require!(f.receiver.is_none() && f.carrier_return.is_none() && !f.never_returns);
        same!(f.borrow_return, None);
        require!(f.return_types.is_empty());
        let [block] = f.blocks.as_slice() else {
            return None;
        };
        same!(block.parameters.len(), 2);
        for parameter in &block.parameters {
            shared_string(module, f, *parameter)?;
        }
        let origin = Origin::Synthetic {
            anchor: spans[index].body(),
            reason: "detached verified E2 arm".to_owned(),
        };
        same!(f.origin, origin);
        same!(block.origin, origin);
        let term = block.terminator.as_ref()?;
        require!(term.abort.is_none());
        same!(term.origin, origin);
        same!(term.kind, TerminatorKind::Return { values: vec![] });
        let calls = &fact.calls()[index];
        same!(calls.len(), spans[index].calls().len());
        same!(block.instructions.len(), calls.len() + 1);
        let parent = loan!(block.parameters[columns[index]]);
        let mut child = None;
        for (position, ((expression, key), span)) in
            calls.iter().zip(spans[index].calls()).enumerate()
        {
            same!(typed.types().call(*expression)?.instance(), key);
            let callee = functions.iter().find(|(candidate, _)| candidate == key)?.1;
            let instruction = f.instruction(block.instructions[position])?;
            same!(instruction.origin, Origin::Source(*span));
            if position == 0 {
                same!(
                    instruction.operation,
                    Operation::BorrowCall {
                        callee,
                        arguments: vec![EntityId::Loan(parent)],
                        source: parent
                    }
                );
                let [result] = instruction.results.as_slice() else {
                    return None;
                };
                shared_string(module, f, *result)?;
                require!(!block.parameters.contains(result));
                child = Some(loan!(*result));
            } else {
                require!(instruction.results.is_empty());
                same!(
                    instruction.operation,
                    Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments: vec![EntityId::Loan(child?)]
                    }
                );
            }
        }
        let end = f.instruction(*block.instructions.last()?)?;
        require!(end.results.is_empty());
        same!(end.operation, Operation::BorrowEnd { loan: child? });
        same!(end.origin, Origin::Source(spans[index].end()));
    }
    Some(())
}
