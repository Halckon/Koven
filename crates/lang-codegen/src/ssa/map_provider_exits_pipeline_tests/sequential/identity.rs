//! Independent queries of the original AST/public FE products and the real SSA graph.
mod graph;
use super::{Case, Failure};
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{failure, need},
    query_inputs::one,
};
pub(super) use graph::ssa;
use lang_frontend::{
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropTarget, IterationCleanupAction as A, IterationExitKind as E,
        LoanTarget, MapIterationSourceAccess, OwnershipCheckedFile, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::{ParsedFile, Statement},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, ParameterMode, TypedFile},
};

pub(super) fn single(
    c: &Case,
    s: &SourceMap,
    p: &ParsedFile,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let mut descriptors = t.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|d| {
        p.ast()
            .statements()
            .get(d.statement())
            .unwrap()
            .span()
            .start()
    });
    need(
        descriptors.len() == 2 && o.map_iterations().len() == 2,
        "single complete two plans",
        &descriptors,
    )?;
    for (index, d) in descriptors.into_iter().enumerate() {
        let plan = o
            .map_iteration(d.statement())
            .ok_or_else(|| failure("single plan", d))?;
        let Some(LoanTarget::Place(place)) = plan.source() else {
            return Err(failure("single direct Borrow source", plan));
        };
        let root = place.root();
        let source = p.ast().expressions().get(d.source()).unwrap().span();
        let expected = if c.slots[index] == 0 {
            "first"
        } else {
            "second"
        };
        let symbol = one(
            n.symbols().iter().filter(|v| v.id() == root),
            "single root symbol",
        )?;
        let reference = one(
            n.references()
                .iter()
                .filter(|r| r.span() == source && r.namespace() == Namespace::Value),
            "single original reference",
        )?;
        let (signature, slot) = one(
            t.callables().iter().flat_map(|f| {
                f.parameter_symbols()
                    .iter()
                    .enumerate()
                    .filter_map(move |(slot, v)| (*v == Some(root)).then_some((f, slot)))
            }),
            "single parameter slot",
        )?;
        need(
            place.is_root()
                && plan.source_access() == MapIterationSourceAccess::Shared
                && s.slice(source) == Ok(expected)
                && symbol.name() == expected
                && matches!(reference.target(),ReferenceTarget::Symbol(id) if *id==root)
                && slot == c.slots[index]
                && signature.parameter_symbols().len() == c.parameters
                && signature.parameters()[slot].mode == ParameterMode::Borrow
                && !o
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == DropTarget::Named(root)),
            "single exact Borrow root/slot/no owner drop",
            plan,
        )?;
        need(
            plan.descriptor() == d
                && plan.loans().iter().all(|l| {
                    l.statement() == d.statement()
                        && l.source() == d.source()
                        && Some(l.origin()) == plan.source()
                }),
            "single same provider component origins",
            plan,
        )?;
        need(plan.exits().len() == 2, "single complete exit count", plan)?;
        for exit in plan.exits() {
            let actual = exit
                .actions()
                .iter()
                .map(|a| match a {
                    A::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.value_binding() =>
                    {
                        "value"
                    }
                    A::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.key_binding() =>
                    {
                        "key"
                    }
                    A::FinishProvider(id) if *id == d.statement() => "finish",
                    A::EndSource(id) if *id == d.statement() => "end",
                    A::Drop(_) => "drop",
                    _ => "unexpected",
                })
                .collect::<Vec<_>>();
            need(
                actual == cleanup(c, index, exit.kind() == E::Exhaustion),
                "single exact cleanup actions",
                (exit, actual),
            )?;
        }
    }
    Ok(())
}

