//! 只用公开 single owner/condition/phi/incoming 锁当前 GO，不读取私有 presence bit。
use super::super::super::ast::{Anchors, Exit, Input};
use super::{Failure, failure, need, one};
use lang_frontend::{
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupOwnerValue, CleanupOwnerValueId,
        CleanupSelection, CleanupSelectorSource, DropPoint, IterationClosurePhiBinding,
        IterationPhiBoundary as B, IterationPhiIncomingKind as K, IterationPhiPresenceSource,
        MapIterationOwnershipPlan, OwnershipCheckedFile,
    },
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Current {
    pub initial: CleanupOwnerValueId,
    pub header: CleanupOwnerValueId,
    pub exit: CleanupOwnerValueId,
    pub header_condition: CleanupConditionId,
    pub exit_condition: CleanupConditionId,
}

pub(super) fn check(
    f: &Input,
    a: &Anchors,
    guard: SymbolId,
    owned: &OwnershipCheckedFile,
) -> Result<Current, Failure> {
    let outer = owned
        .map_iteration(a.sites[0].statement)
        .ok_or_else(|| failure("nested exits FE outer phi plan", &a.sites[0]))?;
    let inner = owned
        .map_iteration(a.sites[1].statement)
        .ok_or_else(|| failure("nested exits FE inner phi plan", &a.sites[1]))?;
    need(
        outer.closure_phis().is_empty()
            && outer.capture_graph().nodes().is_empty()
            && outer.closure_flow().header().is_empty()
            && outer.closure_flow().exit().is_empty(),
        "nested exits FE outer has no carried owner or capture graph",
        outer,
    )?;
    let outer_expected = [
        (
            K::Entry,
            DropPoint::AfterExpression(a.sites[0].source),
            B::Header,
        ),
        (
            K::Fallthrough,
            DropPoint::AfterStatement(a.sites[0].body),
            B::Header,
        ),
        (
            K::Exhaustion,
            DropPoint::LoopExit(a.sites[0].statement),
            B::Exit,
        ),
    ];
    need(
        outer.closure_phi_incomings().len() == outer_expected.len(),
        "nested exits FE outer exact three actual incomings",
        outer,
    )?;
    for (kind, point, boundary) in outer_expected {
        let incoming = one(
            outer
                .closure_phi_incomings()
                .iter()
                .filter(|v| v.kind() == kind),
            "nested exits FE unique actual outer incoming",
        )?;
        need(
            incoming.point() == point
                && incoming.boundary() == boundary
                && incoming.bindings().is_empty()
                && owned.cleanup_conditions().get(incoming.condition())
                    == Some(&CleanupCondition::Always),
            "nested exits FE exact outer incoming point/boundary/condition/no owner",
            incoming,
        )?;
    }
    need(
        inner.closure_phis().len() == 2 && inner.capture_graph().nodes().is_empty(),
        "nested exits FE inner exactly two owner phis and empty capture graph",
        inner,
    )?;
    for flow in [inner.closure_flow().header(), inner.closure_flow().exit()] {
        let binding = one(
            flow.iter(),
            "nested exits FE sole actual carried Guard flow",
        )?;
        need(
            binding.symbol() == guard && binding.origins().is_empty(),
            "nested exits FE exact ordinary GO flow without closure origins",
            binding,
        )?;
    }
    let header = phi(inner, a, guard, B::Header, owned)?;
    let exit = phi(inner, a, guard, B::Exit, owned)?;
    need(
        header.owner() != exit.owner()
            && header.availability_selector() != exit.availability_selector()
            && header.availability_condition() != exit.availability_condition(),
        "nested exits FE Header/Exit owner selector condition are distinct",
        (header, exit),
    )?;
    let entry = one(
        inner
            .closure_phi_incomings()
            .iter()
            .filter(|v| v.kind() == K::Entry),
        "nested exits FE actual inner Entry incoming",
    )?;
    let binding = one(
        entry.bindings().iter(),
        "nested exits FE actual Entry GO binding",
    )?;
    let value = one(
        binding.values().iter(),
        "nested exits FE actual Entry GO value",
    )?;
    let initial = value.source();
    need(
        initial != header.owner()
            && initial != exit.owner()
            && matches!(owned.cleanup_conditions().owner_value(initial),
            Some(CleanupOwnerValue::Expression { expression, origin })
            if *expression == a.guards[0].initializer && *origin == a.guards[0].span),
        "nested exits FE GO Entry source is actual construction owner",
        (value, a.guards[0].span),
    )?;
    let mut expected = vec![
        (
            K::Entry,
            DropPoint::AfterExpression(a.sites[1].source),
            B::Header,
        ),
        (
            K::Exhaustion,
            DropPoint::LoopExit(a.sites[1].statement),
            B::Exit,
        ),
    ];
    match f.exit {
        Exit::Continue => expected.push((
            K::Continue(a.transfer),
            DropPoint::ControlTransfer(a.transfer),
            B::Header,
        )),
        Exit::Break => expected.push((
            K::Break(a.transfer),
            DropPoint::ControlTransfer(a.transfer),
            B::Exit,
        )),
        Exit::Return => (),
    }
    need(
        inner.closure_phi_incomings().len() == expected.len(),
        "nested exits FE complete actual inner incoming set",
        inner.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in expected {
        let incoming = one(
            inner
                .closure_phi_incomings()
                .iter()
                .filter(|v| v.kind() == kind),
            "nested exits FE unique actual inner incoming",
        )?;
        let target = if boundary == B::Header { header } else { exit };
        need(
            incoming.point() == point
                && incoming.boundary() == boundary
                && owned.cleanup_conditions().get(incoming.condition())
                    == Some(&CleanupCondition::Always),
            "nested exits FE exact inner incoming kind/point/boundary/Always",
            incoming,
        )?;
        let binding = one(
            incoming.bindings().iter(),
            "nested exits FE sole nonempty GO incoming",
        )?;
        let value = one(
            binding.values().iter(),
            "nested exits FE sole GO incoming source value",
        )?;
        let entry = kind == K::Entry;
        need(
            binding.target() == target.owner()
                && binding.availability_selector() == target.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions
                && value.source() == if entry { initial } else { header.owner() }
                && value.condition() == binding.available_when()
                && if entry {
                    owned.cleanup_conditions().get(binding.available_when())
                        == Some(&CleanupCondition::Always)
                } else {
                    binding.available_when() == header.availability_condition()
                },
            "nested exits FE every GO incoming source/target/selector/availability and empty static capture fields",
            (incoming, target, value),
        )?;
    }
    Ok(Current {
        initial,
        header: header.owner(),
        exit: exit.owner(),
        header_condition: header.availability_condition(),
        exit_condition: exit.availability_condition(),
    })
}

fn phi<'a>(
    plan: &'a MapIterationOwnershipPlan,
    a: &Anchors,
    guard: SymbolId,
    boundary: B,
    owned: &OwnershipCheckedFile,
) -> Result<&'a IterationClosurePhiBinding, Failure> {
    let phi = one(
        plan.closure_phis()
            .iter()
            .filter(|p| p.boundary() == boundary),
        "nested exits FE unique actual GO boundary phi",
    )?;
    let table = owned.cleanup_conditions();
    let selector = table
        .selector(phi.availability_selector())
        .ok_or_else(|| failure("nested exits FE actual GO phi selector", phi))?;
    let Some(CleanupCondition::Choice {
        selector: choice,
        branches,
    }) = table.get(phi.availability_condition())
    else {
        return Err(failure("nested exits FE GO availability Choice", phi));
    };
    need(
        phi.symbol() == guard
            && phi.root_nodes().is_empty()
            && phi.capture_layout().is_empty()
            && phi.origins().is_empty()
            && phi.root_origins().next().is_none()
            && matches!(table.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
            statement, boundary: actual, symbol, origin
        }) if *statement == a.sites[1].statement && *actual == boundary
            && *symbol == guard && *origin == a.sites[1].span)
            && selector.source()
                == CleanupSelectorSource::IterationPhi {
                    statement: a.sites[1].statement,
                    boundary,
                    slot: 0,
                }
            && selector.control().is_none()
            && selector.selection() == CleanupSelection::IterationPresence
            && selector.origin() == a.sites[1].span
            && selector.branch_count() == 2
            && *choice == phi.availability_selector()
            && branches.len() == 2
            && table.get(branches[0]) == Some(&CleanupCondition::Never)
            && table.get(branches[1]) == Some(&CleanupCondition::Always),
        "nested exits FE exact GO phi owner/origin and Choice[Never,Always] selector",
        (phi, selector, branches),
    )?;
    Ok(phi)
}
