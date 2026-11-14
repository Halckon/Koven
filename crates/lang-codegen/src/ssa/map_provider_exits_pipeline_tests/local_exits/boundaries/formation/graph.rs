//! Exact current-owner execution, full provider columns and real own delivery.
use super::*;
use crate::ssa::{
    map_provider_exits_pipeline_tests::query_graph::{
        Graph, Uses, function as source_function, loan, result, value,
    },
    model::*,
};
mod conditional {
    //! Original literal condition and exact true/false arm identity, including the unexecuted arm.
    use super::*;
    use lang_frontend::parser::LiteralKind;
    pub(super) fn check(g: &Graph<'_>, parsed: &ParsedFile, input: &Inputs) -> Result<(), Failure> {
        for (_, node) in parsed.ast().expressions().iter() {
            let Expression::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = node.payload()
            else {
                continue;
            };
            if !inside(node.span(), input.site.for_span) {
                continue;
            }
            let condition_node = parsed.ast().expressions().get(*condition).unwrap();
            let Expression::Literal(LiteralKind::Boolean(expected)) = condition_node.payload()
            else {
                return Err(failure(
                    "formation original conditional literal Boolean",
                    condition_node,
                ));
            };
            let boolean = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(condition_node.span())
                        && matches!(i.operation, Operation::Constant(ScalarConstant::Boolean(_)))
                }),
                "formation exact original Boolean condition producer",
            )?;
            need(
                boolean.operation == Operation::Constant(ScalarConstant::Boolean(*expected)),
                "formation original Boolean polarity",
                boolean,
            )?;
            let block=one(g.blocks.iter().copied().filter(|&b|g.term(b).is_ok_and(|t|t.origin==Origin::Source(node.span()) && matches!(t.kind,TerminatorKind::Conditional{condition,..} if condition==value(boolean).unwrap()))),"formation actual original If Conditional")?;
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &g.term(block)?.kind
            else {
                unreachable!()
            };
            let branches = [(*then_branch, when_true, 0), (*else_branch, when_false, 1)];
            let mut exits = vec![];
            for (branch, edge, index) in branches {
                let original = parsed.ast().statements().get(branch).unwrap().span();
                need(
                    g.block(edge.target)?.origin == Origin::Source(original),
                    "formation actual true/false block matches original AST arm",
                    (edge, original),
                )?;
                g.incoming_from(edge.target, &[block])?;
                let call = one(
                    input
                        .uses
                        .iter()
                        .filter(|u| u.print && inside(u.at, original)),
                    "formation original arm print use",
                )?;
                need(
                    matches!(call.args.as_slice(),[arg] if arg.root==Some(input.components[index])),
                    "formation true prints original K and false prints original V",
                    call,
                )?;
                let print = one(
                    g.instructions.iter().copied().filter(|i| {
                        i.origin == Origin::Source(call.at)
                            && matches!(i.operation, Operation::PrintString { .. })
                    }),
                    "formation original arm actual PrintString",
                )?;
                need(
                    print.block == edge.target,
                    "formation print resides in its actual original arm",
                    print,
                )?;
                let outgoing = g.branch(print.block)?;
                exits.push((print.block, outgoing));
            }
            need(
                exits[0].1.target == exits[1].1.target,
                "formation both original arms join one actual continuation",
                &exits,
            )?;
            g.incoming_from(exits[0].1.target, &[exits[0].0, exits[1].0])?;
        }
        Ok(())
    }
}
mod paths;
mod uses {
    //! Allow operands only at independently matched original uses and cleanup endpoints.
    use super::*;

