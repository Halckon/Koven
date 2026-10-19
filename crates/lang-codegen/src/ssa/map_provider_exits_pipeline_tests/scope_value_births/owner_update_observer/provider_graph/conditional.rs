//! Named-source carried columns, with the implicit false edge kept distinct.
use super::*;
pub(super) struct Joined<'a> {
    pub owner: EntityId,
    pub block: BlockId,
    pub blocks: [BlockId; 4],
    pub old_drop: Option<&'a Instruction>,
}
pub(super) fn check<'a>(
    g: &Graph<'a>,
    p: &ParsedFile,
    i: &Input,
    (old, new): (EntityId, EntityId),
    old_origin: Option<Span>,
    uses: &mut Uses,
) -> Result<Joined<'a>, Failure> {
    let (expr, condition) = i
        .conditional
        .ok_or_else(|| failure("update original conditional AST", i))?;
    let header = one(
        g.blocks.iter().copied().filter(|&b| {
            g.term(b).is_ok_and(|t| {
                t.origin == Origin::Source(span(p, expr))
                    && matches!(t.kind, TerminatorKind::Conditional { .. })
            })
        }),
        "update original conditional write header",
    )?;
    let TerminatorKind::Conditional {
        condition: actual,
        when_true,
        when_false,
    } = &g.term(header)?.kind
    else {
        unreachable!()
    };
    let read = g.definition(EntityId::Value(*actual))?;
    let Operation::Read {
        source: PlaceAccess::Loan(flag),
    } = read.operation
    else {
        return Err(failure("update actual original flag Read", read));
    };
    let original = *one(
        g.block(g.function.entry_block().unwrap())?
            .parameters
            .iter(),
        "update actual scanner flag parameter",
    )?;
    g.anchored(EntityId::Loan(flag), &[original])?;
    need(
        read.origin == Origin::Source(span(p, condition)) && read.block == header,
        "update condition preserves original flag identity and polarity",
        read,
    )?;
    for &block in &g.blocks {
        for &parameter in &g.block(block)?.parameters {
            if g.function.entity(parameter).unwrap().ty == g.function.entity(original).unwrap().ty {
                g.anchored(parameter, &[original])?;
                uses.protect(parameter);
            }
        }
    }
    uses.permit(EntityId::Loan(flag), read);
    let new_definition = g.definition(new)?;
    need(
        new_definition.block == when_true.target,
        "update only true arm installs the actual new Call/put value",
        new_definition,
    )?;
    if let Operation::MapPut { owner, .. } = new_definition.operation {
        g.anchored(EntityId::Value(owner), &[old])?;
        need(
            new != EntityId::Value(owner),
            "update put yields distinct SSA current owner",
            new_definition,
        )?;
    }
    let yes = g.branch(when_true.target)?;
    let no = g.branch(when_false.target)?;
    need(
        yes.target == no.target,
        "update actual named carried true/implicit-false join",
        (yes, no),
    )?;
    g.incoming_from(when_true.target, &[header])?;
    g.incoming_from(when_false.target, &[header])?;
    g.incoming_from(yes.target, &[when_true.target, when_false.target])?;
    let owner = one(
        g.block(yes.target)?
            .parameters
            .iter()
            .copied()
            .filter(|&param| {
                g.argument(yes, param).is_ok_and(|arg| arg == new)
                    && g.argument(no, param)
                        .is_ok_and(|arg| g.anchored(arg, &[old]).is_ok())
            }),
        "update exact carried named-source column, never arbitrary join0",
    )?;
    g.anchored(owner, &[old, new])?;
    let old_drop = if let Some(origin) = old_origin {
        let drop=one(g.instructions.iter().copied().filter(|op|op.block==when_true.target&&op.origin==Origin::Source(origin)&&matches!(op.operation,Operation::Drop{owner}if g.anchored(EntityId::Value(owner),&[old]).is_ok())),"update conditional true replacement old current owner Drop")?;
        need(
            g.position(new_definition)? < g.position(drop)?
                && g.ops(drop.block)?.last().map(|op| op.id) == Some(drop.id),
            "update replacement RHS completes before old Drop and actual new incoming installation",
            (new_definition, drop),
        )?;
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        uses.permit(EntityId::Value(owner), drop);
        Some(drop)
    } else {
        None
    };
    Ok(Joined {
        owner,
        block: yes.target,
        blocks: [header, when_true.target, when_false.target, yes.target],
        old_drop,
    })
}
