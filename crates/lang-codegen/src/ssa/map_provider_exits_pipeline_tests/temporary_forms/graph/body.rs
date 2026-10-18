//! Shared K/V lineage, optional ordinary reborrows and complete body cleanup.
use super::*;
use std::collections::BTreeMap;

pub(super) fn check<'ir>(
    q: &Query<'_, 'ir>,
    site: &ast::Site,
    block: BlockId,
    roles: Roles,
    key_type: SsaTypeId,
    value_type: SsaTypeId,
    uses: &mut Uses,
) -> Result<&'ir Instruction, Failure> {
    let g = q.g;
    let ops = g.ops(block)?;
    let key = one(
        ops.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
        "temporary form actual body key loan",
    )?;
    let val = one(
        ops.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
        "temporary form actual body value loan",
    )?;
    let next = one(
        ops.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
        "temporary form actual body Next",
    )?;
    need(
        key.operation
            == Operation::MapIterationKeyLoan {
                source: roles.source,
                cursor: roles.cursor,
            }
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                }
            && next.operation
                == Operation::MapIterationNext {
                    source: roles.source,
                    cursor: roles.cursor,
                }
            && [key, val, next]
                .iter()
                .all(|i| i.origin == Origin::Source(site.span)),
        "temporary form original body current source/cursor operations",
        (key, val, next),
    )?;
    let roots = [loan(key)?, loan(val)?];
    let reader_entry = q.reader.block(q.reader.entry_block().unwrap()).unwrap();
    need(
        q.reader.receiver.is_none()
            && q.reader.borrow_return.is_none()
            && q.reader.carrier_return.is_none()
            && q.reader.return_types.is_empty()
            && reader_entry.parameters.len() == 2
            && roots.iter().zip([key_type, value_type]).enumerate().all(
                |(index, (&root, target))| {
                    let ty = EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    };
                    g.function.entity(EntityId::Loan(root)).unwrap().ty == ty
                        && q.reader.entity(reader_entry.parameters[index]).unwrap().ty == ty
                },
            ),
        "temporary form reader Shared ABI preserves actual represented key/value",
        q.reader.id,
    )?;
    let mut lineage = BTreeMap::<LoanId, (usize, Option<LoanId>)>::new();
    let mut live = BTreeSet::new();
    let mut ended_roots = Vec::new();
    let mut calls = 0;
    for instruction in &ops {
        match &instruction.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let current = loan(instruction)?;
                let index = if current == roots[0] { 0 } else { 1 };
                need(
                    roots.contains(&current)
                        && lineage.insert(current, (index, None)).is_none()
                        && live.insert(current),
                    "temporary form exact independent component birth",
                    instruction,
                )?;
                uses.protect(EntityId::Loan(current));
                uses.permit(EntityId::Loan(roles.source), instruction);
                uses.permit(EntityId::Value(roles.cursor), instruction);
            }
            Operation::SharedReborrow { source } => {
                let (index, _) = *lineage
                    .get(source)
                    .ok_or_else(|| failure("temporary form actual reborrow parent", instruction))?;
                let current = loan(instruction)?;
                need(
                    live.contains(source)
                        && instruction.origin
                            == Origin::Source(ast::expression(q.parsed, site.arguments[index]))
                        && g.function.entity(EntityId::Loan(current)).unwrap().ty
                            == g.function.entity(EntityId::Loan(*source)).unwrap().ty
                        && lineage.insert(current, (index, Some(*source))).is_none()
                        && live.insert(current),
                    "temporary form typed live Borrow descends from this original argument",
                    instruction,
                )?;
                uses.permit(EntityId::Loan(*source), instruction);
                uses.protect(EntityId::Loan(current));
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                need(
                    *callee == q.reader.id
                        && receiver.is_none()
                        && arguments.len() == 2
                        && instruction.origin
                            == Origin::Source(ast::expression(q.parsed, site.read))
                        && instruction.results.is_empty()
                        && calls == 0,
                    "temporary form actual original reader Call",
                    instruction,
                )?;
                for (index, &argument) in arguments.iter().enumerate() {
                    let EntityId::Loan(current) = argument else {
                        return Err(failure("temporary form actual Borrow operand", argument));
                    };
                    need(
                        live.contains(&current)
                            && lineage
                                .get(&current)
                                .is_some_and(|(root, _)| *root == index),
                        "temporary form actual reader ordered component lineage",
                        (index, argument),
                    )?;
                    uses.permit(argument, instruction);
                }
                calls += 1;
            }
            Operation::BorrowEnd { loan } => {
                let (_, parent) = lineage
                    .get(loan)
                    .copied()
                    .ok_or_else(|| failure("temporary form actual ended component", instruction))?;
                need(
                    calls == 1
                        && live.contains(loan)
                        && !lineage.iter().any(|(child, (_, parent))| {
                            *parent == Some(*loan) && live.contains(child)
                        })
                        && instruction.origin
                            == Origin::Source(if parent.is_none() {
                                site.span
                            } else {
                                ast::expression(q.parsed, site.read)
                            }),
                    "temporary form derived Borrow ends before parent after real read",
                    instruction,
                )?;
                live.remove(loan);
                if parent.is_none() {
                    ended_roots.push(*loan);
                }
                uses.permit(EntityId::Loan(*loan), instruction);
            }
            Operation::MapIterationNext { .. } => {
                need(
                    instruction.id == next.id
                        && calls == 1
                        && live.is_empty()
                        && ended_roots.as_slice() == [roots[1], roots[0]]
                        && g.function.entity(result(instruction)?).unwrap().ty
                            == g.function.entity(EntityId::Value(roles.cursor)).unwrap().ty,
                    "temporary form read then derived Ends/VEnd/KEnd then real Next",
                    instruction,
                )?;
                uses.permit(EntityId::Loan(roles.source), instruction);
                uses.permit(EntityId::Value(roles.cursor), instruction);
                uses.protect(result(instruction)?);
            }
            _ => {
                return Err(failure(
                    "temporary form unexpected body operation",
                    instruction,
                ));
            }
        }
    }
    need(
        calls == 1
            && live.is_empty()
            && ended_roots.as_slice() == [roots[1], roots[0]]
            && ops.first().map(|i| i.id) == Some(key.id)
            && g.position(key)? < g.position(val)?
            && ops.last().map(|i| i.id) == Some(next.id)
            && g.edges.iter().all(|(_, edge)| {
                edge.arguments.iter().all(
                    |entity| !matches!(entity, EntityId::Loan(loan) if lineage.contains_key(loan)),
                )
            }),
        "temporary form complete body and no component escape",
        (&ops, ended_roots),
    )?;
    Ok(next)
}
