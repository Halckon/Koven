//! 按真实 AST 独立列出 Unit 五出口；条件路径和 owner 入边由实际 SSA 身份查询等强承接。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Input},
};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    owned: &CompilationUnitOwnership,
    source: UnitSymbolId,
    local: UnitSymbolId,
    parameter: Span,
) -> Result<(), Failure> {
    need(
        a.branches.len() == 3 && source.source_unit() == local.source_unit(),
        "conditional FE unit actual controls and source unit",
        (&a.branches, source, local),
    )?;
    let unit = source.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, a.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("conditional FE unit cleanup plan", statement))?;
    let d = p.descriptor();
    let exits = [
        (
            UE::Continue(expression(a.branches[0].transfer)),
            UnitDropPoint::ControlTransfer(expression(a.branches[0].transfer)),
        ),
        (
            UE::Break(expression(a.branches[1].transfer)),
            UnitDropPoint::ControlTransfer(expression(a.branches[1].transfer)),
        ),
        (
            UE::Return(expression(a.branches[2].transfer)),
            UnitDropPoint::ControlTransfer(expression(a.branches[2].transfer)),
        ),
        (
            UE::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, a.body)),
        ),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement)),
    ];
    need(
        p.exits().len() == exits.len(),
        "conditional FE unit exact five-exit set",
        p.exits(),
    )?;
    let guards = owned
        .drops()
        .iter()
        .filter(|v| v.target() == UnitDropTarget::Named(local))
        .collect::<Vec<_>>();
    need(
        guards.len() == 4 && guards.iter().all(|v| v.value_origin() == a.local),
        "conditional FE unit four source-qualified lexical Guard drops",
        &guards,
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|v| v.target() == UnitDropTarget::Named(source))
        .collect::<Vec<_>>();
    need(
        drops.len() == if f.map { 3 } else { 0 }
            && drops.iter().all(|v| v.value_origin() == parameter),
        "conditional FE unit source drops from actual parameter",
        &drops,
    )?;
    for (index, (kind, point)) in exits.into_iter().enumerate() {
        let exit = one(
            p.exits()
                .iter()
                .filter(|v| v.kind() == kind && v.point() == point),
            "conditional FE unit unique expected exit",
        )?;
        let mut expected = Vec::new();
        if index != 4 {
            expected.extend([
                UA::Drop(*one(
                    guards.iter().copied().filter(|v| v.point() == point),
                    "conditional FE unit actual per-path Guard drop",
                )?),
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ]);
        }
        if [1, 2, 4].contains(&index) {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            if f.map {
                expected.push(UA::Drop(*one(
                    drops.iter().copied().filter(|v| v.point() == point),
                    "conditional FE unit actual per-path source drop",
                )?));
            }
        }
        need(
            exit.actions() == expected,
            "conditional FE unit independently expected complete cleanup order",
            (exit, &expected),
        )?;
        // 公开 unit plan 没有 owner/condition/phi 表；不得在此补造对应事实。
        // 此 getter 与 exit.actions 同表，只是附加一致性检查。
        need(
            owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "conditional FE unit cleanup accessor consistency",
            (point, expected),
        )?;
    }
    Ok(())
}
