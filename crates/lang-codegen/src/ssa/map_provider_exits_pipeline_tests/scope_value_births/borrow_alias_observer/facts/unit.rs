//! The same original alias and canonical source IDs, qualified by their actual source unit.
use super::*;
fn root(n: &ValidatedCompilationUnitNames, marker: Span) -> Result<UnitSymbolId, Failure> {
    let (u, s) = one(
        n.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "alias actual qualified original symbol",
    )?;
    one(
        n.names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(id) if id.source_unit() == u && id.symbol() == s => {
                    Some(*id)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter(),
        "alias actual qualified referenced symbol",
    )
}
use std::collections::BTreeSet;
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::borrow_alias_observer) fn check(
    c: &Case,
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    callers::unit(sources, p, i, n, t, o)?;
    let u = one(
        n.names().source_units().iter(),
        "alias original qualified source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let source = root(n, i.source)?;
    let alias = root(n, i.alias)?;
    let ty = t
        .symbol_type(source)
        .ok_or_else(|| failure("alias qualified source type", source))?;
    let init = one(
        n.names()
            .references()
            .iter()
            .filter(|r| r.span() == span(p, i.initializer)),
        "alias qualified original initializer reference",
    )?;
    let scanner = one(
        t.signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|s| s.name_span() == i.scan_name),
        "alias qualified actual scanner signature",
    )?;
    need(
        scanner.parameters().len() == 2
            && scanner.parameters()[0].symbol() == Some(source)
            && scanner.parameters()[0].mode() == ParameterMode::Borrow
            && scanner.parameters()[0].ty() == ty
            && o.binding_kind(source) == Some(OwnershipBindingKind::Shared)
            && init.target() == &UnitReferenceTarget::Symbol(source)
            && t.symbol_type(alias) == Some(ty)
            && t.expression_type(e(i.initializer)) == Some(ty)
            && t.expression_category(e(i.initializer)) == Some(ExpressionCategory::Place)
            && matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|t0|t.types().get(*t0)==Some(&UnitTypeKind::Builtin(BuiltinType::String))))
            && !o.formed_owned_locals().contains(&alias),
        "alias qualified real Borrow source and non-owning concrete Map alias",
        (scanner, source, alias),
    )?;
    let binding = one(
        o.borrow_results().bindings().iter(),
        "alias qualified sole BorrowBindingFact",
    )?;
    let end = one(
        o.borrow_results().ends().iter(),
        "alias qualified sole published body end",
    )?;
    need(
        binding.binding() == alias
            && binding.initializer() == e(i.initializer)
            && binding.storage() == BorrowBindingStorage::BorrowedStorage
            && matches!(binding.origin(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==source)
            && binding.parent().is_none()
            && binding.source_loan().is_none()
            && binding.marker_span() == i.marker
            && end.binding() == alias
            && end.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(u, i.endpoint))
            && !o
                .drops()
                .iter()
                .any(|d| matches!(d.target(),UnitDropTarget::Named(s)if s==source||s==alias)),
        "alias qualified exact canonical root/storage/parent/marker/actual statement endpoint",
        (binding, end),
    )?;
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "alias qualified actual provider/control set",
        o.map_iterations(),
    )?;
    let decl = |scope| {
        one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == u && inside(scope, d.name_span()))
                .map(|d| d.id()),
            "alias actual qualified callable declaration",
        )
    };
    if c.provider {
        let site = site(p, i)?;
        let statement = UnitStatementId::new(u, site.statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("alias qualified actual shared For plan", &site))?;
        let d = plan.descriptor();
        let name = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, site.source)),
            "alias qualified actual For alias Name",
        )?;
        need(
            t.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && d.source_type() == ty
                && name.target() == &UnitReferenceTarget::Symbol(alias)
                && plan.source() == Some(binding.origin())
                && plan.source_access() == UnitIterationSourceAccess::Shared,
            "alias qualified surface alias and canonical parameter source/access",
            plan,
        )?;
        let read = t
            .call(e(site.read))
            .ok_or_else(|| failure("alias qualified actual reader Call", site.read))?;
        need(
            read.target() == UnitCallTarget::Declaration(decl(i.reader)?)
                && read.arguments().len() == 2,
            "alias qualified actual reader target",
            read,
        )?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let s = root(n, site.bindings[index])?;
            let published = &plan.loans()[index];
            let reference = one(
                n.names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == span(p, site.arguments[index])),
                "alias qualified actual component use",
            )?;
            let loan = one(
                o.loans().iter().filter(|l| {
                    l.call() == e(site.read) && l.argument() == e(site.arguments[index])
                }),
                "alias qualified original reader loan",
            )?;
            need(
                d.bindings()[index] == s
                    && reference.target() == &UnitReferenceTarget::Symbol(s)
                    && o.binding_kind(s) == Some(OwnershipBindingKind::Shared)
                    && published.statement() == statement
                    && published.source() == e(site.source)
                    && published.component() == component
                    && published.binding() == s
                    && published.kind() == LoanKind::Shared
                    && Some(published.origin()) == plan.source()
                    && read.arguments()[index].argument_index() == index
                    && read.arguments()[index].parameter_index() == index
                    && read.arguments()[index].mode() == ParameterMode::Borrow
                    && read.arguments()[index].parameter_type()
                        == [d.key_type(), d.value_type()][index]
                    && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==s)
                    && loan.begin_span() == span(p, site.arguments[index])
                    && loan.end_span() == span(p, site.read),
                "alias qualified complete canonical K/V and synchronous delivery/loan bounds",
                (published, loan),
            )?;
        }
        for (kind, point, actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.body)),
                vec![
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UnitIterationExitKind::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UnitIterationCleanupAction::FinishProvider(statement),
                    UnitIterationCleanupAction::EndSource(statement),
                ],
            ),
        ] {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "alias qualified actual complete normal exit",
            )?;
            need(
                plan.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "alias qualified exact normal component/provider cleanup before alias scope end",
                exit,
            )?;
        }
    } else {
        let call = i.call.unwrap();
        let d = t
            .call(e(call))
            .ok_or_else(|| failure("alias qualified control actual Call", call))?;
        let Expression::Call { arguments, .. } = p.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        let arg = arguments[0].value;
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == e(call) && l.argument() == e(arg)),
            "alias qualified control loan",
        )?;
        let contract = one(
            o.call_argument_contracts_for(e(call))
                .filter(|v| v.argument() == e(arg)),
            "alias qualified control real Borrow argument contract",
        )?;
        need(
            d.target() == UnitCallTarget::Declaration(decl(i.observe)?)
                && matches!(d.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty&&a.category()==ExpressionCategory::Place)
                && loan.kind() == LoanKind::Shared
                && loan.target() == binding.origin()
                && loan.begin_span() == span(p, arg)
                && loan.end_span() == span(p, call)
                && contract.parameter_index() == 0
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.parameter_type() == ty
                && contract.category() == ExpressionCategory::Place,
            "alias qualified control uses exact canonical root until call completion",
            (d, loan, contract),
        )?;
    }
    Ok(Proof {
        declaration: i.declaration,
        point: i.endpoint,
        endpoint: p.ast().statements().get(i.endpoint).unwrap().span(),
    })
}
