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

pub(in crate::ssa) mod unit_9 {
//! 同次 unit 分析逐完整 qualified For/source/binding 身份查询；不读取私有 acquired 表。
use crate::ssa::map_provider_exits_pipeline_tests::temporary::{
    crate::ssa::map_provider_exits_pipeline_tests::temporary::Report,
    ast::{Anchors, Function, Input},
};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn declaration(
    names: &ValidatedCompilationUnitNames,
    function: &Function,
    unit: SourceUnitId,
) -> Option<DeclarationId> {
    let found = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| {
            d.source_unit() == unit
                && d.root() == function.item
                && d.name_span() == function.name_span
                && d.kind() == SymbolKind::Function
                && d.name() == function.name
        })
        .collect::<Vec<_>>();
    match found.as_slice() {
        [d] => Some(d.id()),
        _ => None,
    }
}

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn binding_at(names: &ValidatedCompilationUnitNames, id: UnitSymbolId, span: Span) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|source| {
            source.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol()
                    && s.span() == span
                    && s.kind() == SymbolKind::ForBinding
                    && !s.is_synthetic()
            })
        })
}

pub(in crate::ssa) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    diagnostic!(
        "SPEC0331 TEMPORARY UNIT REAL typed_maps={:#?} maps={:#?} drops={:#?} conditional_drops={:#?} contracts={:#?} loans={:#?} deliveries={:#?}",
        typed.map_iterations(),
        owned.map_iterations(),
        owned.drops(),
        owned.conditional_receiver_drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(a) = anchors else { return };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit temporary exactly one actual source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    report.check(
        "unit temporary original source and exact two typed/owned plans",
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        (unit, typed.map_iterations(), owned.map_iterations()),
    );
    signatures(a, names, typed, owned, unit, report);
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        report.check(
            "unit temporary actual qualified AST site",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
            (
                statement,
                source,
                site.body,
                crate::ssa::map_provider_exits_pipeline_tests::fixture::location(&f.sources, site.source_span),
            ),
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(statement),
            owned.map_iteration(statement),
        ) else {
            report.check(
                "unit temporary actual qualified site descriptor/plan",
                false,
                (
                    statement,
                    source,
                    typed.map_iterations(),
                    owned.map_iterations(),
                ),
            );
            continue;
        };
        let constructor = if f.map {
            IntrinsicTypeConstructor::Map
        } else {
            IntrinsicTypeConstructor::MutableMap
        };
        let kind = if f.map {
            MapIterationKind::Map
        } else {
            MapIterationKind::MutableMap
        };
        report.check(
            "unit temporary exact qualified For/source and intrinsic String K/V",
            d.statement() == statement && d.source() == source && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
            (statement, source, d, p),
        );
        let owner = match p.source() {
            Some(UnitLoanTarget::Temporary(id)) => Some(*id),
            _ => None,
        };
        report.check(
            "unit temporary public category and qualified expression origin",
            typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                && owner == Some(source),
            (statement, source, p.source_access(), p.source_kind()),
        );
        if let Some(owner) = owner {
            owners.push(owner);
        }
        bindings.push(d.bindings());
        let factory = a
            .function(site.factory)
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(source);
        report.check(
            "unit temporary own real factory declaration and owned result",
            factory.is_some() && call.is_some_and(|call| {
                call.expression() == source
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == factory)
                    && call.receiver().is_none() && call.arguments().is_empty()
                    && call.return_type() == d.source_type()
                    && call.result_source() == CallableResultSource::Owned
            }),
            (statement, source, site.factory, factory, call),
        );
        report.check(
            "unit temporary distinct qualified Key/Value bindings and loans",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (statement, d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            report.check(
                "unit temporary actual marker/type/Shared component lineage",
                binding.source_unit() == unit
                    && binding_at(names, binding, site.bindings[i])
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[i].symbol() == binding
                    && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[i]
                    && loan.statement() == statement
                    && loan.source() == source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                (statement, binding, loan),
            );
        }
        let read = a
            .function("read")
            .and_then(|function| declaration(names, function, unit));
        let call_id = expression(site.read);
        let call = typed.call(call_id);
        report.check(
            "unit temporary own read declaration and two actual Borrow arguments",
            read.is_some() && call.is_some_and(|call| {
                call.expression() == call_id
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == read)
                    && call.receiver().is_none()
                    && call.arguments().len() == 2
                    && typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                    && typed.expression_type(call_id) == Some(call.return_type())
                    && call.arguments().iter().enumerate().all(|(i, argument)| {
                        argument.argument_index() == i
                            && argument.parameter_index() == i
                            && argument.mode() == ParameterMode::Borrow
                            && argument.parameter_type() == [d.key_type(), d.value_type()][i]
                            && typed.expression_type(expression(site.arguments[i]))
                                == Some(argument.parameter_type())
                            && reference(f, names, site.arguments[i]) == Some(d.bindings()[i])
                    })
            }),
            (statement, call_id, read, call),
        );
        let contracts = owned
            .call_argument_contracts_for(call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit temporary read exact qualified Borrow contracts",
            contracts.len() == 2
                && site.arguments.iter().enumerate().all(|(i, argument)| {
                    contracts
                        .iter()
                        .filter(|c| {
                            c.call() == call_id
                                && c.argument() == expression(*argument)
                                && c.parameter_index() == i
                                && c.parameter_type() == [d.key_type(), d.value_type()][i]
                                && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        })
                        .count()
                        == 1
                })
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|delivery| delivery.call() == call_id),
            (statement, &contracts),
        );
        let call_loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit temporary read loans retain this site's qualified bindings",
            call_loans.len() == 2 && site.arguments.iter().enumerate().all(|(i, argument)| {
                call_loans.iter().filter(|loan| loan.argument() == expression(*argument) && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == d.bindings()[i])).count() == 1
            }),
            (statement, &call_loans),
        );
        // unit 仅公开退出计划及同点完整动作；不虚构 cleanup_steps/condition/owner API。
        for exit in p.exits() {
            diagnostic!(
                "SPEC0331 TEMPORARY UNIT SITE statement={statement:?} source={source:?} owner={owner:?} factory={} exit={exit:#?} actual_at_point={:#?}",
                site.factory,
                owned.iteration_cleanup_at(exit.point())
            );
        }
    }
    report.check(
        "unit temporary two qualified sites retain independent origins and bindings",
        matches!(owners.as_slice(), [first, second] if first != second && first.source_unit() == unit && second.source_unit() == unit)
            && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
        (owners, bindings),
    );
}

