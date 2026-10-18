//! Actual selected callees, ordered Shared ABI, and caller owner restoration.
use super::*;

pub(super) fn provider_calls(
    parsed: &ParsedFile,
    module: &Module,
    g: &Graph<'_>,
    inputs: &Inputs,
    layers: &[Layer<'_>],
    uses: &mut Uses,
) -> Result<(), Failure> {
    let Origin::Source(scope) = g.function.origin else {
        return Err(failure("stack source callable origin", g.function.id));
    };
    for call in inputs.calls.iter().filter(|c| facts::inside(c.span, scope)) {
        let target = function(module, call.callee)?;
        let actual = one(
            g.instructions.iter().copied().filter(|i| {
                at(i, call.span) && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "stack exact original source Call once",
        )?;
        synchronous_loans(g, call, actual)?;
        let Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } = &actual.operation
        else {
            unreachable!()
        };
        let entry = target.block(target.entry_block().unwrap()).unwrap();
        need(
            *callee == target.id
                && receiver.is_none()
                && arguments.len() == call.arguments.len()
                && entry.parameters.len() == arguments.len(),
            "stack actual callee/arity/ABI",
            actual,
        )?;
        for (slot, (&(argument_span, marker, borrow), &argument)) in
            call.arguments.iter().zip(arguments).enumerate()
        {
            need(
                g.function.entity(argument).unwrap().ty
                    == target.entity(entry.parameters[slot]).unwrap().ty,
                "stack ordered argument represented ABI",
                (slot, actual),
            )?;
            if let Some((layer, role)) = marker.and_then(|m| {
                layers.iter().find_map(|l| {
                    l.site
                        .ast
                        .names
                        .iter()
                        .position(|&n| n == m)
                        .map(|r| (l, r))
                })
            }) {
                need(
                    borrow && layer.components[role].contains(&argument),
                    "stack actual original K/V argument cannot exchange roles",
                    (call, actual, slot),
                )?;
                if let Ok(producer) = g.definition(argument)
                    && matches!(producer.operation, Operation::SharedReborrow { .. })
                {
                    need(
                        at(producer, argument_span),
                        "stack original argument Shared reborrow span",
                        producer,
                    )?;
                }
                uses.permit(argument, actual);
            }
        }
    }
    for i in &g.instructions {
        let Operation::StringEqual { left, right } = i.operation else {
            continue;
        };
        let binary = one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, n)| at(i, n.span()).then_some(n)),
            "stack StringEqual exact original AST expression",
        )?;
        let Expression::Binary {
            left: lhs,
            operator: lang_frontend::parser::BinaryOperator::Equal,
            right: rhs,
            ..
        } = binary.payload()
        else {
            return Err(failure("stack original component equality", binary));
        };
        let left_ast = parsed.ast().expressions().get(*lhs).unwrap();
        let right_ast = parsed.ast().expressions().get(*rhs).unwrap();
        let name = one(
            inputs.names.iter().filter(|n| n.span == left_ast.span()),
            "stack condition exact original Name reference",
        )?;
        let (layer, role) = one(
            layers.iter().filter_map(|l| {
                l.site
                    .ast
                    .names
                    .iter()
                    .position(|&marker| marker == name.marker)
                    .map(|r| (l, r))
            }),
            "stack condition actual K/V role from same FE",
        )?;
        need(
            layer.components[role].contains(&left),
            "stack equality left operand is exact original component",
            (i, name),
        )?;
        let literal = g.definition(right)?;
        need(
            at(literal, right_ast.span())
                && matches!(literal.operation, Operation::StringLiteral { .. })
                && result(literal)? == right,
            "stack equality right operand is exact original literal producer",
            (i, literal, right_ast),
        )?;
        uses.permit(left, i);
        let TerminatorKind::Conditional { condition, .. } = g.term(i.block)?.kind else {
            return Err(failure("stack equality actual conditional branch", i));
        };
        need(
            EntityId::Value(condition) == result(i)?,
            "stack condition branch uses actual equality result",
            i,
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Root {
    marker: Span,
    born: EntityId,
    owners: BTreeSet<EntityId>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CallerState {
    owners: Vec<Option<EntityId>>,
    loans: BTreeMap<EntityId, usize>,
}

pub(super) fn callers(
    _sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    inputs: &Inputs,
) -> Result<(), Failure> {
    for f in &module.functions {
        let Origin::Source(scope) = f.origin else {
            continue;
        };
        let calls = inputs
            .calls
            .iter()
            .filter(|c| facts::inside(c.span, scope))
            .collect::<Vec<_>>();
        if !calls
            .iter()
            .any(|c| inputs.sites.iter().any(|s| s.ast.function == c.callee))
        {
            continue;
        }
        let g = Graph::with_abort(f)?;
        let mut roots = Vec::new();
        for (_, node) in parsed.ast().items().iter() {
            let Item::Variable {
                name: NameMarker::Present(marker),
                initializer: expr,
                ..
            } = node.payload()
            else {
                continue;
            };
            if !facts::inside(node.span(), scope) {
                continue;
            }
            let span = parsed.ast().expressions().get(*expr).unwrap().span();
            let Some(call) = calls.iter().find(|c| c.span == span) else {
                continue;
            };
            let target = function(module, call.callee)?;
            let actual=one(g.instructions.iter().copied().filter(|i|at(i,span)&&matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==target.id&&arguments.len()==call.arguments.len())),"stack caller factory selected once")?;
            if actual.results.len() != 1 {
                continue;
            }
            let born = result(actual)?;
            if module
                .map_container(f.entity(born).unwrap().ty.semantic_type())
                .is_none()
            {
                continue;
            }
            let mut anchors = vec![born];
            loop {
                let all = family(&g, &anchors)?;
                let before = anchors.len();
                for i in &g.instructions {
                    if matches!(i.operation,Operation::MapPut{owner,..} if all.contains(&EntityId::Value(owner)))
                    {
                        let new = result(i)?;
                        if !anchors.contains(&new) {
                            anchors.push(new);
                        }
                    }
                }
                if before == anchors.len() {
                    roots.push(Root {
                        marker: *marker,
                        born,
                        owners: all,
                    });
                    break;
                }
            }
        }
        // The original tiny empty entry calls are also observed, even where no
        // caller root exists (make() is nested directly in scan(make())).
        for call in &calls {
            let target = function(module, call.callee)?;
            let actual = one(
                g.instructions.iter().copied().filter(|i| {
                    at(i, call.span) && matches!(i.operation, Operation::DirectCall { .. })
                }),
                "stack caller original Call unique",
            )?;
            need(
                matches!(&actual.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==target.id&&arguments.len()==call.arguments.len()),
                "stack caller exact selected declaration",
                actual,
            )?;
            synchronous_loans(&g, call, actual)?;
        }
        if roots.is_empty() {
            continue;
        }
        let entry = f.entry_block().unwrap();
        let mut pending = vec![(
            entry,
            CallerState {
                owners: vec![None; roots.len()],
                loans: BTreeMap::new(),
            },
        )];
        let mut visited = BTreeSet::new();
        while let Some((block, mut state)) = pending.pop() {
            if !visited.insert((block, state.clone())) {
                continue;
            }
            for i in g.ops(block)? {
                for (index, root) in roots.iter().enumerate() {
                    if i.results.contains(&root.born) {
                        need(
                            state.owners[index].is_none(),
                            "stack caller factory births one owner per path",
                            i,
                        )?;
                        state.owners[index] = Some(root.born);
                    }
                }
                match &i.operation {
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    } => {
                        let root = g.definition(EntityId::Place(*place))?;
                        if let Operation::RootPlace { owner } = root.operation
                            && let Some(index) = roots
                                .iter()
                                .position(|r| r.owners.contains(&EntityId::Value(owner)))
                        {
                            need(
                                state.owners[index] == Some(EntityId::Value(owner)),
                                "stack caller Borrow uses current owner",
                                i,
                            )?;
                            need(
                                state.loans.insert(result(i)?, index).is_none(),
                                "stack caller distinct Borrow birth",
                                i,
                            )?;
                        }
                    }
                    Operation::SharedReborrow { source } => {
                        if let Some(&root) = state.loans.get(&EntityId::Loan(*source)) {
                            state.loans.insert(result(i)?, root);
                        }
                    }
                    Operation::BorrowEnd { loan } => {
                        if state.loans.contains_key(&EntityId::Loan(*loan)) {
                            state.loans.remove(&EntityId::Loan(*loan));
                        }
                    }
                    Operation::MapPut { owner, .. } => {
                        if let Some(index) = roots
                            .iter()
                            .position(|r| r.owners.contains(&EntityId::Value(*owner)))
                        {
                            need(
                                state.owners[index] == Some(EntityId::Value(*owner))
                                    && !state.loans.values().any(|&r| r == index),
                                "stack Mutable put after all caller loans end",
                                i,
                            )?;
                            let new = result(i)?;
                            need(
                                roots[index].owners.contains(&new),
                                "stack MapPut actual new owner identity",
                                i,
                            )?;
                            state.owners[index] = Some(new);
                        }
                    }
                    Operation::MapSize { owner } => {
                        if let Some(index) = roots.iter().position(|r| r.owners.contains(owner)) {
                            need(
                                state.owners[index] == Some(*owner),
                                "stack size reads current post-put owner",
                                i,
                            )?;
                        }
                    }
                    Operation::DirectCall { arguments, .. } => {
                        let call = one(
                            calls.iter().copied().filter(|c| at(i, c.span)),
                            "stack caller Call FE anchor",
                        )?;
                        for (&arg, &(_, marker, borrow)) in arguments.iter().zip(&call.arguments) {
                            let Some(index) =
                                marker.and_then(|m| roots.iter().position(|r| r.marker == m))
                            else {
                                continue;
                            };
                            if borrow {
                                need(
                                    state.loans.get(&arg) == Some(&index)
                                        && state.owners[index].is_some(),
                                    "stack original caller Borrow argument identity",
                                    (i, arg),
                                )?;
                            } else {
                                need(
                                    state.owners[index] == Some(arg)
                                        && !state.loans.values().any(|&r| r == index),
                                    "stack original caller Own transfer identity",
                                    (i, arg),
                                )?;
                                state.owners[index] = None;
                            }
                        }
                    }
                    Operation::Drop { owner } => {
                        if let Some(index) = roots
                            .iter()
                            .position(|r| r.owners.contains(&EntityId::Value(*owner)))
                        {
                            need(
                                state.owners[index] == Some(EntityId::Value(*owner))
                                    && !state.loans.values().any(|&r| r == index),
                                "stack caller sole current owner Drop after lease restoration",
                                i,
                            )?;
                            state.owners[index] = None;
                        }
                    }
                    _ => {}
                }
            }
            match &g.term(block)?.kind {
                TerminatorKind::Return { .. } => need(
                    state.owners.iter().all(Option::is_none) && state.loans.is_empty(),
                    "stack caller finite Return has delivered/dropped every source once",
                    &state,
                )?,
                TerminatorKind::Abort => {}
                _ => {
                    for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                        let mut next = state.clone();
                        for owner in next.owners.iter_mut().flatten() {
                            *owner = transport(&g, edge, *owner)?;
                        }
                        next.loans = next
                            .loans
                            .iter()
                            .map(|(&loan, &root)| Ok((transport(&g, edge, loan)?, root)))
                            .collect::<Result<_, Failure>>()?;
                        pending.push((edge.target, next));
                    }
                }
            }
        }
    }
    Ok(())
}
fn transport(g: &Graph<'_>, edge: &Edge, entity: EntityId) -> Result<EntityId, Failure> {
    if edge.arguments.contains(&entity) {
        g.transported(edge, entity)
    } else {
        Ok(entity)
    }
}