    pub(super) fn check(
        g: &Graph<'_>,
        module: &Module,
        parsed: &ParsedFile,
        input: &Inputs,
        uses: &mut Uses,
        (components, leases): (&[BTreeSet<EntityId>; 2], &BTreeSet<EntityId>),
    ) -> Result<(), Failure> {
        // Protect every actual Map owner, including the separate rebound epoch.
        for &block in &g.blocks {
            for &e in &g.block(block)?.parameters {
                if map_value(module, g, e) {
                    uses.protect(e);
                }
            }
        }
        for i in &g.instructions {
            for &e in &i.results {
                if map_value(module, g, e) {
                    uses.protect(e);
                }
            }
        }
        let source_ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
        let source_end = one(
            source_ends.into_iter(),
            "formation exact sole exhaustion source End",
        )?;
        need(
            source_end.origin == Origin::Source(input.site.for_span),
            "formation source End original cleanup origin",
            source_end,
        )?;
        if let Operation::BorrowEnd { loan } = source_end.operation {
            uses.permit(EntityId::Loan(loan), source_end);
        }
        let mut component_ends = vec![];
        for roots in components {
            let end=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if roots.contains(&EntityId::Loan(loan)))),"formation actual sole component root End")?;
            need(
                end.origin == Origin::Source(input.site.for_span),
                "formation component End exact original For origin",
                end,
            )?;
            if let Operation::BorrowEnd { loan } = end.operation {
                uses.permit(EntityId::Loan(loan), end);
            }
            component_ends.push(end);
        }
        need(
            component_ends[0].block == component_ends[1].block
                && g.position(component_ends[1])? < g.position(component_ends[0])?,
            "formation exact VEnd then KEnd actual cleanup",
            &component_ends,
        )?;
        for call in input.uses.iter().filter(|call| call.print) {
            one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(call.at)
                        && matches!(
                            i.operation,
                            Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                        )
                }),
                "formation each actual original println has exactly one instruction",
            )?;
        }
        for i in &g.instructions {
            match &i.operation {
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } => {
                    let call = one(
                        input.uses.iter().filter(|call| {
                            call.target.is_some() && i.origin == Origin::Source(call.at)
                        }),
                        "formation no unregistered actual source Call",
                    )?;
                    let target = source_function(module, call.target.unwrap())?;
                    need(
                        target.id == *callee && call.args.len() == arguments.len(),
                        "formation actual original Call target and argument count",
                        i,
                    )?;
                    let mut child_ends = vec![];
                    for (&entity, argument) in arguments.iter().zip(&call.args) {
                        argument_use(
                            g,
                            module,
                            input,
                            components,
                            (argument, entity),
                            i,
                            (uses, &mut child_ends),
                        )?;
                    }
                    endpoints(g, i, &child_ends)?;
                }
                Operation::DirectCall { .. } => {
                    return Err(failure("formation unexpected receiver Call", i));
                }
                Operation::PrintString { value } => {
                    let call = one(
                        input
                            .uses
                            .iter()
                            .filter(|call| call.print && i.origin == Origin::Source(call.at)),
                        "formation actual original println Call",
                    )?;
                    let argument = one(
                        call.args.iter(),
                        "formation println original argument slot zero",
                    )?;
                    let mut child_ends = vec![];
                    argument_use(
                        g,
                        module,
                        input,
                        components,
                        (argument, EntityId::Loan(*value)),
                        i,
                        (uses, &mut child_ends),
                    )?;
                    endpoints(g, i, &child_ends)?;
                }
                Operation::MapPut { owner, .. } => {
                    need(
                        input
                            .site
                            .put
                            .is_some_and(|id| i.origin == Origin::Source(span(parsed, id))),
                        "formation only original typed put consumes owner",
                        i,
                    )?;
                    uses.permit(EntityId::Value(*owner), i);
                }
                Operation::Drop { owner } if map_value(module, g, EntityId::Value(*owner)) => {
                    need(
                        input
                            .drops
                            .iter()
                            .any(|d| i.origin == Origin::Source(d.origin)),
                        "formation only same-analysis original Map Drop use",
                        i,
                    )?;
                    uses.permit(EntityId::Value(*owner), i);
                }
                Operation::MapSize { owner } => {
                    let at = match i.origin {
                        Origin::Source(at) => at,
                        _ => return Err(failure("formation MapSize original origin", i)),
                    };
                    let property = one(
                        input.sizes.iter().filter(|(source, _)| *source == at),
                        "formation actual same-analysis size property",
                    )?;
                    let birth = input.site.birth;
                    need(
                        property.1 == input.site.root,
                        "formation size original named source root",
                        property,
                    )?;
                    g.anchored(*owner, &[birth_seed(g, module, input, birth)?])?;
                    uses.permit(*owner, i);
                }
                Operation::StringClone { source } => {
                    let (at, receiver) = one(
                        input
                            .clones
                            .iter()
                            .copied()
                            .filter(|(at, _)| i.origin == Origin::Source(*at)),
                        "formation actual original builtin clone descriptor",
                    )?;
                    let begin = g.definition(EntityId::Loan(*source))?;
                    let Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    } = begin.operation
                    else {
                        return Err(failure("formation actual literal clone BorrowBegin", begin));
                    };
                    let root = g.definition(EntityId::Place(place))?;
                    let Operation::RootPlace { owner } = root.operation else {
                        return Err(failure("formation clone actual literal RootPlace", root));
                    };
                    let producer = g.definition(EntityId::Value(owner))?;
                    need(
                        producer.origin == Origin::Source(receiver)
                            && matches!(producer.operation, Operation::StringLiteral { .. })
                            && begin.origin == Origin::Source(receiver),
                        "formation clone borrows its original literal receiver",
                        (producer, begin),
                    )?;
                    uses.permit(EntityId::Loan(*source), i);
                    let end = exact_end(g, *source, i, (receiver, at))?;
                    uses.permit(EntityId::Loan(*source), end);
                    endpoints(g, i, &[*source])?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn birth_seed(
        g: &Graph<'_>,
        module: &Module,
        input: &Inputs,
        birth: Birth,
    ) -> Result<EntityId, Failure> {
        match birth {
            Birth::Call(id) => {
                original_call(g, module, input, span_of_birth(input, id)?).and_then(result)
            }
            Birth::Parameter(_) => one(
                g.block(g.function.entry_block().unwrap())?
                    .parameters
                    .iter()
                    .copied()
                    .filter(|&p| map_value(module, g, p)),
                "formation exact original owner entry slot",
            ),
        }
    }
    // AST expression IDs are preserved in Inputs; use the corresponding selected
    // source Call span, never a same-typed arbitrary producer.
    fn span_of_birth(
        input: &Inputs,
        id: lang_frontend::ast::ExpressionId,
    ) -> Result<Span, Failure> {
        one(
            input
                .births
                .iter()
                .filter_map(|(original, at)| (*original == id).then_some(*at)),
            "formation original birth expression/span bridge",
        )
    }
    fn argument_use(
        g: &Graph<'_>,
        module: &Module,
        input: &Inputs,
        components: &[BTreeSet<EntityId>; 2],
        (argument, entity): (&Argument, EntityId),
        call: &Instruction,
        registration: (&mut Uses, &mut Vec<LoanId>),
    ) -> Result<(), Failure> {
        let (uses, child_ends) = registration;
        if let Some(index) = argument
            .root
            .and_then(|r| input.components.iter().position(|&s| s == r))
        {
            need(
                argument.borrow && matches!(entity, EntityId::Loan(_)),
                "formation original component uses only its Borrow slot",
                (argument, entity),
            )?;
            component_argument(
                g,
                &components[index],
                argument,
                entity,
                call,
                uses,
                child_ends,
            )?;
        } else if argument.borrow {
            let EntityId::Loan(current) = entity else {
                return Err(failure(
                    "formation actual original Borrow argument ABI",
                    entity,
                ));
            };
            let begin = g.definition(entity)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure(
                    "formation named root actual shared BorrowBegin",
                    begin,
                ));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure(
                    "formation actual original borrowed root place",
                    root,
                ));
            };
            if argument.root.is_none() {
                let bytes = argument.literal.as_ref().ok_or_else(|| {
                    failure(
                        "formation original temporary Borrow must be exact literal",
                        argument,
                    )
                })?;
                let producer = g.definition(EntityId::Value(owner))?;
                need(
                    producer.origin == Origin::Source(argument.at)
                        && matches!(&producer.operation,Operation::StringLiteral{bytes:actual,..} if actual==bytes)
                        && module.type_kind(
                            g.function
                                .entity(EntityId::Value(owner))
                                .unwrap()
                                .ty
                                .semantic_type(),
                        ) == Some(&SsaTypeKind::StringOwner)
                        && root.origin == Origin::Source(argument.at)
                        && producer.block == call.block
                        && root.block == call.block
                        && g.position(producer)? < g.position(root)?
                        && g.position(root)? < g.position(begin)?,
                    "formation exact original literal bytes/String type/span/root before Borrow",
                    (producer, root, begin, call),
                )?;
                // The protected literal owner must be consumed once after its
                // synchronous print loan ends, never passed to another use.
                let drop = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::Drop { owner }),
                    "formation literal owner unique actual Drop",
                )?;
                let (call_return, drop_origin) = argument.literal_drop.ok_or_else(|| {
                    failure(
                        "formation literal original FE Temporary Drop bridge",
                        argument,
                    )
                })?;
                need(
                    call.origin == Origin::Source(call_return)
                        && drop.origin == Origin::Source(drop_origin)
                        && drop_origin == argument.at
                        && drop.block == call.block,
                    "formation literal actual Drop matches original FE CallReturn point/origin",
                    (call, drop, argument),
                )?;
                uses.permit(EntityId::Value(owner), root);
                uses.permit(EntityId::Value(owner), drop);
            } else {
                let birth = argument
                    .birth
                    .ok_or_else(|| failure("formation original named root birth", argument))?;
                g.anchored(
                    EntityId::Value(owner),
                    &[birth_seed(g, module, input, birth)?],
                )?;
                uses.permit(EntityId::Value(owner), root);
            }
            let endpoint = argument.endpoint.ok_or_else(|| {
                failure("formation same-analysis original Borrow endpoint", argument)
            })?;
            need(
                begin.origin == Origin::Source(endpoint.0),
                "formation actual Borrow starts at original argument",
                begin,
            )?;
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), begin);
            uses.permit(entity, call);
            let end = exact_end(g, current, call, endpoint)?;
            uses.permit(entity, end);
            if argument.literal.is_some() {
                let drop = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::Drop { owner }),
                    "formation literal actual Drop after CallReturn",
                )?;
                need(
                    g.position(drop)? == g.position(end)? + 1,
                    "formation original literal CallReturn Drop immediately follows its unique loan End",
                    (end, drop),
                )?;
            }
            child_ends.push(current);
        } else {
            need(
                argument.root == Some(input.site.root) && matches!(entity, EntityId::Value(_)),
                "formation own transfer original source root and Value ABI",
                (argument, entity),
            )?;
            g.anchored(
                entity,
                &[birth_seed(
                    g,
                    module,
                    input,
                    argument
                        .birth
                        .ok_or_else(|| failure("formation own original birth", argument))?,
                )?],
            )?;
            uses.permit(entity, call);
        }
        Ok(())
    }
    fn component_argument(
        g: &Graph<'_>,
        roots: &BTreeSet<EntityId>,
        argument: &Argument,
        entity: EntityId,
        call: &Instruction,
        uses: &mut Uses,
        child_ends: &mut Vec<LoanId>,
    ) -> Result<(), Failure> {
        if roots.contains(&entity) {
            uses.permit(entity, call);
            return Ok(());
        }
        let mut current = entity;
        let mut visited = BTreeSet::new();
        while !roots.contains(&current) {
            need(
                visited.insert(current),
                "formation acyclic actual argument reborrow chain",
                current,
            )?;
            let begin = g.definition(current)?;
            let Operation::SharedReborrow { source } = begin.operation else {
                return Err(failure(
                    "formation argument must descend from its original component",
                    begin,
                ));
            };
            let parent = EntityId::Loan(source);
            need(
                begin.origin == Origin::Source(argument.at),
                "formation original argument reborrow origin",
                begin,
            )?;
            uses.permit(parent, begin);
            if current == entity {
                uses.permit(current, call);
            }
            let EntityId::Loan(loan) = current else {
                return Err(failure("formation actual child Loan result", current));
            };
            let endpoint = argument.endpoint.ok_or_else(|| {
                failure(
                    "formation actual child original synchronous endpoint",
                    argument,
                )
            })?;
            let end = exact_end(g, loan, call, endpoint)?;
            uses.permit(current, end);
            child_ends.push(loan);
            current = parent;
        }
        Ok(())
    }
    fn exact_end<'a>(
        g: &Graph<'a>,
        loan: LoanId,
        call: &Instruction,
        (begin, end): (Span, Span),
    ) -> Result<&'a Instruction, Failure> {
        let endpoint = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan }),
            "formation actual loan unique original CallReturn End",
        )?;
        let producer = g.definition(EntityId::Loan(loan))?;
        need(
            producer.origin == Origin::Source(begin)
                && endpoint.origin == Origin::Source(end)
                && call.origin == Origin::Source(end)
                && producer.block == call.block
                && endpoint.block == call.block
                && g.position(producer)? < g.position(call)?
                && g.position(call)? < g.position(endpoint)?,
            "formation actual synchronous begin/Call/End original spans and order",
            (producer, call, endpoint),
        )?;
        Ok(endpoint)
    }
    fn endpoints(g: &Graph<'_>, call: &Instruction, loans: &[LoanId]) -> Result<(), Failure> {
        for &loan in loans {
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan }),
                "formation actual synchronous unique End",
            )?;
            let operations = g.ops(call.block)?;
            for instruction in &operations[g.position(call)? + 1..g.position(end)?] {
                need(
                    matches!(instruction.operation,Operation::BorrowEnd{loan} if loans.contains(&loan)),
                    "formation original CallReturn End cannot cross another use/print/provider",
                    (call, instruction, end),
                )?;
            }
        }
        Ok(())
    }
}
use std::collections::BTreeSet;

