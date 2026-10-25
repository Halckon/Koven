//! Exact remove result, assertion edge/proof/take and the final owner births.

use std::collections::BTreeSet;

use lang_frontend::{
    ownership_checking::NonNullAssertionTransferKind,
    parser::{Expression, LiteralKind},
    type_checking::{AssertionFailureEffect, UnitExpressionId, UnitTypeId},
};

use super::*;
use crate::ssa::model::{Definition, Edge, TerminatorKind};

fn edges(function: &Function) -> Vec<&Edge> {
    function
        .blocks
        .iter()
        .flat_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Branch(edge)) => vec![edge],
                Some(TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }) => {
                    vec![when_true, when_false]
                }
                Some(TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                }) => {
                    vec![when_null, when_non_null]
                }
                _ => Vec::new(),
            },
        )
        .collect()
}

pub(super) fn same_owner(function: &Function, current: ValueId, birth: ValueId) -> Outcome<()> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![current];
    let ty = value_type(function, birth)?;
    while let Some(value) = pending.pop() {
        gate(
            "Resource owner transport keeps exact storage",
            value_type(function, value)? == ty,
            value,
        )?;
        if value == birth || !seen.insert(value) {
            continue;
        }
        let data = function
            .entity(EntityId::Value(value))
            .ok_or_else(|| missing("Resource transported entity", value))?;
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(missing(
                "Resource cleanup cannot substitute a same-typed foreign owner",
                data,
            ));
        };
        let incoming: Vec<_> = edges(function)
            .into_iter()
            .filter(|edge| edge.target == block)
            .collect();
        gate(
            "Resource transported owner has its real incoming column",
            !incoming.is_empty(),
            data,
        )?;
        for edge in incoming {
            let Some(EntityId::Value(parent)) = edge.arguments.get(index) else {
                return Err(missing("Resource owner incoming argument", edge));
            };
            // These frozen straight-line programs have only an assertion branch, no loop.
            gate(
                "Resource owner lineage is acyclic",
                !seen.contains(parent),
                (value, parent),
            )?;
            pending.push(*parent);
        }
    }
    Ok(())
}

