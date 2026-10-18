//! Read only original declaration/formation anchors and complete actual transport columns.
use super::*;
pub(super) fn inside(span: Span, outer: Span) -> bool {
    span.source_id() == outer.source_id()
        && span.start() > outer.start()
        && span.end() < outer.end()
}
pub(super) fn declaration(
    parsed: &ParsedFile,
    sources: &SourceMap,
    name: &str,
) -> Result<Span, Failure> {
    one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;
        matches!(n.payload(),Item::Function {name:NameMarker::Present(s),..} if sources.slice(*s)==Ok(name)).then_some(n.span())}),"original callable declaration")
}
pub(super) fn site(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scanner: &str,
    maker: &str,
) -> Result<Site, Failure> {
    let scan = declaration(parsed, sources, scanner)?;
    let mut loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| match n.payload() {
            Statement::For { source, body, .. } if inside(n.span(), scan) => {
                Some((n.span(), *source, *body))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    loops.sort_by_key(|(s, _, _)| s.start());
    need(
        loops.len() == if case.form == 2 { 2 } else { 1 },
        "all original scanner For sites",
        &loops,
    )?;
    for (_, source, _) in &loops {
        let n = parsed.ast().expressions().get(*source).unwrap();
        need(
            n.payload() == &Expression::Name && sources.slice(n.span()) == Ok("source"),
            "original source Name for each For",
            n,
        )?;
    }
    let binding = |wanted: &str| {
        one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        kind,
                        ..
                    } if inside(n.span(), scan)
                        && n.span().end() < loops[0].0.start()
                        && sources.slice(*name) == Ok(wanted) =>
                    {
                        Some((*name, *initializer, *kind))
                    }
                    _ => None,
                }),
            "actual original source declaration",
        )
    };
    let (name, mut initializer, kind) = binding("source")?;
    need(
        kind == if case.binding == 1 {
            VariableKind::Var
        } else {
            VariableKind::Val
        },
        "actual selected val/var formation",
        kind,
    )?;
    if case.binding == 2 {
        let alias = parsed.ast().expressions().get(initializer).unwrap();
        need(
            alias.payload() == &Expression::Name && sources.slice(alias.span()) == Ok("original"),
            "original moved alias Name",
            alias,
        )?;
        let (original, init, kind) = binding("original")?;
        need(
            kind == VariableKind::Val && original.start() < name.start(),
            "original factory precedes alias Move",
            original,
        )?;
        initializer = init;
    }
    let call = parsed.ast().expressions().get(initializer).unwrap();
    need(
        matches!(call.payload(),Expression::Call {callee,arguments,..} if arguments.is_empty() && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())==Ok(maker)),
        "actual selected zeroarg factory Call",
        call,
    )?;
    Ok(Site {
        scan,
        binding: name,
        call: call.span(),
        factory: declaration(parsed, sources, maker)?,
        loops: loops
            .into_iter()
            .enumerate()
            .map(|(i, (span, _, body))| LoopSite {
                span,
                body,
                exit: if i == 0 { case.exit } else { 0 },
            })
            .collect(),
    })
}

