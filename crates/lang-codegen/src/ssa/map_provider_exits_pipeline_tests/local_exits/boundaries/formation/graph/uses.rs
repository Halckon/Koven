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
fn span_of_birth(input: &Inputs, id: lang_frontend::ast::ExpressionId) -> Result<Span, Failure> {
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
        let endpoint = argument
            .endpoint
            .ok_or_else(|| failure("formation same-analysis original Borrow endpoint", argument))?;
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
