//! Bounded path witness for this closed, independently replayed SSA subset only.
//! Logical owner generations are NOT heap allocations or native execution evidence.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Datum {
    Int(i32),
    Bool(bool),
    Owner(usize),
    Place(usize),
    Input(i32),
    Loan(usize, usize, LoanKind),
}
#[derive(Debug)]
struct Run {
    printed: Vec<Vec<u8>>,
    dropped: Vec<usize>,
    created: usize,
}
type Result<T> = std::result::Result<T, &'static str>;

pub(super) fn check_program(program: &Program, entry: FunctionId) {
    for (parent, child, final_text, generations) in [
        (0, 0, "first", 2),
        (0, 2, "first", 2),
        (1, 0, "parent", 3),
        (2, 0, "parent", 4),
        (1, 2, "next", 5),
        (2, 2, "next", 8),
        (3, 4, "next", 17),
        (-1, 2, "first", 2),
        (2, -1, "parent", 4),
    ] {
        let run = execute(program, entry, [parent, child], 2048).expect("closed actual SSA path");
        assert_eq!(
            run.printed,
            [final_text.as_bytes().to_vec(), b"kept".to_vec()]
        );
        assert_eq!(run.created, generations);
        let expected = std::iter::once(0)
            .chain(2..generations)
            .chain(std::iter::once(1))
            .collect::<Vec<_>>();
        assert_eq!(
            run.dropped, expected,
            "logical Drop generations for ({parent},{child})"
        );
    }
    assert!(matches!(
        execute(program, entry, [2, 2], 1),
        Err("step budget exhausted")
    ));
}

/// Called only after the independent AfterChild bound/source/complete SSA replay.
/// This is the same bounded logical model, not a native or allocator witness.
pub(super) fn check_after_child(
    program: &Program,
    entry: FunctionId,
    initial: &[u8],
    parent_value: &[u8],
    retained: &[u8],
) {
    for (parent, child, generations) in [
        (0, 0, 2),
        (0, 2, 2),
        (1, 0, 3),
        (2, 0, 4),
        (1, 2, 5),
        (2, 2, 8),
        (3, 4, 17),
        (-1, 2, 2),
        (2, -1, 4),
    ] {
        let run = execute(program, entry, [parent, child], 2048)
            .expect("closed child-then-parent actual SSA path");
        let final_value = if parent > 0 { parent_value } else { initial };
        assert_eq!(run.printed, [final_value.to_vec(), retained.to_vec()]);
        assert_eq!(run.created, generations);
        let expected = std::iter::once(0)
            .chain(2..generations)
            .chain(std::iter::once(1))
            .collect::<Vec<_>>();
        assert_eq!(
            run.dropped, expected,
            "AfterChild logical Drops ({parent},{child})"
        );
    }
}

/// Source-derived expected bytes for the sealed two-counter before-child graph.
/// Still only the bounded logical SSA witness, never native or allocator verification.
pub(super) fn check_before_child(
    program: &Program,
    entry: FunctionId,
    literals: &[(Origin, Vec<u8>); 4],
) {
    for (parent, child, generations) in [
        (0, 0, 2),
        (0, 2, 2),
        (1, 0, 3),
        (2, 0, 4),
        (1, 2, 5),
        (2, 2, 8),
        (3, 4, 17),
        (-1, 2, 2),
        (2, -1, 4),
    ] {
        let run = execute(program, entry, [parent, child], 2048)
            .expect("closed parent-then-child actual SSA path");
        let final_value = if parent <= 0 {
            &literals[0].1
        } else if child <= 0 {
            &literals[2].1
        } else {
            &literals[3].1
        };
        assert_eq!(run.printed, [final_value.clone(), literals[1].1.clone()]);
        assert_eq!(run.created, generations);
        let expected = std::iter::once(0)
            .chain(2..generations)
            .chain(std::iter::once(1))
            .collect::<Vec<_>>();
        assert_eq!(
            run.dropped, expected,
            "BeforeChild logical Drops ({parent},{child})"
        );
    }
}

