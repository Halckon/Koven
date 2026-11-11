//! Follow actual, single-definition, ordered entry moves to a catalog producer.
use super::*;
use crate::ssa::refine::body::RStmt;

pub(super) fn producer(statements: &[RStmt], root: Slot, origin: Span) -> Option<Slot> {
    let mut current = root;
    let mut before = statements.len();
    loop {
        let mut definitions = statements
            .iter()
            .enumerate()
            .filter(|(_, statement)| statement.results.contains(&current));
        let (index, statement) = definitions.next()?;
        if definitions.next().is_some() || index >= before || statement.results != [current] {
            return None;
        }
        match statement.op {
            ROp::StringLiteral { .. } if statement.span == origin => return Some(current),
            ROp::Alias { source } => {
                // Each intermediate is consumed by this one move, not another
                // alias/drop/read. The final root may be used later by the loop.
                if statements
                    .iter()
                    .flat_map(|s| s.op.operands())
                    .filter(|operand| *operand == source)
                    .count()
                    != 1
                {
                    return None;
                }
                before = index;
                current = source;
            }
            _ => return None,
        }
    }
}