// A forwarded existing component/parameter loan has no new SSA lifetime to
// end. Only a real argument-born BorrowBegin/SharedReborrow is checked here.
fn synchronous_loans(
    g: &Graph<'_>,
    call: &facts::Call,
    actual: &Instruction,
) -> Result<(), Failure> {
    let Operation::DirectCall { arguments, .. } = &actual.operation else {
        return Err(failure("stack actual synchronous source Call", actual));
    };
    need(
        call.loans.len() == arguments.len(),
        "stack full original Call loan endpoint slots",
        call,
    )?;
    let mut fresh = Vec::new();
    for (&argument, endpoint) in arguments.iter().zip(&call.loans) {
        let Some(endpoint) = endpoint else {
            continue;
        };
        let EntityId::Loan(loan) = argument else {
            return Err(failure(
                "stack actual Borrow argument loan",
                (call, actual, argument),
            ));
        };
        let Ok(birth) = g.definition(argument) else {
            continue;
        };
        if !matches!(
            birth.operation,
            Operation::BorrowBegin { .. } | Operation::SharedReborrow { .. }
        ) {
            continue;
        }
        need(
            at(birth, endpoint.begin)
                && birth.block == actual.block
                && g.position(birth)? < g.position(actual)?,
            "stack actual new argument loan begins at original FE argument before Call",
            (call, birth, actual),
        )?;
        need(
            !fresh.contains(&loan),
            "stack real argument-born loan identity is unique",
            actual,
        )?;
        fresh.push(loan);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan }),
            "stack actual new argument has one synchronous End",
        )?;
        need(
            at(end, endpoint.end)
                && end.block == actual.block
                && g.position(actual)? < g.position(end)?,
            "stack new argument End has exact original FE CallReturn endpoint",
            (call, actual, end),
        )?;
    }
    if fresh.is_empty() {
        return Ok(());
    }
    let operations = g.ops(actual.block)?;
    let mut remaining = fresh;
    for instruction in operations.iter().skip(g.position(actual)? + 1) {
        if remaining.is_empty() {
            break;
        }
        let Operation::BorrowEnd { loan } = instruction.operation else {
            return Err(failure(
                "stack new synchronous loan cannot cross a later operation",
                (actual, instruction, remaining),
            ));
        };
        let index = one(
            remaining
                .iter()
                .enumerate()
                .filter_map(|(index, &id)| (id == loan).then_some(index)),
            "stack CallReturn ends only its actual new argument loans",
        )?;
        remaining.remove(index);
    }
    need(
        remaining.is_empty(),
        "stack all real new argument loans end at this CallReturn boundary",
        (actual, remaining),
    )
}
