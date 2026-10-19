//! Exact published canonical alias storage and source lifetime from the original analysis.
mod callers;
mod unit;
use super::super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Input, inside, span, symbol};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
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
pub(super) fn single(
    c: &Case,
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    callers::single(sources, p, i, n, t, o)?;
    let source = symbol(n, i.source)?;
    let alias = symbol(n, i.alias)?;
    let ty = t
        .symbol_type(source)
        .ok_or_else(|| failure("alias original source type", source))?;
    let initializer = one(
        n.references()
            .iter()
            .filter(|r| r.span() == span(p, i.initializer)),
        "alias actual initializer Name reference",
    )?;
    need(
        t.parameter_mode(source) == Some(ParameterMode::Borrow)
            && o.binding_kind(source) == Some(OwnershipBindingKind::Shared)
            && initializer.target() == &ReferenceTarget::Symbol(source)
            && t.symbol_type(alias) == Some(ty)
            && t.expression_type(i.initializer) == Some(ty)
            && t.expression_category(i.initializer) == Some(ExpressionCategory::Place)
            && matches!(t.types().get(ty),Some(TypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|t0|t.types().get(*t0)==Some(&TypeKind::Builtin(BuiltinType::String))))
            && !o.formed_owned_locals().contains(&alias),
        "alias exact original Borrow parameter and non-owning named Map storage",
        (source, alias, ty),
    )?;
    let binding = one(
        o.borrow_results().bindings().iter(),
        "alias sole actual BorrowBindingFact",
    )?;
    let end = one(
        o.borrow_results().ends().iter(),
        "alias sole actual BorrowBindingEndFact",
    )?;
    need(
        binding.binding() == alias
            && binding.initializer() == i.initializer
            && binding.storage() == BorrowBindingStorage::BorrowedStorage
            && matches!(binding.origin(),LoanTarget::Place(path)if path.is_root()&&path.root()==source)
            && binding.parent().is_none()
            && binding.source_loan().is_none()
            && binding.marker_span() == i.marker
            && end.binding() == alias
            && end.point() == DropPoint::AfterStatement(i.endpoint)
            && !o
                .drops()
                .iter()
                .any(|d| matches!(d.target(),DropTarget::Named(s)if s==source||s==alias)),
        "alias exact canonical root/parent/source-loan/marker/actual statement end and no owned cleanup",
        (binding, end),
    )?;
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "alias exact real acquired provider set",
        o.map_iterations(),
    )?;
    if c.provider {
        let site = site(p, i)?;
        let plan = o
            .map_iteration(site.statement)
            .ok_or_else(|| failure("alias actual shared For plan", &site))?;
        let d = plan.descriptor();
        let name = one(
            n.references()
                .iter()
                .filter(|r| r.span() == span(p, site.source)),
            "alias original For alias Name",
        )?;
        need(
            t.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && d.source_type() == ty
                && name.target() == &ReferenceTarget::Symbol(alias)
                && plan.source() == Some(binding.origin())
                && plan.source_access() == MapIterationSourceAccess::Shared
                && plan.closure_phis().is_empty(),
            "alias For surface Name and canonical parameter source remain distinct exact identities",
            plan,
        )?;
        let read = t
            .call(site.read)
            .ok_or_else(|| failure("alias actual reader Call", site.read))?;
        need(
            read.target() == CallableTarget::Source(symbol(n, i.reader_name)?)
                && read.arguments().len() == 2,
            "alias actual selected reader",
            read,
        )?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let s = symbol(n, site.bindings[index])?;
            let published = &plan.loans()[index];
            let reference = one(
                n.references()
                    .iter()
                    .filter(|r| r.span() == span(p, site.arguments[index])),
                "alias actual K/V use Name",
            )?;
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
                "alias actual synchronous reader loan",
            )?;
            let endpoint = one(
                o.loan_ends()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
                "alias actual reader independent endpoint",
            )?;
            need(
                d.bindings()[index] == s
                    && reference.target() == &ReferenceTarget::Symbol(s)
                    && o.binding_kind(s) == Some(OwnershipBindingKind::Shared)
                    && published.statement() == site.statement
                    && published.source() == site.source
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
                    && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==s)
                    && loan.begin_span() == span(p, site.arguments[index])
                    && loan.end_span() == span(p, site.read)
                    && endpoint.point() == LoanEndPoint::CallReturn(site.read),
                "alias complete ordered canonical K/V loan and reader delivery/endpoint",
                (published, loan, endpoint),
            )?;
        }
        for (kind, point, actions) in [
            (
                IterationExitKind::Fallthrough,
                DropPoint::AfterStatement(site.body),
                vec![
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                IterationExitKind::Exhaustion,
                DropPoint::LoopExit(site.statement),
                vec![
                    IterationCleanupAction::FinishProvider(site.statement),
                    IterationCleanupAction::EndSource(site.statement),
                ],
            ),
        ] {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "alias actual full normal provider exit",
            )?;
            need(
                plan.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && o.cleanup_conditions().get(exit.condition())
                        == Some(&CleanupCondition::Always)
                    && o.cleanup_steps()
                        .iter()
                        .filter_map(|(at, a)| (*at == point).then_some(*a))
                        .collect::<Vec<_>>()
                        == actions,
                "alias exact normal component/provider cleanup without ending parent alias",
                exit,
            )?;
        }
        need(
            plan.closure_phi_incomings().len() == 3,
            "alias complete original incoming set",
            plan.closure_phi_incomings(),
        )?;
        for (kind, point, boundary) in [
            (
                IterationPhiIncomingKind::Entry,
                DropPoint::AfterExpression(site.source),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Fallthrough,
                DropPoint::AfterStatement(site.body),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Exhaustion,
                DropPoint::LoopExit(site.statement),
                IterationPhiBoundary::Exit,
            ),
        ] {
            let edge = one(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "alias actual normal incoming",
            )?;
            need(
                edge.point() == point
                    && edge.boundary() == boundary
                    && edge.bindings().is_empty()
                    && o.cleanup_conditions().get(edge.condition())
                        == Some(&CleanupCondition::Always),
                "alias exact normal shared-source incoming/condition payload",
                edge,
            )?;
        }
    } else {
        let call = i.call.unwrap();
        let d = t
            .call(call)
            .ok_or_else(|| failure("alias actual control Call", call))?;
        let Expression::Call { arguments, .. } = p.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        let arg = arguments[0].value;
        let target = one(
            n.symbols().iter().filter(|s| {
                inside(i.observe, s.span())
                    && s.kind() == lang_frontend::name_resolution::SymbolKind::Function
            }),
            "alias original observe symbol",
        )?;
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == call && l.argument() == arg),
            "alias control actual loan",
        )?;
        let endpoint = one(
            o.loan_ends()
                .iter()
                .filter(|l| l.call() == call && l.argument() == arg),
            "alias control independent CallReturn endpoint",
        )?;
        need(
            d.target() == CallableTarget::Source(target.id())
                && matches!(d.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty&&a.category()==ExpressionCategory::Place)
                && loan.kind() == LoanKind::Shared
                && loan.target() == binding.origin()
                && loan.begin_span() == span(p, arg)
                && loan.end_span() == span(p, call)
                && endpoint.point() == LoanEndPoint::CallReturn(call),
            "alias control uses canonical original root through real alias until actual CallReturn",
            (d, loan, endpoint),
        )?;
    }
    Ok(Proof {
        declaration: i.declaration,
        point: i.endpoint,
        endpoint: p.ast().statements().get(i.endpoint).unwrap().span(),
    })
}
pub(super) use unit::check as unit;