pub(super) fn unit(
    c: &Case,
    s: &SourceMap,
    p: &ParsedFile,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let original = one(n.names().source_units().iter(), "unit original source")?;
    let unit = original.source_unit();
    let mut descriptors = t.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|d| {
        p.ast()
            .statements()
            .get(d.statement().statement())
            .unwrap()
            .span()
            .start()
    });
    need(
        descriptors.len() == 2 && o.map_iterations().len() == 2,
        "unit complete two plans",
        &descriptors,
    )?;
    for (index, d) in descriptors.into_iter().enumerate() {
        need(
            d.statement().source_unit() == unit,
            "unit original For source",
            d,
        )?;
        let plan = o
            .map_iteration(d.statement())
            .ok_or_else(|| failure("unit plan", d))?;
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            return Err(failure("unit direct Borrow source", plan));
        };
        let root = place.root();
        let source = p
            .ast()
            .expressions()
            .get(d.source().expression())
            .unwrap()
            .span();
        let expected = if c.slots[index] == 0 {
            "first"
        } else {
            "second"
        };
        let symbol = one(
            original
                .resolution()
                .symbols()
                .iter()
                .filter(|v| v.id() == root.symbol()),
            "unit root symbol",
        )?;
        let reference = one(
            n.names().references().iter().filter(|r| {
                r.source_unit() == unit
                    && r.span() == source
                    && r.namespace() == Some(Namespace::Value)
            }),
            "unit original reference",
        )?;
        let (signature, slot) = one(
            t.signatures()
                .declarations()
                .iter()
                .filter_map(|v| v.callable())
                .flat_map(|f| {
                    f.parameters()
                        .iter()
                        .enumerate()
                        .filter_map(move |(slot, v)| {
                            (v.symbol() == Some(root)).then_some((f, slot))
                        })
                }),
            "unit parameter slot",
        )?;
        need(
            place.is_root()
                && root.source_unit() == unit
                && plan.source_access() == UnitIterationSourceAccess::Shared
                && s.slice(source) == Ok(expected)
                && symbol.name() == expected
                && matches!(reference.target(),UnitReferenceTarget::Symbol(id) if *id==root)
                && slot == c.slots[index]
                && signature.parameters().len() == c.parameters
                && signature.parameters()[slot].mode() == ParameterMode::Borrow
                && !o
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(root)),
            "unit exact Borrow root/slot/no owner drop",
            plan,
        )?;
        need(
            plan.descriptor() == d
                && plan.loans().iter().all(|l| {
                    l.statement() == d.statement()
                        && l.source() == d.source()
                        && Some(l.origin()) == plan.source()
                }),
            "unit same provider component origins",
            plan,
        )?;
        need(plan.exits().len() == 2, "unit complete exit count", plan)?;
        for exit in plan.exits() {
            let actual = exit
                .actions()
                .iter()
                .map(|a| match a {
                    UA::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.value_binding() =>
                    {
                        "value"
                    }
                    UA::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.key_binding() =>
                    {
                        "key"
                    }
                    UA::FinishProvider(id) if *id == d.statement() => "finish",
                    UA::EndSource(id) if *id == d.statement() => "end",
                    UA::Drop(_) => "drop",
                    _ => "unexpected",
                })
                .collect::<Vec<_>>();
            need(
                actual == cleanup(c, index, exit.kind() == UE::Exhaustion),
                "unit exact cleanup actions",
                (exit, actual),
            )?;
        }
    }
    Ok(())
}

fn cleanup(c: &Case, index: usize, exhaustion: bool) -> Vec<&'static str> {
    if exhaustion {
        return vec!["finish", "end"];
    }
    let mut expected = Vec::new();
    if index == 0 && (c.label.ends_with("-break") || c.label.ends_with("-continue")) {
        expected.push("drop")
    }
    expected.extend(["value", "key"]);
    if index == 0 && c.label.ends_with("-break") {
        expected.extend(["finish", "end"])
    }
    expected
}

pub(super) fn for_spans(p: &ParsedFile) -> Vec<lang_frontend::source::Span> {
    let mut spans = p
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| matches!(n.payload(), Statement::For { .. }).then_some(n.span()))
        .collect::<Vec<_>>();
    spans.sort_by_key(|s| s.start());
    spans
}
