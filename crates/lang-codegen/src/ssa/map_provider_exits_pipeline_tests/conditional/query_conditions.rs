//! 按真实 AST 独立列出五出口、条件树和 owner 入边；不调用 producer 的资格/证明 helper。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Input},
};
use lang_frontend::{
    name_resolution::{SymbolId, UnitSymbolId},
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
        CleanupSelection, CleanupSelectorId, CleanupSelectorSource, CompilationUnitOwnership,
        DropPoint, DropTarget, IterationCleanupAction as A, IterationExitKind as E,
        IterationPhiBoundary as Boundary, IterationPhiIncomingKind as K,
        IterationPhiPresenceSource, OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

// 第 i 个槽是实际第 i 个 If；true=0，false=1。空路径只接受 Always。
const PATHS: [&[usize]; 5] = [&[0], &[1, 0], &[1, 1, 0], &[1, 1, 1], &[]];

fn leaf(table: &CleanupConditions, id: CleanupConditionId, always: bool) -> Result<(), Failure> {
    need(
        table.get(id)
            == Some(if always {
                &CleanupCondition::Always
            } else {
                &CleanupCondition::Never
            }),
        "conditional FE exact condition leaf",
        (id, table.get(id), always),
    )
}

fn selected(
    table: &CleanupConditions,
    id: CleanupConditionId,
    expected: CleanupSelectorId,
    slot: usize,
) -> Result<CleanupConditionId, Failure> {
    let Some(CleanupCondition::Choice { selector, branches }) = table.get(id) else {
        return Err(failure(
            "conditional FE actual Choice node",
            (id, table.get(id)),
        ));
    };
    need(
        *selector == expected && branches.len() == 2 && slot < 2,
        "conditional FE exact Choice selector/domain",
        (id, expected, slot, table.get(id)),
    )?;
    leaf(table, branches[1 - slot], false)?;
    Ok(branches[slot])
}

fn path(
    f: &Input,
    a: &Anchors,
    table: &CleanupConditions,
    mut id: CleanupConditionId,
    choices: &[usize],
    selectors: &mut [Option<CleanupSelectorId>; 3],
) -> Result<(), Failure> {
    for (index, slot) in choices.iter().copied().enumerate() {
        let Some(CleanupCondition::Choice { selector, .. }) = table.get(id) else {
            return Err(failure(
                "conditional FE path requires Choice",
                (index, id, table.get(id)),
            ));
        };
        let actual = table
            .selector(*selector)
            .ok_or_else(|| failure("conditional FE actual selector", selector))?;
        let branch = &a.branches[index];
        need(
            actual.source() == CleanupSelectorSource::Control(branch.expression)
                && actual.selection() == CleanupSelection::Branch
                && actual.origin() == f.expression(branch.expression)?.1
                && actual.branch_count() == 2,
            "conditional FE selector is this actual If",
            (branch, actual),
        )?;
        if let Some(expected) = selectors[index] {
            need(
                *selector == expected,
                "conditional FE repeated If has same selector",
                (index, selector, expected),
            )?;
        } else {
            selectors[index] = Some(*selector);
        }
        id = selected(table, id, *selector, slot)?;
    }
    leaf(table, id, true)
}

pub(super) fn single(
    f: &Input,
    a: &Anchors,
    owned: &OwnershipCheckedFile,
    source: SymbolId,
    local: SymbolId,
    parameter: Span,
) -> Result<(), Failure> {
    need(
        a.branches.len() == 3,
        "conditional FE exact three controls",
        &a.branches,
    )?;
    let p = owned
        .map_iteration(a.statement)
        .ok_or_else(|| failure("conditional FE actual cleanup plan", a.statement))?;
    let d = p.descriptor();
    let table = owned.cleanup_conditions();
    let mut selectors = [None; 3];
    let exits = [
        (
            E::Continue(a.branches[0].transfer),
            DropPoint::ControlTransfer(a.branches[0].transfer),
        ),
        (
            E::Break(a.branches[1].transfer),
            DropPoint::ControlTransfer(a.branches[1].transfer),
        ),
        (
            E::Return(a.branches[2].transfer),
            DropPoint::ControlTransfer(a.branches[2].transfer),
        ),
        (E::Fallthrough, DropPoint::AfterStatement(a.body)),
        (E::Exhaustion, DropPoint::LoopExit(a.statement)),
    ];
    need(
        p.exits().len() == exits.len(),
        "conditional FE exact five-exit set",
        p.exits(),
    )?;
    let guards = owned
        .drops()
        .iter()
        .filter(|v| v.target() == DropTarget::Named(local))
        .collect::<Vec<_>>();
    need(
        guards.len() == 4,
        "conditional FE four Guard cleanup identities",
        &guards,
    )?;
    let guard_owner = one(
        guards.iter().filter(|v| v.point() == exits[0].1),
        "conditional FE first Guard cleanup",
    )?
    .owner()
    .ok_or_else(|| failure("conditional FE Guard initializer owner", &guards))?;
    need(
        matches!(table.owner_value(guard_owner), Some(CleanupOwnerValue::Expression { expression, origin })
        if *expression == a.guard && *origin == f.expression(a.guard)?.1),
        "conditional FE Guard owner is actual constructor",
        table.owner_value(guard_owner),
    )?;
    for guard in &guards {
        need(
            guard.value_origin() == a.local
                && guard.owner() == Some(guard_owner)
                && guard.condition().is_none()
                && guard.capture_slot().is_none()
                && guard.instance_address().is_none(),
            "conditional FE same lexical Guard and initializer owner",
            guard,
        )?;
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|v| v.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    need(
        drops.len() == if f.map { 3 } else { 0 }
            && drops.iter().all(|v| {
                v.value_origin() == parameter
                    && v.capture_slot().is_none()
                    && v.instance_address().is_none()
            }),
        "conditional FE source Drop set and parameter origin",
        &drops,
    )?;
    for (index, (kind, point)) in exits.into_iter().enumerate() {
        let exit = one(
            p.exits()
                .iter()
                .filter(|v| v.kind() == kind && v.point() == point),
            "conditional FE unique expected exit identity",
        )?;
        path(f, a, table, exit.condition(), PATHS[index], &mut selectors)?;
        let mut expected = Vec::new();
        if index != 4 {
            let guard = *one(
                guards.iter().copied().filter(|v| v.point() == point),
                "conditional FE per-path actual Guard Drop",
            )?;
            expected.extend([
                A::Drop(guard),
                A::EndBinding {
                    statement: a.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: a.statement,
                    symbol: d.key_binding(),
                },
            ]);
        }
        if [1, 2, 4].contains(&index) {
            expected.extend([A::FinishProvider(a.statement), A::EndSource(a.statement)]);
            if f.map {
                expected.push(A::Drop(*one(
                    drops.iter().copied().filter(|v| v.point() == point),
                    "conditional FE per-path source Drop",
                )?));
            }
        }
        need(
            exit.actions() == expected,
            "conditional FE independently expected complete exit actions",
            (exit, &expected),
        )?;
        // 同表 getter 仅作附加一致性；上面的 AST/参数/Guard 关系才建立预期动作。
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            complete == expected,
            "conditional FE cleanup accessor consistency",
            (point, complete, expected),
        )?;
    }
    need(
        selectors.iter().all(Option::is_some)
            && selectors[0] != selectors[1]
            && selectors[0] != selectors[2]
            && selectors[1] != selectors[2],
        "conditional FE three distinct actual If selectors",
        selectors,
    )?;
    let incoming = [
        (
            K::Entry,
            DropPoint::AfterExpression(a.source),
            Boundary::Header,
            4,
        ),
        (
            K::Continue(a.branches[0].transfer),
            exits[0].1,
            Boundary::Header,
            0,
        ),
        (
            K::Break(a.branches[1].transfer),
            exits[1].1,
            Boundary::Exit,
            1,
        ),
        (K::Fallthrough, exits[3].1, Boundary::Header, 3),
        (K::Exhaustion, exits[4].1, Boundary::Exit, 4),
    ];
    need(
        p.closure_phi_incomings().len() == incoming.len(),
        "conditional FE exactly Entry Continue Break Fallthrough Exhaustion",
        p.closure_phi_incomings(),
    )?;
    if !f.map {
        need(
            p.closure_phis().is_empty(),
            "conditional FE Borrow no source owner phi",
            p.closure_phis(),
        )?;
        for (kind, point, boundary, index) in incoming {
            let edge = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|v| v.kind() == kind),
                "conditional FE Borrow actual incoming",
            )?;
            need(
                edge.point() == point && edge.boundary() == boundary && edge.bindings().is_empty(),
                "conditional FE Borrow exact incoming with no owner bindings",
                edge,
            )?;
            path(f, a, table, edge.condition(), PATHS[index], &mut selectors)?;
        }
        return Ok(());
    }
    let header = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == source && v.boundary() == Boundary::Header),
        "conditional FE source Header phi",
    )?;
    let exit = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == source && v.boundary() == Boundary::Exit),
        "conditional FE source Exit phi",
    )?;
    need(
        p.closure_phis().len() == 2
            && header.owner() != exit.owner()
            && header.availability_selector() != exit.availability_selector()
            && header.availability_condition() != exit.availability_condition(),
        "conditional FE distinct Header/Exit owner selector and presence",
        p.closure_phis(),
    )?;
    for phi in [header, exit] {
        need(
            matches!(table.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
            statement, boundary, symbol, origin }) if *statement == a.statement && *boundary == phi.boundary()
                && *symbol == source && *origin == a.span)
                && phi.root_nodes().is_empty()
                && phi.capture_layout().is_empty()
                && phi.origins().is_empty(),
            "conditional FE source phi has exact statement symbol boundary and origin",
            phi,
        )?;
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("conditional FE actual source presence selector", phi))?;
        need(
            selector.source()
                == CleanupSelectorSource::IterationPhi {
                    statement: a.statement,
                    boundary: phi.boundary(),
                    slot: 0,
                }
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.origin() == a.span
                && selector.branch_count() == 2
                && !selectors.contains(&Some(phi.availability_selector())),
            "conditional FE exact source presence selector",
            selector,
        )?;
        let present = selected(
            table,
            phi.availability_condition(),
            phi.availability_selector(),
            1,
        )?;
        leaf(table, present, true)?;
    }
    for (index, point) in [(1, exits[1].1), (2, exits[2].1), (4, exits[4].1)] {
        let drop = one(
            drops.iter().copied().filter(|v| v.point() == point),
            "conditional FE exact source cleanup point",
        )?;
        need(
            drop.owner() == Some(header.owner()),
            "conditional FE source Drop consumes current Header owner",
            drop,
        )?;
        let condition = drop
            .condition()
            .ok_or_else(|| failure("conditional FE source Drop presence", drop))?;
        let present = selected(table, condition, header.availability_selector(), 1)?;
        path(f, a, table, present, PATHS[index], &mut selectors)?;
    }
    for (kind, point, boundary, index) in incoming {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|v| v.kind() == kind),
            "conditional FE exact source incoming",
        )?;
        need(
            edge.point() == point && edge.boundary() == boundary,
            "conditional FE incoming actual control point and boundary",
            edge,
        )?;
        path(f, a, table, edge.condition(), PATHS[index], &mut selectors)?;
        let binding = one(
            edge.bindings().iter(),
            "conditional FE sole source incoming binding",
        )?;
        let target = if boundary == Boundary::Header {
            header
        } else {
            exit
        };
        need(
            binding.target() == target.owner()
                && binding.availability_selector() == target.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "conditional FE source incoming complete target and no implicit owner fields",
            binding,
        )?;
        if kind == K::Entry {
            let value = one(
                binding.values().iter(),
                "conditional FE real parameter seed",
            )?;
            leaf(table, binding.available_when(), true)?;
            need(
                value.condition() == binding.available_when()
                    && matches!(table.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol, origin })
                    if *symbol == source && *origin == parameter),
                "conditional FE Header seeded by actual parameter owner",
                value,
            )?;
        } else if boundary == Boundary::Header {
            let value = one(
                binding.values().iter(),
                "conditional FE one current owner on backedge",
            )?;
            need(
                value.source() == header.owner() && value.condition() == binding.available_when(),
                "conditional FE backedge current Header owner and availability",
                value,
            )?;
            let present = selected(
                table,
                binding.available_when(),
                header.availability_selector(),
                1,
            )?;
            path(f, a, table, present, PATHS[index], &mut selectors)?;
        } else {
            need(
                binding.values().is_empty(),
                "conditional FE ended owner absent from Exit incoming",
                binding,
            )?;
            leaf(table, binding.available_when(), false)?;
        }
    }
    Ok(())
}

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
