//! Complete bounded original caller, test-only. No production receipt or activation.
use super::{e2_arm_fragments, model::*};
use lang_frontend::{
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::ValidatedCompilationUnitTypes,
};
pub(super) struct EmittedFunction<'a> {
    experiment: &'a TwoSourceExperiment,
    fact: &'a TwoSourceCounterFact,
    pub(super) fragments: e2_arm_fragments::EmittedArms,
    pub(super) caller: FunctionId,
}
impl EmittedFunction<'_> {
    pub(super) fn issued_for(
        &self,
        experiment: &TwoSourceExperiment,
        fact: &TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> bool {
        experiment.owns(self.fact)
            && self.experiment.matches_types(typed)
            && fact
                .arms()
                .iter()
                .zip(self.fact.arms())
                .all(|(a, b)| a.same_identity(b))
    }
}
fn loan(entity: EntityId) -> Option<LoanId> {
    if let EntityId::Loan(id) = entity {
        Some(id)
    } else {
        None
    }
}
fn value(entity: EntityId) -> Option<ValueId> {
    if let EntityId::Value(id) = entity {
        Some(id)
    } else {
        None
    }
}
pub(super) fn emit<'a>(
    experiment: &'a TwoSourceExperiment,
    fact: &'a TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedFunction<'a>> {
    let shell = experiment.function_shell(fact, typed)?;
    let mut fragments = e2_arm_fragments::emit(experiment, fact, typed)?;
    let caller = append_caller(
        fragments.leaves.program.modules.first_mut()?,
        shell,
        fragments.arms,
        fragments.columns,
        experiment.arm_spans(fact)?,
    )?;
    super::verify::verify_program(&fragments.leaves.program).ok()?;
    Some(EmittedFunction {
        fragments,
        caller,
        experiment,
        fact,
    })
}
/// Internal emitter: caller supplies locators from the same owned Program, never another package.
pub(super) fn append_caller(
    module: &mut Module,
    shell: &lang_frontend::ownership_checking::two_source_experiment::FunctionShell,
    arms: [FunctionId; 2],
    columns: [usize; 2],
    spans: &[lang_frontend::ownership_checking::two_source_experiment::ArmSourceSpans; 2],
) -> Option<FunctionId> {
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let roles = shell.roles();
    let mut parameters = vec![shared(string); 4];
    parameters[roles[2]] = shared(integer);
    parameters[roles[3]] = shared(boolean);
    let caller = module
        .add_function(
            format!("e2_original_caller_{}", module.functions.len()),
            vec![],
            Origin::Source(shell.body()),
        )
        .ok()?;
    let f = module.function_mut(caller)?;
    let synthetic = || Origin::Synthetic {
        anchor: shell.body(),
        reason: "bounded E2 caller CFG".to_owned(),
    };
    let entry = f
        .add_block(parameters.clone(), Origin::Source(shell.body()))
        .ok()?;
    parameters.push(EntityType::Value(integer));
    // Every normal edge transports the full original ABI and the current counter.
    let header = f.add_block(parameters.clone(), synthetic()).ok()?;
    let split = f.add_block(parameters.clone(), synthetic()).ok()?;
    let arm_a = f.add_block(parameters.clone(), synthetic()).ok()?;
    let arm_b = f.add_block(parameters.clone(), synthetic()).ok()?;
    let join = f.add_block(parameters.clone(), synthetic()).ok()?;
    let success = f.add_block(parameters.clone(), synthetic()).ok()?;
    let exit = f.add_block(parameters.clone(), synthetic()).ok()?;
    parameters.push(EntityType::Value(integer));
    let abort = f.add_block(parameters, synthetic()).ok()?;
    let [init_span, condition_span, choose_span, decrement_span] = shell.control_spans();
    let [zero_span, one_span] = shell.literals();
    let entry_args = f.block(entry)?.parameters.clone();
    let (_, initial) = f
        .append_instruction(
            entry,
            Operation::Read {
                source: PlaceAccess::Loan(loan(entry_args[roles[2]])?),
            },
            vec![EntityType::Value(integer)],
            Origin::Source(init_span),
        )
        .ok()?;
    let mut arguments = entry_args;
    arguments.push(*initial.first()?);
    f.set_terminator(
        entry,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments,
        }),
        synthetic(),
    )
    .ok()?;
    let args = f.block(header)?.parameters.clone();
    let (_, zero) = f
        .append_instruction(
            header,
            Operation::Constant(ScalarConstant::Integer(0)),
            vec![EntityType::Value(integer)],
            Origin::Source(zero_span),
        )
        .ok()?;
    let (_, condition) = f
        .append_instruction(
            header,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left: value(args[4])?,
                right: value(zero[0])?,
            },
            vec![EntityType::Value(boolean)],
            Origin::Source(condition_span),
        )
        .ok()?;
    f.set_terminator(
        header,
        TerminatorKind::Conditional {
            condition: value(condition[0])?,
            when_true: Edge {
                target: split,
                arguments: args.clone(),
            },
            when_false: Edge {
                target: exit,
                arguments: args,
            },
        },
        Origin::Source(condition_span),
    )
    .ok()?;
    let args = f.block(split)?.parameters.clone();
    let (_, choose) = f
        .append_instruction(
            split,
            Operation::Read {
                source: PlaceAccess::Loan(loan(args[roles[3]])?),
            },
            vec![EntityType::Value(boolean)],
            Origin::Source(choose_span),
        )
        .ok()?;
    f.set_terminator(
        split,
        TerminatorKind::Conditional {
            condition: value(choose[0])?,
            when_true: Edge {
                target: arm_a,
                arguments: args.clone(),
            },
            when_false: Edge {
                target: arm_b,
                arguments: args,
            },
        },
        Origin::Source(choose_span),
    )
    .ok()?;
    for (index, block) in [arm_a, arm_b].into_iter().enumerate() {
        let args = f.block(block)?.parameters.clone();
        let mut actual = vec![args[roles[0]]; 2];
        actual[columns[0]] = args[roles[0]];
        actual[columns[1]] = args[roles[1]];
        f.append_instruction(
            block,
            Operation::DirectCall {
                callee: arms[index],
                receiver: None,
                arguments: actual,
            },
            vec![],
            Origin::Synthetic {
                anchor: spans[index].body(),
                reason: "outline verified E2 arm".to_owned(),
            },
        )
        .ok()?;
        f.set_terminator(
            block,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: args,
            }),
            synthetic(),
        )
        .ok()?;
    }
    let args = f.block(join)?.parameters.clone();
    let (_, one) = f
        .append_instruction(
            join,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            Origin::Source(one_span),
        )
        .ok()?;
    let (_, checked) = f
        .append_instruction(
            join,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left: value(args[4])?,
                right: value(one[0])?,
            },
            vec![EntityType::Value(integer), EntityType::Value(boolean)],
            Origin::Source(decrement_span),
        )
        .ok()?;
    let mut success_args = args.clone();
    success_args[4] = checked[0];
    let mut abort_args = args;
    abort_args.push(one[0]);
    f.set_terminator(
        join,
        TerminatorKind::Conditional {
            condition: value(checked[1])?,
            when_true: Edge {
                target: abort,
                arguments: abort_args,
            },
            when_false: Edge {
                target: success,
                arguments: success_args,
            },
        },
        Origin::Source(decrement_span),
    )
    .ok()?;
    let args = f.block(success)?.parameters.clone();
    f.set_terminator(
        success,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments: args,
        }),
        Origin::Source(decrement_span),
    )
    .ok()?;
    let divisor = value(f.block(abort)?.parameters[5])?;
    f.set_abort_terminator(
        abort,
        AbortReason::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            divisor,
        },
        Origin::Source(decrement_span),
    )
    .ok()?;
    f.set_terminator(exit, TerminatorKind::Return { values: vec![] }, synthetic())
        .ok()?;
    Some(caller)
}
