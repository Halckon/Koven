//! Concrete Int storage, actual Copy reads, and complete current-owner transport.
mod factory {
    //! makeInt really constructs, inserts the two scalar payloads, consumes, and returns that owner.
    use super::*;
    pub(super) fn check(
        p: &ParsedFile,
        i: &ast::Input,
        module: &Module,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let f = function(module, i.make)?;
        let g = Graph::new(f)?;
        let construct = at(&g, ast::span(p, i.construction), |op| {
            matches!(op, Operation::MapConstruct { .. })
        })?;
        let put = at(&g, ast::span(p, i.put), |op| {
            matches!(op, Operation::MapPut { .. })
        })?;
        let consume = at(&g, ast::span(p, i.consume), |op| {
            matches!(op, Operation::MapConsume { .. })
        })?;
        let Operation::MapConstruct { map_type: mutable } = construct.operation else {
            unreachable!()
        };
        let Operation::MapPut {
            owner: initial,
            key,
            value: payload,
        } = put.operation
        else {
            unreachable!()
        };
        let (_, kt, vt) = module.map_container(map).unwrap();
        need(
            module.map_container(mutable) == Some((MapContainerKind::MutableMap, kt, vt))
                && initial == value(construct)?
                && consume.operation
                    == Operation::MapConsume {
                        owner: value(put)?,
                        map_type: map,
                    }
                && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
                && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable)
                && f.entity(result(consume)?).unwrap().ty == EntityType::Value(map)
                && g.blocks.len() == 1
                && g.edges.is_empty()
                && g.position(construct)? < g.position(put)?
                && g.position(put)? < g.position(consume)?
                && g.term(consume.block)?.origin == Origin::Source(ast::span(p, i.returned))
                && matches!(&g.term(consume.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(consume)?]),
            "Int actual original construction/updated owner/consume/current Return chain",
            (construct, put, consume),
        )?;
        let mut uses = Uses::default();
        for e in [result(construct)?, result(put)?, result(consume)?] {
            uses.protect(e);
        }
        uses.permit(result(construct)?, put);
        uses.permit(result(put)?, consume);
        for ((id, expression), expected) in [(key, i.payload[0]), (payload, i.payload[1])]
            .into_iter()
            .zip([1, 2])
        {
            let literal = g.definition(EntityId::Value(id))?;
            need(
                literal.operation == Operation::Constant(ScalarConstant::Integer(expected))
                    && literal.origin == Origin::Source(ast::span(p, expression))
                    && f.entity(EntityId::Value(id)).unwrap().ty == EntityType::Value(kt),
                "Int actual exact put scalar bytes/value/type",
                literal,
            )?;
            uses.protect(EntityId::Value(id));
            uses.permit(EntityId::Value(id), put);
        }
        for (index, data) in f.values.iter().enumerate() {
            if module.map_container(data.ty.semantic_type()).is_some() {
                need(
                    [result(construct)?, result(put)?, result(consume)?].contains(
                        &EntityId::Value(ValueId {
                            function: f.id,
                            index,
                        }),
                    ),
                    "Int all makeInt Map Values are this actual owner evolution",
                    (index, data),
                )?;
            }
        }
        need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapConstruct{..})).count()==1&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapPut{..})).count()==1&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapConsume{..})).count()==1&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())),"Int no stale owner cleanup or substitute factory construction",f.id)?;
        uses.check(&g, Some((consume.block, value(consume)?)))
    }
}
mod provider {
    //! Every Int-provider owner/source/cursor column and both normal endpoints are actual.
    use super::super::super::super::{births::ast::Site, query_map_normal_body as body};
    use super::*;
    pub(super) fn check(
        g: &Graph<'_>,
        module: &Module,
        reader: &Function,
        p: &ParsedFile,
        selected: (&Site, EntityId, &Instruction),
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (site, seed, drop) = selected;
        let EntityId::Value(owner) = seed else {
            return Err(failure("Int actual source is owner Value", seed));
        };
        let map = g.function.entity(seed).unwrap().ty.semantic_type();
        let (_, kt, vt) = module.map_container(map).unwrap();
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::RootPlace { owner }),
            "Int actual current source root",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("Int actual source Place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|op| {
                op.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "Int actual source Shared lease",
        )?;
        let first = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationFirst { .. })
        })?;
        let has = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationHasNext { .. })
        })?;
        let source = loan(begin)?;
        let cursor = value(first)?;
        need(
            first.operation == Operation::MapIterationFirst { source }
                && [root, begin, first].iter().all(|op| {
                    op.origin == Origin::Source(site.span)
                        && op.block == g.definition(seed).unwrap().block
                })
                && g.position(g.definition(seed)?)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(first)?,
            "Int actual birth/Root/Shared/First sequence",
            (root, begin, first),
        )?;
        uses.permit(seed, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(source), first);
        let via = |frame: [EntityId; 3], edge: &Edge| -> Result<[EntityId; 3], Failure> {
            need(
                edge.arguments.len() == 3
                    && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                        == frame.into_iter().collect(),
                "Int complete current three-column transport",
                (frame, edge),
            )?;
            Ok([
                g.transported(edge, frame[0])?,
                g.transported(edge, frame[1])?,
                g.transported(edge, frame[2])?,
            ])
        };
        let initial = [seed, EntityId::Loan(source), EntityId::Value(cursor)];
        let header = via(initial, g.branch(first.block)?)?;
        need(
            g.branch(first.block)?.target == has.block
                && has.operation
                    == Operation::MapIterationHasNext {
                        source: match header[1] {
                            EntityId::Loan(l) => l,
                            _ => unreachable!(),
                        },
                        cursor: match header[2] {
                            EntityId::Value(v) => v,
                            _ => unreachable!(),
                        },
                    },
            "Int actual current header source/cursor",
            has,
        )?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("Int actual HasNext conditional", has));
        };
        need(
            *condition == value(has)? && g.ops(has.block)?.len() == 1,
            "Int sole actual header guard",
            has,
        )?;
        let current = via(header, when_true)?;
        let exit = via(header, when_false)?;
        let EntityId::Loan(body_source) = current[1] else {
            unreachable!()
        };
        let EntityId::Value(body_cursor) = current[2] else {
            unreachable!()
        };
        let next = body::check(
            g,
            reader,
            body::Body {
                block: when_true.target,
                span: site.span,
                read: ast::span(p, site.read),
                arguments: site.arguments.map(|id| ast::span(p, id)),
                types: [kt, vt],
            },
            body_source,
            body_cursor,
            uses,
        )?;
        let back = g.branch(next.block)?;
        need(
            back.target == has.block
                && via([current[0], current[1], result(next)?], back)? == header,
            "Int exact current Next/source/owner backedge",
            back,
        )?;
        g.incoming_from(has.block, &[first.block, next.block])?;
        g.incoming_from(when_true.target, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        need(
            g.blocks.iter().copied().collect::<BTreeSet<_>>()
                == [first.block, has.block, next.block, when_false.target]
                    .into_iter()
                    .collect(),
            "Int complete actual provider CFG closure",
            &g.blocks,
        )?;
        for frame in [initial, header, current, exit] {
            for e in frame {
                uses.protect(e);
            }
            g.anchored(frame[0], &[seed])?;
            g.anchored(frame[1], &[EntityId::Loan(source)])?;
            g.anchored(frame[2], &[EntityId::Value(cursor), result(next)?])?;
            need(
                g.function.entity(frame[0]).unwrap().ty == EntityType::Value(map)
                    && g.function.entity(frame[1]).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map,
                        }
                    && matches!(g.function.entity(frame[2]).unwrap().ty,EntityType::Value(ty)if module.type_kind(ty)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
                "Int every current owner/source/cursor column type",
                frame,
            )?;
        }
        for e in [header[1], header[2]] {
            uses.permit(e, has);
        }
        let end = one(
            g.ops(when_false.target)?.into_iter().filter(
                |op| matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==exit[1]),
            ),
            "Int actual exhaustion EndSource",
        )?;
        let ops = g.ops(end.block)?;
        need(
            end.origin == Origin::Source(site.span)
                && drop.operation
                    == Operation::Drop {
                        owner: match exit[0] {
                            EntityId::Value(v) => v,
                            _ => unreachable!(),
                        },
                    }
                && drop.block == end.block
                && ops.first().map(|op| op.id) == Some(end.id)
                && ops.get(1).map(|op| op.id) == Some(drop.id)
                && ops.len() == 2
                && matches!(&g.term(end.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
            "Int current source End/current owner Drop/void Return exact terminal order",
            (end, drop),
        )?;
        uses.permit(exit[1], end);
        Ok(())
    }
}
mod reader {
    //! The reader copies actual Int loans, compares the real K/V values and guards success output.
    use super::*;
    fn prints(g: &Graph<'_>, op: &Instruction) -> bool {
        match &op.operation {
            Operation::PrintLiteral { bytes } => bytes == b"int\n",
            Operation::PrintString { value } => {
                let Ok(begin) = g.definition(EntityId::Loan(*value)) else {
                    return false;
                };
                let Operation::BorrowBegin { place, .. } = begin.operation else {
                    return false;
                };
                let Ok(root) = g.definition(EntityId::Place(place)) else {
                    return false;
                };
                let Operation::RootPlace { owner } = root.operation else {
                    return false;
                };
                g.definition(EntityId::Value(owner)).is_ok_and(
                    |op| matches!(&op.operation,Operation::StringLiteral{bytes,..}if bytes==b"int"),
                )
            }
            _ => false,
        }
    }
    pub(super) fn check(
        p: &ParsedFile,
        i: &ast::Input,
        module: &Module,
        int: SsaTypeId,
    ) -> Result<(), Failure> {
        let f = function(module, i.reader)?;
        let g = Graph::with_abort(f)?;
        let parameters = &g.block(f.entry_block().unwrap())?.parameters;
        need(
            parameters.len() == 2
                && f.return_types.is_empty()
                && f.receiver.is_none()
                && parameters.iter().all(|e| {
                    f.entity(*e).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: int,
                        }
                }),
            "Int actual reader declared Shared parameters with Copy target storage",
            parameters,
        )?;
        let mut uses = Uses::default();
        let mut conditions = Vec::new();
        for (index, &parameter) in parameters.iter().enumerate() {
            let compare = at(&g, ast::span(p, i.compares[index]), |op| {
                matches!(op, Operation::Compare { .. })
            })?;
            let Operation::Compare {
                operator: ComparisonOperator::Equal,
                left,
                right,
            } = compare.operation
            else {
                return Err(failure("Int actual exact scalar equality", compare));
            };
            let read = g.definition(EntityId::Value(left))?;
            let literal = g.definition(EntityId::Value(right))?;
            let aliases = family(&g, parameter)?;
            let Operation::Read {
                source: PlaceAccess::Loan(loan),
            } = read.operation
            else {
                return Err(failure(
                    "Int actual Copy read from current parameter loan",
                    read,
                ));
            };
            need(
                aliases.contains(&EntityId::Loan(loan))
                    && read.origin == Origin::Source(ast::span(p, i.compared[index]))
                    && f.entity(result(read)?).unwrap().ty == EntityType::Value(int)
                    && literal.operation
                        == Operation::Constant(ScalarConstant::Integer(index as i128 + 1))
                    && literal.origin == Origin::Source(ast::span(p, i.literals[index]))
                    && f.entity(result(literal)?).unwrap().ty == EntityType::Value(int),
                "Int copied value comes from exact original K/V parameter and compares literal 1/2",
                (read, literal, compare),
            )?;
            for e in aliases {
                uses.protect(e);
            }
            uses.permit(EntityId::Loan(loan), read);
            uses.protect(result(read)?);
            uses.permit(result(read)?, compare);
            uses.protect(result(literal)?);
            uses.permit(result(literal)?, compare);
            conditions.push(value(compare)?);
        }
        need(
            g.instructions.iter().filter(|op| prints(&g, op)).count() == 1,
            "Int sole actual success output",
            f.id,
        )?;
        uses.check(&g, None)?;
        // Enumerate the four truth assignments of these two exact comparison results.
        // Every actual edge/parameter is retained; an unknown Boolean producer is an error.
        for key in [false, true] {
            for value in [false, true] {
                let mut booleans = BTreeMap::<ValueId, bool>::new();
                let mut block = f.entry_block().unwrap();
                let mut seen = BTreeSet::new();
                let mut output = 0;
                loop {
                    need(
                        seen.insert(block),
                        "Int reader actual acyclic comparison CFG",
                        block,
                    )?;
                    for op in g.ops(block)? {
                        if let Some(index) = conditions
                            .iter()
                            .position(|v| op.results.as_slice() == [EntityId::Value(*v)])
                        {
                            booleans
                                .insert(conditions[index], if index == 0 { key } else { value });
                        } else {
                            match op.operation {
                                Operation::Constant(ScalarConstant::Boolean(v)) => {
                                    booleans.insert(super::value(op)?, v);
                                }
                                Operation::Copy { source } => {
                                    if let Some(v) = booleans.get(&source).copied() {
                                        booleans.insert(super::value(op)?, v);
                                    }
                                }
                                _ => {}
                            }
                        }
                        if prints(&g, op) {
                            output += 1;
                            need(
                                key && value && output == 1,
                                "Int only both actual scalar equalities may reach success print",
                                (key, value, op),
                            )?;
                        }
                    }
                    let edge = match &g.term(block)?.kind {
                        TerminatorKind::Branch(e) => e,
                        TerminatorKind::Conditional {
                            condition,
                            when_true,
                            when_false,
                        } => {
                            let condition=booleans.get(condition).copied().ok_or_else(||failure("Int conditional must read exact comparison/actual transported Boolean",condition))?;
                            if condition { when_true } else { when_false }
                        }
                        TerminatorKind::Return { values } => {
                            need(
                                key && value && output == 1 && values.is_empty(),
                                "Int normal Return only follows both scalar comparisons and output",
                                (key, value, output),
                            )?;
                            break;
                        }
                        TerminatorKind::Abort => {
                            need(
                                !(key && value) && output == 0,
                                "Int failed actual scalar equality reaches Abort without success",
                                (key, value, output),
                            )?;
                            break;
                        }
                        other => return Err(failure("Int original reader terminal", other)),
                    };
                    let values = edge
                        .arguments
                        .iter()
                        .map(|arg| match arg {
                            EntityId::Value(id) => booleans.get(id).copied(),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    for (&parameter, value) in g.block(edge.target)?.parameters.iter().zip(values) {
                        if let (EntityId::Value(parameter), Some(value)) = (parameter, value) {
                            booleans.insert(parameter, value);
                        }
                    }
                    block = edge.target;
                }
            }
        }
        Ok(())
    }
}
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast, facts::Proof};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::Span};
use std::collections::{BTreeMap, BTreeSet};
fn at<'ir>(
    g: &Graph<'ir>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.origin == Origin::Source(span) && predicate(&op.operation)),
        "Int exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, seed: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = BTreeSet::from([seed]);
    loop {
        let before = all.len();
        for (_, e) in &g.edges {
            for (&a, &p) in e.arguments.iter().zip(&g.block(e.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if before == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[seed])?;
    }
    Ok(all)
}
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &ast::Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "Int actual original module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::with_abort(scan)?;
    let entry = function(module, i.entry)?;
    let eg = Graph::new(entry)?;
    let invoke = at(&eg, ast::span(p, i.scanner_call), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    need(
        invoke.operation
            == Operation::DirectCall {
                callee: scan.id,
                receiver: None,
                arguments: vec![],
            }
            && invoke.results.is_empty()
            && eg.blocks.len() == 1
            && eg
                .block(entry.entry_block().unwrap())?
                .parameters
                .is_empty(),
        "Int original entry actual zeroarg scan call",
        invoke,
    )?;
    let birth = at(&g, ast::span(p, i.birth), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let factory = function(module, i.make)?;
    need(
        birth.operation
            == Operation::DirectCall {
                callee: factory.id,
                receiver: None,
                arguments: vec![],
            }
            && g.block(scan.entry_block().unwrap())?.parameters.is_empty(),
        "Int original named source birth actual makeInt call",
        birth,
    )?;
    let seed = result(birth)?;
    let owner = value(birth)?;
    let map = scan.entity(seed).unwrap().ty.semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("Int actual Map output storage", map))?;
    need(
        kind == MapContainerKind::Map
            && key == val
            && module.type_kind(key)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
            && factory.return_types.as_slice() == [map],
        "Int actual concrete Copy Int32 key/value map storage",
        (map, key, val),
    )?;
    factory::check(p, i, module, map)?;
    let owners = family(&g, seed)?;
    let mut uses = Uses::default();
    for &e in &owners {
        uses.protect(e);
    }
    let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if owners.contains(&EntityId::Value(owner)))),"Int unique actual current Map cleanup")?;
    need(
        drop.origin == Origin::Source(proof.drop) && proof.drop == i.source,
        "Int same-analysis original named source Drop provenance",
        drop,
    )?;
    if let Some(site) = &i.site {
        reader::check(p, i, module, key)?;
        provider::check(
            &g,
            module,
            function(module, i.reader)?,
            p,
            (site, seed, drop),
            &mut uses,
        )?;
    } else {
        let size = at(&g, ast::span(p, i.size.unwrap()), |op| {
            matches!(op, Operation::MapSize { .. })
        })?;
        need(
            size.operation == Operation::MapSize { owner: seed }
                && drop.operation == Operation::Drop { owner }
                && size.block == drop.block
                && birth.block == size.block
                && g.position(birth)? < g.position(size)?
                && g.position(drop)? == g.position(size)? + 1
                && !g.instructions.iter().any(|op| {
                    matches!(
                        op.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
            "Int control real current MapSize then immediate unique owner Drop",
            (birth, size, drop),
        )?;
        uses.permit(seed, size);
    }
    let Operation::Drop { owner: current } = drop.operation else {
        unreachable!()
    };
    uses.permit(EntityId::Value(current), drop);
    for (index, data) in scan.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                owners.contains(&EntityId::Value(ValueId {
                    function: scan.id,
                    index,
                })),
                "Int every scanner Map Value is the exact current source family",
                (index, data),
            )?;
        }
    }
    need(c.provider==i.site.is_some()&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==1&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::MapConstruct{..}|Operation::MapPut{..}|Operation::MapConsume{..}))&&g.blocks.iter().all(|&block|!matches!(&g.term(block).unwrap().kind,TerminatorKind::Return{values}if !values.is_empty())),"Int one current cleanup and void terminal, no alternate scanner Map birth",scan.id)?;
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &BTreeSet::from([seed]),
    )?;
    uses.check(&g, None)
}