pub(super) fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut members = BTreeSet::from([anchor]);
    loop {
        let before = members.len();
        for (_, edge) in &g.edges {
            for (&arg, &parameter) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if members.contains(&arg) {
                    members.insert(parameter);
                }
            }
        }
        if members.len() == before {
            break;
        }
    }
    for &member in &members {
        transport(g, member, anchor)?;
    }
    Ok(members)
}
// Every dependency is inspected; a recurrent column also needs a real anchor path.
fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let ty = g.function.entity(anchor).unwrap().ty;
    let mut pending = vec![current];
    let mut incoming = BTreeMap::new();
    while let Some(e) = pending.pop() {
        if e == anchor || incoming.contains_key(&e) {
            continue;
        }
        let d = g
            .function
            .entity(e)
            .ok_or_else(|| failure("actual current transport entity", e))?;
        let Definition::BlockParameter { block, .. } = d.definition else {
            return Err(failure("foreign current owner/loan producer", d));
        };
        need(d.ty == ty, "current transport keeps represented type", d)?;
        let edges = g.incoming(block);
        need(!edges.is_empty(), "complete nonempty predecessor set", e)?;
        let args = edges
            .iter()
            .map(|(_, edge)| g.argument(edge, e))
            .collect::<Result<Vec<_>, _>>()?;
        pending.extend(args.iter().copied());
        incoming.insert(e, args);
    }
    let mut rooted = BTreeSet::from([anchor]);
    loop {
        let before = rooted.len();
        for (&e, args) in &incoming {
            if args.iter().any(|a| rooted.contains(a)) {
                rooted.insert(e);
            }
        }
        if before == rooted.len() {
            break;
        }
    }
    need(
        incoming.keys().all(|e| rooted.contains(e)),
        "all current incoming paths eventually reach original real anchor",
        incoming,
    )
}
pub(super) fn reaches(g: &Graph<'_>, from: BlockId, to: BlockId, include_start: bool) -> bool {
    let mut pending = if include_start {
        vec![from]
    } else {
        g.edges
            .iter()
            .filter_map(|(b, e)| (*b == from).then_some(e.target))
            .collect()
    };
    let mut seen = BTreeSet::new();
    while let Some(b) = pending.pop() {
        if b == to {
            return true;
        }
        if seen.insert(b) {
            pending.extend(
                g.edges
                    .iter()
                    .filter_map(|(from, e)| (*from == b).then_some(e.target)),
            );
        }
    }
    false
}
pub(super) fn print<'a, 'p>(
    q: &Query<'a, 'p>,
    outer: Span,
    wanted: &str,
) -> Result<&'p Instruction, Failure> {
    let (span, arg) = one(
        q.parsed.ast().expressions().iter().filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            let [a] = arguments.as_slice() else {
                return None;
            };
            let arg = q.parsed.ast().expressions().get(a.value).ok()?.span();
            (inside(n.span(), outer)
                && q.sources
                    .slice(q.parsed.ast().expressions().get(*callee).ok()?.span())
                    == Ok("println")
                && q.sources.slice(arg) == Ok(format!("\"{wanted}\"").as_str()))
            .then_some((n.span(), arg))
        }),
        "original scanner marker call",
    )?;
    let i = one(
        q.g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual scanner marker instruction",
    )?;
    match &i.operation {
        Operation::PrintLiteral { bytes } => {
            let mut expected = wanted.as_bytes().to_vec();
            expected.push(b'\n');
            need(*bytes == expected, "marker actual bytes", i)?;
        }
        Operation::PrintString { value: loan } => {
            let birth = q.g.definition(EntityId::Loan(*loan))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("marker Shared String loan", birth));
            };
            let root = q.g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("marker actual String root", root));
            };
            let literal = q.g.definition(EntityId::Value(owner))?;
            need(
                literal.origin == Origin::Source(arg)
                    && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==wanted.as_bytes()),
                "marker actual String literal identity",
                literal,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(i)
}
pub(super) fn reader(q: &Query<'_, '_>, nullable: bool) -> Result<(), Failure> {
    let reader = function(q.module, declaration(q.parsed, q.sources, "read")?)?;
    let rg = Graph::new(reader)?;
    let params = &rg.block(reader.entry_block().unwrap())?.parameters;
    need(
        params.len() == 2
            && params.iter().zip([q.types.0, q.types.1]).all(|(p, t)| {
                reader.entity(*p).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: t,
                    }
            }),
        "actual reader Shared Resource component ABI",
        params,
    )?;
    let tests = rg
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::NullableLoanIsNull { .. }))
        .collect::<Vec<_>>();
    need(tests.len()==usize::from(nullable) && tests.iter().all(|i|matches!(i.operation,Operation::NullableLoanIsNull {source} if EntityId::Loan(source)==params[1])),"nullable check uses actual Shared parameter without owner extraction",&tests)?;
    let resource = match q.module.type_kind(q.types.1) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => q.types.1,
    };
    need(
        !rg.instructions.iter().any(|i| {
            matches!(i.operation, Operation::NullableTake { .. })
                || matches!(i.operation, Operation::Drop {owner} if [q.types.1,resource].contains(&reader.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))
        }),
        "reader never consumes borrowed Resource payload",
        reader.id,
    )
}