fn execute(program: &Program, entry: FunctionId, inputs: [i32; 2], mut fuel: usize) -> Result<Run> {
    crate::ssa::verify::verify_program(program).map_err(|_| "invalid SSA")?;
    let [module] = program.modules.as_slice() else {
        return Err("module shape");
    };
    let [f] = module.functions.as_slice() else {
        return Err("function shape");
    };
    if f.id() != entry {
        return Err("entry identity");
    }
    let mut block = f.entry_block().ok_or("entry block")?;
    let parameters = &f.block(block).ok_or("entry block")?.parameters;
    let [EntityId::Loan(_), EntityId::Loan(_)] = parameters.as_slice() else {
        return Err("input ABI");
    };
    let mut env = parameters
        .iter()
        .copied()
        .zip(inputs.map(Datum::Input))
        .collect::<BTreeMap<_, _>>();
    let mut owners = Vec::<Vec<u8>>::new();
    let mut live = BTreeSet::new();
    let mut loans = BTreeMap::<usize, (usize, LoanKind)>::new();
    let mut next_loan = 0;
    let mut printed = Vec::new();
    let mut dropped = Vec::new();
    loop {
        let b = f.block(block).ok_or("missing block")?;
        for id in &b.instructions {
            fuel = fuel.checked_sub(1).ok_or("step budget exhausted")?;
            let i = f.instruction(*id).ok_or("missing instruction")?;
            let get = |id| env.get(&id).copied().ok_or("missing executed value");
            let value = |id| get(EntityId::Value(id));
            let integer = |id| match value(id)? {
                Datum::Int(v) => Ok(v),
                _ => Err("expected Int32"),
            };
            let owner = |id| match value(id)? {
                Datum::Owner(g) if live.contains(&g) => Ok(g),
                _ => Err("expected live owner"),
            };
            let results = match &i.operation {
                Operation::StringLiteral { bytes, .. } => {
                    let generation = owners.len();
                    owners.push(bytes.clone());
                    live.insert(generation);
                    vec![Datum::Owner(generation)]
                }
                Operation::Constant(ScalarConstant::Integer(n)) => {
                    vec![Datum::Int(i32::try_from(*n).map_err(|_| "Int32 constant")?)]
                }
                Operation::Read {
                    source: PlaceAccess::Loan(l),
                } => match get(EntityId::Loan(*l))? {
                    Datum::Input(n) => vec![Datum::Int(n)],
                    _ => return Err("unsupported read"),
                },
                Operation::Compare {
                    operator: ComparisonOperator::GreaterThan,
                    left,
                    right,
                } => vec![Datum::Bool(integer(*left)? > integer(*right)?)],
                Operation::CheckedArithmetic {
                    operator: CheckedArithmeticOperator::Subtract,
                    left,
                    right,
                } => {
                    let (n, overflow) = integer(*left)?.overflowing_sub(integer(*right)?);
                    vec![Datum::Int(n), Datum::Bool(overflow)]
                }
                Operation::RootPlace { owner: root } => vec![Datum::Place(owner(*root)?)],
                Operation::BorrowBegin { place, kind } => {
                    let Datum::Place(g) = get(EntityId::Place(*place))? else {
                        return Err("expected root place");
                    };
                    if !live.contains(&g) || loans.values().any(|(other, _)| *other == g) {
                        return Err("overlapping closed borrow");
                    }
                    let token = next_loan;
                    next_loan += 1;
                    loans.insert(token, (g, *kind));
                    vec![Datum::Loan(token, g, *kind)]
                }
                Operation::RootReplace {
                    owner: old,
                    loan,
                    replacement,
                } => {
                    let old = owner(*old)?;
                    let new = owner(*replacement)?;
                    let Datum::Loan(token, g, LoanKind::Exclusive) = get(EntityId::Loan(*loan))?
                    else {
                        return Err("exclusive Replace loan");
                    };
                    if g != old
                        || old == new
                        || loans.remove(&token) != Some((old, LoanKind::Exclusive))
                    {
                        return Err("Replace identity");
                    }
                    vec![Datum::Owner(new), Datum::Owner(old)]
                }
                Operation::PrintString { value: loan } => {
                    let Datum::Loan(token, g, LoanKind::Shared) = get(EntityId::Loan(*loan))?
                    else {
                        return Err("shared print loan");
                    };
                    if !live.contains(&g) || loans.get(&token) != Some(&(g, LoanKind::Shared)) {
                        return Err("stale print loan");
                    }
                    printed.push(owners.get(g).ok_or("owner generation")?.clone());
                    vec![]
                }
                Operation::BorrowEnd { loan } => {
                    let Datum::Loan(token, g, kind) = get(EntityId::Loan(*loan))? else {
                        return Err("ending borrow");
                    };
                    if loans.remove(&token) != Some((g, kind)) {
                        return Err("stale End");
                    }
                    vec![]
                }
                Operation::Drop { owner: root } => {
                    let g = owner(*root)?;
                    if loans.values().any(|(other, _)| *other == g) || !live.remove(&g) {
                        return Err("borrowed or duplicate Drop");
                    }
                    dropped.push(g);
                    vec![]
                }
                _ => return Err("outside closed instruction subset"),
            };
            if results.len() != i.results.len() {
                return Err("result arity");
            }
            for (id, datum) in i.results.iter().copied().zip(results) {
                env.insert(id, datum);
            }
        }
        fuel = fuel.checked_sub(1).ok_or("step budget exhausted")?;
        let edge = match &b.terminator.as_ref().ok_or("missing terminator")?.kind {
            TerminatorKind::Branch(edge) => edge,
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } => match env.get(&EntityId::Value(*condition)) {
                Some(Datum::Bool(true)) => when_true,
                Some(Datum::Bool(false)) => when_false,
                _ => return Err("branch condition"),
            },
            TerminatorKind::Return { values } if values.is_empty() => {
                if !live.is_empty() || !loans.is_empty() {
                    return Err("unfinished owners or borrows");
                }
                return Ok(Run {
                    printed,
                    dropped,
                    created: owners.len(),
                });
            }
            TerminatorKind::Abort => return Err("actual abort edge"),
            _ => return Err("outside closed terminator subset"),
        };
        let target = f.block(edge.target).ok_or("edge target")?;
        if target.parameters.len() != edge.arguments.len() {
            return Err("edge arity");
        }
        // Snapshot every actual argument before rebinding reused loop parameter IDs.
        let arguments = edge
            .arguments
            .iter()
            .map(|id| env.get(id).copied().ok_or("edge value"))
            .collect::<Result<Vec<_>>>()?;
        for (id, datum) in target.parameters.iter().copied().zip(arguments) {
            env.insert(id, datum);
        }
        block = target.id;
    }
}
