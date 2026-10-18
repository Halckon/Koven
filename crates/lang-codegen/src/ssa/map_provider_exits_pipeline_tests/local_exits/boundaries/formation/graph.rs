//! Exact current-owner execution, full provider columns and real own delivery.
use super::*;
use crate::ssa::{
    map_provider_exits_pipeline_tests::query_graph::{
        Graph, Uses, function as source_function, loan, result, value,
    },
    model::*,
};
mod paths;
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
    let mut component_family = [family(&g, components[0])?, family(&g, components[1])?];
    // Ordinary calls may either forward a component or establish a temporary
    // child reborrow. Only actual child instructions create extra endpoints.
    for i in &g.instructions {
        if let Operation::SharedReborrow { source } = i.operation {
            for set in &mut component_family {
                if set.contains(&EntityId::Loan(source)) {
                    set.insert(result(i)?);
                }
            }
        }
    }
    need(
        component_family[0].is_disjoint(&component_family[1]),
        "formation K/V families never mix",
        &component_family,
    )?;
    for set in &component_family {
        for &e in set {
            uses.protect(e);
        }
    }
    for i in &g.instructions {
        match &i.operation {
            Operation::RootPlace { owner }
                if owners.contains(&EntityId::Value(*owner))
                    || original_owners.contains(&EntityId::Value(*owner)) =>
            {
                uses.permit(EntityId::Value(*owner), i)
            }
            Operation::MapPut { owner, .. }
                if original_owners.contains(&EntityId::Value(*owner)) =>
            {
                uses.permit(EntityId::Value(*owner), i)
            }
            Operation::Drop { owner }
                if owners.contains(&EntityId::Value(*owner))
                    || original_owners.contains(&EntityId::Value(*owner)) =>
            {
                uses.permit(EntityId::Value(*owner), i)
            }
            Operation::BorrowEnd { loan } if leases.contains(&EntityId::Loan(*loan)) => {
                uses.permit(EntityId::Loan(*loan), i)
            }
            Operation::SharedReborrow { source } => {
                uses.permit(EntityId::Loan(*source), i);
            }
            Operation::BorrowEnd { loan } => {
                uses.permit(EntityId::Loan(*loan), i);
            }
            Operation::PrintString { value } => {
                uses.permit(EntityId::Loan(*value), i);
            }
            Operation::DirectCall { arguments, .. } => {
                for &e in arguments {
                    uses.permit(e, i);
                }
            }
            _ => {}
        }
    }
    for i in &g.instructions {
        if let Operation::MapSize { owner } = i.operation {
            let Origin::Source(at) = i.origin else {
                return Err(failure("formation actual MapSize origin", i));
            };
            let property = one(
                parsed.ast().expressions().iter().filter_map(|(id, n)| {
                    (n.span() == at && matches!(n.payload(), Expression::Member { .. }))
                        .then_some(id)
                }),
                "formation actual original size property",
            )?;
            need(
                inside(span(parsed, property), input.site.scan)
                    && (owners.contains(&owner) || original_owners.contains(&owner)),
                "formation size belongs to original scanner",
                i,
            )?;
            uses.permit(owner, i);
        }
    }
    if input.site.sink.is_none() {
        let read = one(
            input.calls.iter().filter_map(|(at, target)| {
                function(sources, parsed, "read")
                    .ok()
                    .filter(|(s, _)| s == target)
                    .map(|_| *at)
            }),
            "formation actual component reader Call",
        )?;
        let call = original_call(&g, module, input, read)?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        need(
            arguments.len() == 2
                && arguments
                    .iter()
                    .enumerate()
                    .all(|(i, e)| component_family[i].contains(e)),
            "formation reader ordered original K/V ABI",
            call,
        )?;
    } else {
        for (i, set) in component_family.iter().enumerate() {
            let name = if i == 0 { "key" } else { "value" };
            let argument=one(parsed.ast().expressions().iter().filter_map(|(_,n)|{
                let Expression::Call{callee,arguments,..}=n.payload() else{return None};
                (inside(n.span(),input.site.for_span) && sources.slice(span(parsed,*callee))==Ok("println")
                    && matches!(arguments.as_slice(),[a] if sources.slice(span(parsed,a.value))==Ok(name))).then_some(n.span())
            }),"formation actual conditional print Call")?;
            one(g.instructions.iter().copied().filter(|v|v.origin==Origin::Source(argument)
                && matches!(v.operation,Operation::PrintString{value} if set.contains(&EntityId::Loan(value)))),"formation conditional arm prints its original component")?;
        }
    }
    super::representation::graph(module, &g, input, seed, next)?;
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