pub(super) fn check(
    view: &OwnedCompilationUnitView<'_, '_>,
    parsed: &ParsedFile,
    function: &Function,
    nullable_type: UnitTypeId,
    nullable: SsaTypeId,
    inner: SsaTypeId,
) -> Outcome<Vec<(&'static str, ValueId)>> {
    let source_unit = view
        .names()
        .names()
        .index()
        .source_units()
        .iter()
        .find(|source| source.source_id() == parsed.source_id())
        .ok_or_else(|| missing("Resource transfer original source unit", parsed.source_id()))?
        .id();
    let mut births = Vec::new();
    let nulls: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableNull { .. }))
        .collect();
    let ordinary: Vec<_> = view
        .names()
        .names()
        .source_units()
        .iter()
        .filter(|source| source.source_unit() == source_unit)
        .flat_map(|source| source.resolution().symbols())
        .filter(|symbol| symbol.name() == "ordinary" && !symbol.is_synthetic())
        .collect();
    gate(
        "ordinary null retains its own operation",
        nulls.len() == ordinary.len(),
        (&nulls, &ordinary),
    )?;
    for null in nulls {
        let expressions: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                node.span() == null.origin.span()
                    && matches!(node.payload(), Expression::Literal(LiteralKind::Null))
            })
            .collect();
        let &(expression, _) = one(
            &expressions,
            "ordinary N comes from its original null expression",
        )?;
        gate(
            "ordinary null and real remove result share the exact N identity",
            null.operation == Operation::NullableNull { nullable }
                && value_type(function, result(null, 0)?)? == nullable
                && view
                    .types()
                    .expression_type(UnitExpressionId::new(source_unit, expression))
                    == Some(nullable_type),
            null,
        )?;
        births.push(("ordinary", result(null, 0)?));
    }
    let removes: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::MapRemove { .. }))
        .collect();
    let descriptors = view.types().map_removes();
    gate(
        "all original remove descriptors have exactly one real operation",
        removes.len() == descriptors.len(),
        (&removes, descriptors),
    )?;
    if descriptors.is_empty() {
        gate(
            "null-only source has no fake remove, assertion or take",
            view.ownership().non_null_assertions().is_empty()
                && function.instructions.iter().all(|instruction| {
                    !matches!(instruction.operation, Operation::NullableTake { .. })
                })
                && function.blocks.iter().all(|block| {
                    !matches!(
                        block.terminator.as_ref().map(|term| &term.kind),
                        Some(TerminatorKind::NullableBranch { .. })
                    )
                }),
            function,
        )?;
        return Ok(births);
    }
    let descriptor = one(descriptors, "one original Resource remove descriptor")?;
    let remove = *one(&removes, "one actual original Resource remove")?;
    let remove_span = parsed
        .ast()
        .expressions()
        .get(descriptor.expression().expression())
        .map_err(|error| missing("Resource remove original expression", error))?
        .span();
    let plan = one(
        view.ownership().non_null_assertions(),
        "one original Resource assertion ownership plan",
    )?;
    let assertion = plan.descriptor();
    let assertion_span = parsed
        .ast()
        .expressions()
        .get(assertion.expression().expression())
        .map_err(|error| missing("Resource assertion original expression", error))?
        .span();
    gate(
        "remove result and assertion plan retain the original qualified identity",
        remove.origin == Origin::Source(remove_span)
            && descriptor.expression().source_unit() == source_unit
            && descriptor.result_type() == nullable_type
            && assertion.operand() == descriptor.expression()
            && assertion.nullable_type() == nullable_type
            && view.types().non_null_assertions().contains(assertion)
            && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && plan.null_effect() == AssertionFailureEffect::Abort,
        (descriptor, plan, remove),
    )?;
    gate(
        "remove returns updated outer and one separate nullable owner",
        remove.results.len() == 2,
        remove,
    )?;
    let updated_outer = result(remove, 0)?;
    let wrapper = result(remove, 1)?;
    gate(
        "remove result uses N, not updated outer storage",
        value_type(function, wrapper)? == nullable
            && value_type(function, updated_outer)? != nullable,
        remove,
    )?;
    births.push(("outer", updated_outer));
    let branches: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| {
            let term = block.terminator.as_ref()?;
            matches!(term.kind, TerminatorKind::NullableBranch { .. }).then_some((block, term))
        })
        .collect();
    let &(header, branch) = one(&branches, "one actual assertion branch")?;
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view: proof,
    } = &branch.kind
    else {
        unreachable!("filtered nullable branch")
    };
    gate(
        "assertion tests the exact remove result once",
        *owner == wrapper && branch.origin == Origin::Source(assertion_span),
        branch,
    )?;
    let absent = function
        .block(when_null.target)
        .ok_or_else(|| missing("assertion null block", when_null))?;
    let present = function
        .block(when_non_null.target)
        .ok_or_else(|| missing("assertion present block", when_non_null))?;
    let columns: Vec<_> = when_non_null
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value == EntityId::Value(wrapper)).then_some(index))
        .collect();
    let &column = one(&columns, "one exact non-null owner column")?;
    let Some(EntityId::Value(carried)) = present.parameters.get(column).copied() else {
        return Err(missing("non-null owner parameter", present));
    };
    gate(
        "assertion has exact owner/proof edge identity and no null unwind",
        absent.id != present.id
            && when_null.arguments.get(column) == Some(&EntityId::Value(wrapper))
            && present.parameters.len() == when_non_null.arguments.len() + 1
            && present.parameters.last() == Some(&EntityId::Loan(*proof))
            && absent.parameters.len() == when_null.arguments.len()
            && absent.instructions.is_empty()
            && absent
                .terminator
                .as_ref()
                .is_some_and(|term| matches!(term.kind, TerminatorKind::Abort)),
        (header, absent, present),
    )?;
    for edge in [when_null, when_non_null] {
        let incoming: Vec<_> = edges(function)
            .into_iter()
            .filter(|actual| actual.target == edge.target)
            .collect();
        gate(
            "assertion successor has only its original incoming edge",
            incoming.as_slice() == [edge],
            incoming,
        )?;
    }
    let takes: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
        .collect();
    let take = *one(&takes, "one actual Resource NullableTake")?;
    gate(
        "NullableTake consumes the exact non-null wrapper/proof once",
        take.block == present.id
            && take.origin == Origin::Source(assertion_span)
            && take.operation
                == Operation::NullableTake {
                    owner: carried,
                    proof: *proof,
                }
            && take.results.len() == 1
            && value_type(function, result(take, 0)?)? == inner,
        take,
    )?;
    same_owner(function, carried, wrapper)?;
    for instruction in &function.instructions {
        if instruction.id != take.id {
            gate(
                "consumed nullable wrapper and proof have no other instruction user",
                !instruction.operation.entities().iter().any(|entity| {
                    [
                        EntityId::Value(wrapper),
                        EntityId::Value(carried),
                        EntityId::Loan(*proof),
                    ]
                    .contains(entity)
                }),
                instruction,
            )?;
        }
    }
    gate(
        "taken wrapper never returns or crosses a later edge",
        function
            .blocks
            .iter()
            .filter(|block| block.id != header.id)
            .all(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_none_or(|term| !term.kind.entities().contains(&EntityId::Value(carried)))
            }),
        carried,
    )?;
    births.push(("got", result(take, 0)?));
    Ok(births)
}
