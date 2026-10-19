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
                        booleans.insert(conditions[index], if index == 0 { key } else { value });
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