fn family(g: &Graph<'_>, seed: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut set = BTreeSet::from([seed]);
    loop {
        let before = set.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if set.contains(&a) {
                    set.insert(p);
                }
            }
        }
        if before == set.len() {
            break;
        }
    }
    for &e in &set {
        g.anchored(e, &[seed])?;
    }
    Ok(set)
}
fn map_value(module: &Module, g: &Graph<'_>, e: EntityId) -> bool {
    matches!(g.function.entity(e).map(|v|v.ty),Some(EntityType::Value(t)) if module.map_container(t).is_some())
}
fn original_call<'a>(
    g: &Graph<'a>,
    module: &Module,
    input: &Inputs,
    at: Span,
) -> Result<&'a Instruction, Failure> {
    let target = one(
        input
            .calls
            .iter()
            .filter_map(|(s, t)| (*s == at).then_some(*t)),
        "formation same-analysis selected callable",
    )?;
    let callee = source_function(module, target)?;
    one(g.instructions.iter().copied().filter(|i| i.origin==Origin::Source(at)
        && matches!(i.operation,Operation::DirectCall{callee:c,receiver:None,..} if c==callee.id)),
        "formation unique original SSA Call/selected callee")
}

pub(in super::super::super::super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    input: &Inputs,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "formation original module")?;
    let (entry_span, _) = super::function(sources, parsed, "entry")?;
    let entry_function = source_function(module, entry_span)?;
    crate::ssa::map_provider_exits_pipeline_tests::identity::entry_sources(
        sources,
        program,
        entry_function.id,
        &[("entry", entry_span), ("scan", input.site.scan)],
    )?;
    let scanner = source_function(module, input.site.scan)?;
    let g = Graph::new(scanner)?;
    let mut uses = Uses::default();
    for &(at, _) in &input.calls {
        original_call(&g, module, input, at)?;
    }
    let entry = scanner.entry_block().unwrap();
    let seed = match input.site.birth {
        Birth::Call(id) => {
            let i = original_call(&g, module, input, span(parsed, id))?;
            need(
                matches!(&i.operation,Operation::DirectCall{arguments,receiver:None,..} if arguments.is_empty())
                    && map_value(module, &g, result(i)?),
                "formation actual zeroarg owned source birth",
                i,
            )?;
            result(i)?
        }
        Birth::Parameter(marker) => {
            need(
                marker == input.site.root || input.site.aliases.iter().any(|(_, s)| *s == marker),
                "formation exact moved parameter marker",
                marker,
            )?;
            let p = one(
                g.block(entry)?.parameters.iter().copied(),
                "formation actual owning entry slot zero",
            )?;
            need(
                map_value(module, &g, p),
                "formation owning parameter Value ABI",
                p,
            )?;
            p
        }
    };
    let original_seed = seed;
    let seed = if let Some(id) = input.site.put {
        let put = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span(parsed, id))
                    && matches!(i.operation, Operation::MapPut { .. })
            }),
            "formation unique actual typed put",
        )?;
        let Operation::MapPut { owner, key, value } = put.operation else {
            unreachable!()
        };
        g.anchored(EntityId::Value(owner), &[seed])?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            arguments.len() == 2,
            "formation original put arguments",
            arguments,
        )?;
        for (&v, a) in [key, value].iter().zip(arguments) {
            let producer = g.definition(EntityId::Value(v))?;
            need(
                producer.origin == Origin::Source(span(parsed, a.value))
                    && matches!(
                        producer.operation,
                        Operation::StringClone { .. } | Operation::HeapAllocate { .. }
                    ),
                "formation exact actual put K/V owning producers",
                producer,
            )?;
        }
        need(
            map_value(module, &g, result(put)?),
            "formation put returns actual new Map owner",
            put,
        )?;
        result(put)?
    } else {
        seed
    };
    let owners = family(&g, seed)?;
    let original_owners = family(&g, original_seed)?;
    if seed != original_seed {
        need(
            owners.is_disjoint(&original_owners),
            "formation put output is distinct from consumed input",
            (&owners, &original_owners),
        )?;
    }
    for &owner in owners.iter().chain(&original_owners) {
        uses.protect(owner);
    }
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(input.site.for_span) && predicate(&i.operation)
            }),
            "formation actual unique provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let begin = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("formation actual source shared BorrowBegin", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("formation actual source RootPlace", root));
    };
    g.anchored(EntityId::Value(owner), &[seed])?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    let leases = family(&g, EntityId::Loan(source))?;
    let mut cursors = family_with(&g, result(first)?, &[result(first)?, result(next)?])?;
    // Next has a new cursor producer, yet every header/backedge must be rooted
    // in First or this exact Next, with no unrelated cursor producer.
    cursors.extend(family_with(
        &g,
        result(next)?,
        &[result(first)?, result(next)?],
    )?);
    for &e in &leases {
        uses.protect(e);
    }
    for &e in &cursors {
        uses.protect(e);
    }
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = has.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationNext {
        source: ns,
        cursor: nc,
    } = next.operation
    else {
        unreachable!()
    };
    g.anchored(EntityId::Loan(ns), &[EntityId::Loan(bs)])?;
    g.anchored(EntityId::Value(nc), &[EntityId::Value(bc)])?;
    let pre = g.branch(first.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("formation real has-next condition", has));
    };
    need(
        pre.target == has.block
            && *condition == value(has)?
            && when_true.target == key.block
            && g.transported(pre, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(pre, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "formation precise source/cursor entry and ordered K/V",
        (pre, when_true),
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let back = g.branch(next.block)?;
    need(
        back.target == has.block
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(ns)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "formation exact Next current backedge",
        back,
    )?;
    for (l, i) in [(source, first), (hs, has), (bs, key), (bs, val), (ns, next)] {
        uses.permit(EntityId::Loan(l), i);
    }
    uses.permit(EntityId::Value(hc), has);
    for i in [key, val] {
        uses.permit(EntityId::Value(bc), i);
    }
    uses.permit(EntityId::Value(nc), next);
    let components = [result(key)?, result(val)?];
    need(
        components[0] != components[1] && key.block == val.block,
        "formation independent actual K/V",
        components,
    )?;
    let component_family = [family(&g, components[0])?, family(&g, components[1])?];
    need(
        component_family[0].is_disjoint(&component_family[1]),
        "formation independent original K/V transport families",
        &component_family,
    )?;
    for set in &component_family {
        for &entity in set {
            uses.protect(entity);
        }
    }
    uses::check(
        &g,
        module,
        parsed,
        input,
        &mut uses,
        (&component_family, &leases),
    )?;
    conditional::check(&g, parsed, input)?;
    super::representation::graph(module, &g, input, seed, next)?;
    let mut owner_births = BTreeSet::from([original_seed, seed]);
    for &(at, _) in &input.calls {
        let call = original_call(&g, module, input, at)?;
        for &entity in &call.results {
            if map_value(module, &g, entity) {
                owner_births.insert(entity);
            }
        }
    }
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &owner_births,
    )?;
    paths::check(
        &g,
        module,
        input,
        parsed,
        seed,
        (&leases, &component_family),
        (next, has, first),
    )?;
    uses.check(&g, None)
}
fn family_with(
    g: &Graph<'_>,
    seed: EntityId,
    anchors: &[EntityId],
) -> Result<BTreeSet<EntityId>, Failure> {
    let mut set = BTreeSet::from([seed]);
    loop {
        let before = set.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if set.contains(&a) {
                    set.insert(p);
                }
            }
        }
        if before == set.len() {
            break;
        }
    }
    for &e in &set {
        g.anchored(e, anchors)?;
    }
    Ok(set)
}
