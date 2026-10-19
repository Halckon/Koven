//! Exhaustive actual CFG order proves cleanup dominates every later marker and Return.
use super::*;
pub(super) fn check(
    g: &Graph<'_>,
    seed: EntityId,
    size: &Instruction,
    drop: &Instruction,
    after: &Instruction,
) -> Result<(), Failure> {
    let birth = g.definition(seed)?;
    need(
        birth.block == size.block
            && size.block == drop.block
            && g.position(birth)? < g.position(size)?
            && g.position(drop)? == g.position(size)? + 1,
        "birth control actual producer/size/immediate cleanup in preheader",
        (birth, size, drop),
    )?;
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8, 0u8)];
    let mut seen = BTreeSet::new();
    let mut returned = 0;
    let mut aborted = 0;
    while let Some((block, mut made, mut read, mut freed, mut marked)) = pending.pop() {
        if !seen.insert((block, made, read, freed, marked)) {
            continue;
        }
        for op in g.ops(block)? {
            if op.id == birth.id {
                made += 1;
                need(
                    (made, read, freed, marked) == (1, 0, 0, 0),
                    "birth control actual result created once before any use",
                    op,
                )?;
            }
            if op.id == size.id {
                read += 1;
                need(
                    (made, read, freed, marked) == (1, 1, 0, 0),
                    "birth control size reads actual live result once",
                    op,
                )?;
            }
            if op.id == drop.id {
                freed += 1;
                need(
                    (made, read, freed, marked) == (1, 1, 1, 0),
                    "birth control unique Drop follows size on every path",
                    op,
                )?;
            }
            if op.id == after.id {
                marked += 1;
                need(
                    (made, read, freed, marked) == (1, 1, 1, 1),
                    "birth control actual cleanup dominates every after marker path",
                    op,
                )?;
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Return { values } => {
                returned += 1;
                need(
                    values.is_empty() && (made, read, freed, marked) == (1, 1, 1, 1),
                    "birth every actual normal Return follows one size/cleanup/marker",
                    g.term(block)?,
                )?;
            }
            TerminatorKind::Abort => {
                aborted += 1;
                need(
                    (made, read, freed, marked) == (1, 1, 1, 0),
                    "birth no-For failure edge starts after cleanup and before after",
                    g.term(block)?,
                )?;
            }
            _ => {}
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, made, read, freed, marked));
        }
    }
    need(
        returned == 1 && aborted == 1,
        "birth control actual successful and bad-size paths both accounted",
        (returned, aborted),
    )
}
