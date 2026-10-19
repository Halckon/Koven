//! Both original argument arms and all scalar/loan uses through the actual selected Call.
use super::*;
/// Actual source anchors shared by the original temporary and named-local adapters.
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Input {
    pub call: Span,
    pub argument: Span,
    pub flag: Span,
    pub arms: [Span; 2],
}
#[allow(clippy::too_many_arguments)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check<'p>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &Input,
    module: &Module,
    g: &Graph<'p>,
    factory: &Function,
    uses: &mut Uses,
) -> Result<(&'p Instruction, &'p Instruction), Failure> {
    let call = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==factory.id),
        ),
        "exactly one selected factory Call",
    )?;
    need(
        call.origin == Origin::Source(input.call),
        "selected factory actual source Call origin",
        call,
    )?;
    let Operation::DirectCall {
        receiver: None,
        arguments,
        ..
    } = &call.operation
    else {
        return Err(failure("ordinary factory direct ABI", call));
    };
    let [arg @ EntityId::Loan(actual_loan)] = arguments.as_slice() else {
        return Err(failure("factory argument actual Shared Int loan", call));
    };
    let begin = g.definition(*arg)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("actual argument Shared birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner: phi } = root.operation else {
        return Err(failure("actual argument RootPlace", root));
    };
    let int = g
        .function
        .entity(EntityId::Value(phi))
        .unwrap()
        .ty
        .semantic_type();
    let factory_parameters = &factory
        .block(factory.entry_block().unwrap())
        .unwrap()
        .parameters;
    let [factory_parameter] = factory_parameters.as_slice() else {
        return Err(failure(
            "factory actual one Borrow Int parameter",
            factory_parameters,
        ));
    };
    let shared_int = EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    };
    need(
        module.type_kind(int)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && g.function.entity(*arg).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: int,
                }
            && g.function.entity(EntityId::Place(place)).unwrap().ty == EntityType::Place(int)
            && factory.entity(*factory_parameter).unwrap().ty == shared_int,
        "actual original factory Borrow Int32 entry ABI",
        call,
    )?;
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual_loan }),
        "actual argument unique CallReturn End",
    )?;
    need(
        root.origin == Origin::Source(input.argument)
            && begin.origin == root.origin
            && end.origin == Origin::Source(input.call)
            && [root, begin, end].iter().all(|i| i.block == call.block)
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?
            && g.position(call)? < g.position(end)?,
        "If result Root/Borrow/actual Call/End full order",
        (root, begin, call, end),
    )?;
    let data = g.function.entity(EntityId::Value(phi)).unwrap();
    need(
        matches!(data.definition,Definition::BlockParameter {block,..} if block==call.block),
        "factory argument is actual merge Value parameter",
        data,
    )?;
    let yes = at(g, input.arms[0], |o| {
        *o == Operation::Constant(ScalarConstant::Integer(1))
    })?;
    let no = at(g, input.arms[1], |o| {
        *o == Operation::Constant(ScalarConstant::Integer(2))
    })?;
    need(
        [yes, no]
            .iter()
            .all(|i| g.function.entity(result(i).unwrap()).unwrap().ty == EntityType::Value(int)),
        "both literal arm results have exact Int32 storage",
        (yes, no),
    )?;
    let flag = at(g, input.flag, |o| matches!(o, Operation::Read { .. }))?;
    let Operation::Read {
        source: PlaceAccess::Loan(flag_source),
    } = flag.operation
    else {
        return Err(failure("argument flag reads actual Borrow Boolean", flag));
    };
    let params = &g.block(g.function.entry_block().unwrap())?.parameters;
    let [parameter] = params.as_slice() else {
        return Err(failure("actual scanner one Borrow Boolean entry", params));
    };
    let boolean = g.function.entity(*parameter).unwrap().ty.semantic_type();
    need(
        module.type_kind(boolean) == Some(&SsaTypeKind::Boolean)
            && g.function.entity(*parameter).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: boolean,
                }
            && g.function.entity(result(flag)?).unwrap().ty == EntityType::Value(boolean),
        "actual original scanner Borrow Boolean ABI and Read result",
        flag,
    )?;
    g.anchored(EntityId::Loan(flag_source), &[*parameter])?;
    let term = g.term(flag.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("actual argument If Conditional", term));
    };
    need(
        *condition == value(flag)?
            && term.origin == Origin::Source(input.argument)
            && when_true.target == yes.block
            && when_false.target == no.block
            && yes.block != no.block,
        "true arm is original 1 and false arm original 2",
        term,
    )?;
    g.incoming_from(yes.block, &[flag.block])?;
    g.incoming_from(no.block, &[flag.block])?;
    let y = g.branch(yes.block)?;
    let n = g.branch(no.block)?;
    need(
        y.target == call.block
            && n.target == call.block
            && g.argument(y, EntityId::Value(phi))? == result(yes)?
            && g.argument(n, EntityId::Value(phi))? == result(no)?
            && g.term(yes.block)?.origin == Origin::Source(input.argument)
            && g.term(no.block)?.origin == Origin::Source(input.argument),
        "both exact original literal incoming columns feed actual Call argument Phi",
        (y, n, phi),
    )?;
    g.incoming_from(call.block, &[yes.block, no.block])?;
    g.anchored(EntityId::Value(phi), &[result(yes)?, result(no)?])?;
    let scalar_family = [result(yes)?, result(no)?, EntityId::Value(phi)]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for &e in &scalar_family {
        uses.protect(e);
    }
    uses.permit(EntityId::Value(phi), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(*arg);
    uses.permit(*arg, call);
    uses.permit(*arg, end);
    let flags = family(g, *parameter)?;
    for &e in &flags {
        uses.protect(e);
    }
    let current_flags = g
        .block(call.block)?
        .parameters
        .iter()
        .copied()
        .filter(|e| flags.contains(e))
        .collect::<Vec<_>>();
    match current_flags.as_slice() {
        [] => need(
            [yes.block, no.block].iter().all(|&b| {
                g.block(b)
                    .unwrap()
                    .parameters
                    .iter()
                    .all(|e| !flags.contains(e))
            }),
            "immutable single flag is not fabricated as arm transport",
            &current_flags,
        )?,
        [current] => {
            let yes_flag = g.transported(when_true, *parameter)?;
            let no_flag = g.transported(when_false, *parameter)?;
            need(
                g.argument(y, *current)? == yes_flag && g.argument(n, *current)? == no_flag,
                "Unit argument merge keeps exact flag from both original arm columns",
                (current, yes_flag, no_flag),
            )?;
        }
        _ => {
            return Err(failure(
                "unique original current flag argument column",
                current_flags,
            ));
        }
    }
    uses.permit(EntityId::Loan(flag_source), flag);
    let flag_ends = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            matches!(i.operation, Operation::BorrowEnd { loan }
            if flags.contains(&EntityId::Loan(loan)))
        })
        .collect::<Vec<_>>();
    if let [end] = flag_ends.as_slice() {
        let Operation::BorrowEnd { loan } = end.operation else {
            unreachable!()
        };
        let candidates = g
            .block(end.block)?
            .parameters
            .iter()
            .copied()
            .filter(|e| flags.contains(e))
            .collect::<Vec<_>>();
        let current = match candidates.as_slice() {
            [] => *parameter,
            [current] => *current,
            _ => {
                return Err(failure(
                    "unique current flag at callable Return",
                    candidates,
                ));
            }
        };
        need(
            EntityId::Loan(loan) == current
                && matches!(&g.term(end.block)?.kind, TerminatorKind::Return { values }
                    if values.is_empty()),
            "flag ABI End, when emitted, closes exact current loan only at callable Return",
            end,
        )?;
        uses.permit(current, end);
    } else {
        need(
            flag_ends.is_empty(),
            "flag ABI has no duplicate End sites",
            flag_ends,
        )?;
    }
    for (from, edge) in &g.edges {
        need(
            !edge.arguments.contains(arg) && !edge.arguments.contains(&EntityId::Place(place)),
            "argument synchronous loan/place never escapes merge block",
            edge,
        )?;
        for e in &edge.arguments {
            if scalar_family.contains(e) {
                need(
                    (*from == yes.block && *e == result(yes)?)
                        || (*from == no.block && *e == result(no)?),
                    "argument scalar transport only its original two incoming edges",
                    (from, edge, e),
                )?;
            }
        }
    }
    let read = result(flag)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&read))
            && g.edges.iter().all(|(_, e)| !e.arguments.contains(&read))
            && g.blocks
                .iter()
                .all(|&b| b == flag.block || !g.term(b).unwrap().kind.entities().contains(&read)),
        "flag Read has only actual original Conditional use",
        flag,
    )?;
    // Re-read original spans here so a source/IR mismatch cannot hide behind selected values.
    need(
        sources.slice(input.arms[0]) == Ok("1")
            && sources.slice(input.arms[1]) == Ok("2")
            && parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, n)| n.span() == input.argument)
                .count()
                == 1,
        "same original argument AST identity",
        input.argument,
    )?;
    Ok((call, end))
}
