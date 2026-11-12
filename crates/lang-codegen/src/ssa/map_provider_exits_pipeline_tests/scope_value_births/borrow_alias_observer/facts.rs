//! Exact published canonical alias storage and source lifetime from the original analysis.
mod callers {
    //! Original caller owns each make() result through the corresponding scanner invocation.
    use super::*;
    fn calls(
        sources: &SourceMap,
        p: &ParsedFile,
        i: &Input,
    ) -> Result<
        Vec<(
            lang_frontend::ast::ExpressionId,
            Vec<lang_frontend::ast::ExpressionId>,
        )>,
        Failure,
    > {
        let mut out = p
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if inside(i.entry, n.span()) && sources.slice(span(p, *callee)) == Ok("scan") => {
                    Some((id, arguments.iter().map(|a| a.value).collect::<Vec<_>>()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        out.sort_by_key(|(id, _)| span(p, *id).start());
        need(out.len() == 2, "alias original two entry invocations", &out)?;
        Ok(out)
    }
    pub(super) fn unit(
        sources: &SourceMap,
        p: &ParsedFile,
        i: &Input,
        n: &ValidatedCompilationUnitNames,
        t: &CompilationUnitTypes,
        o: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let u = one(
            n.names().source_units().iter(),
            "alias caller original unit",
        )?
        .source_unit();
        let e = |id| UnitExpressionId::new(u, id);
        let decl = |marker| {
            one(
                n.names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == u && d.name_span() == marker)
                    .map(|d| d.id()),
                "alias qualified caller declaration",
            )
        };
        let make =
            super::super::super::super::temporary_arguments::ast::function(sources, p, "make", None)?;
        for ((id, args), flag) in calls(sources, p, i)?.into_iter().zip(["true", "false"]) {
            need(
                args.len() == 2 && sources.slice(span(p, args[1])) == Ok(flag),
                "alias qualified original true/false operands",
                &args,
            )?;
            let call = t
                .call(e(id))
                .ok_or_else(|| failure("alias qualified actual scanner Call", id))?;
            let birth = t
                .call(e(args[0]))
                .ok_or_else(|| failure("alias qualified caller owned make", args[0]))?;
            need(
                call.target() == UnitCallTarget::Declaration(decl(i.scan_name)?)
                    && call.arguments().len() == 2
                    && birth.target() == UnitCallTarget::Declaration(decl(make.name)?)
                    && birth.arguments().is_empty()
                    && birth.result_source() == CallableResultSource::Owned
                    && birth.borrow_return().is_none(),
                "alias qualified original caller target/birth",
                (call, birth),
            )?;
            for (index, arg) in args.iter().copied().enumerate() {
                let d = call.arguments()[index];
                let loan = one(
                    o.loans()
                        .iter()
                        .filter(|l| l.call() == e(id) && l.argument() == e(arg)),
                    "alias qualified actual caller loan",
                )?;
                let contract = one(
                    o.call_argument_contracts_for(e(id))
                        .filter(|a| a.argument() == e(arg)),
                    "alias qualified actual caller argument contract",
                )?;
                need(
                    d.argument_index() == index
                        && d.parameter_index() == index
                        && d.mode() == ParameterMode::Borrow
                        && d.category() == ExpressionCategory::Temporary
                        && t.expression_type(e(arg)) == Some(d.parameter_type())
                        && loan.kind() == LoanKind::Shared
                        && loan.target() == &UnitLoanTarget::Temporary(e(arg))
                        && loan.begin_span() == span(p, arg)
                        && loan.end_span() == span(p, id)
                        && contract.parameter_index() == index
                        && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        && contract.parameter_type() == d.parameter_type()
                        && contract.category() == ExpressionCategory::Temporary,
                    "alias qualified caller exact slot/root/contract/return bounds",
                    (d, loan, contract),
                )?;
            }
            let drop = one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Temporary(e(args[0]))),
                "alias qualified caller sole actual Map cleanup",
            )?;
            need(
                drop.point() == UnitDropPoint::CallReturn(e(id))
                    && drop.value_origin() == span(p, args[0]),
                "alias qualified caller current Map ends after scanner",
                drop,
            )?;
        }
        Ok(())
    }
}
mod unit {
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
}
use super::super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Input, inside, span};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::{Expression, ForBinding, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
    type_checking::*,
};
#[derive(Debug)]
pub(super) struct Proof {
    pub declaration: Span,
    pub point: lang_frontend::ast::StatementId,
    pub endpoint: Span,
}
pub(super) fn site(p: &ParsedFile, i: &Input) -> Result<Site, Failure> {
    let (statement, source) = i.site.ok_or_else(|| failure("alias actual For site", i))?;
    let node = p.ast().statements().get(statement).unwrap();
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        body,
        ..
    } = node.payload()
    else {
        unreachable!()
    };
    let [NameMarker::Present(k), NameMarker::Present(v)] = names.as_slice() else {
        return Err(failure("alias actual K/V names", names));
    };
    let (read, arguments) = one(
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { arguments, .. } if inside(node.span(), n.span()) => {
                    Some((id, arguments))
                }
                _ => None,
            }),
        "alias actual sole For reader Call",
    )?;
    let [a, b] = arguments.as_slice() else {
        return Err(failure("alias actual reader argument slots", arguments));
    };
    Ok(Site {
        statement,
        span: node.span(),
        source,
        body: *body,
        bindings: [*k, *v],
        read,
        arguments: [a.value, b.value],
    })
}
pub(super) use unit::check as unit;