fn signatures(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
    report: &mut Report,
) {
    for function in &a.functions {
        let id = declaration(names, function, unit);
        let signatures = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|signature| Some(signature.declaration()) == id)
            .filter_map(|signature| signature.callable())
            .collect::<Vec<_>>();
        report.check(
            "unit temporary actual function declaration and callable",
            id.is_some() && matches!(signatures.as_slice(), [signature] if signature.name_span() == function.name_span
                && matches!(signature.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)),
            (function, id, &signatures),
        );
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let return_type = a
            .sites
            .iter()
            .find(|site| site.factory == function.name)
            .and_then(|site| typed.map_iteration(UnitStatementId::new(unit, site.statement)))
            .map(|d| d.source_type());
        report.check(
            "unit temporary callable result and exact Borrow/value signature",
            signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && if function.name == "read" {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        && signature.parameters().len() == 2
                        && signature.parameters().iter().all(|p| {
                            p.mode() == ParameterMode::Borrow
                                && typed.types().get(p.ty())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                                && p.symbol().is_some_and(|id| {
                                    id.source_unit() == unit
                                        && owned.binding_kind(id)
                                            == Some(OwnershipBindingKind::Shared)
                                })
                        })
                } else {
                    signature.parameters().is_empty()
                        && if function.name == "entry" {
                            typed.types().get(signature.return_type())
                                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        } else {
                            Some(signature.return_type()) == return_type
                        }
                },
            (function, signature, return_type),
        );
    }
}
}
