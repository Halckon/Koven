//! Same raw analysis: actual Fn0 Unit callback and the Map born inside its LambdaBody.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod owner;
mod unit;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
#[derive(Debug)]
pub(super) struct Proof {
    pub callback_drop: Span,
    pub source_drop: Span,
}
type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);
fn root(names: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
    one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == marker && !s.is_synthetic()),
        "A2 actual original local/function symbol",
    )
    .map(|s| s.id())
}
fn callable(names: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
    one(
        names.symbols().iter().filter(|s| {
            ast::inside(span, s.span())
                && s.kind() == lang_frontend::name_resolution::SymbolKind::Function
                && !s.is_synthetic()
        }),
        "A2 actual original callable symbol",
    )
    .map(|s| s.id())
}
pub(super) fn single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let (c, _, parsed, i) = input;
    let callback = root(names, i.callback)?;
    let source = root(names, i.source)?;
    let descriptor = owned
        .closure(i.lambda)
        .ok_or_else(|| failure("A2 actual original lambda closure", i.lambda))?;
    let invoke = typed
        .call(i.invoke)
        .ok_or_else(|| failure("A2 actual original FunctionValue Call", i.invoke))?;
    let function = typed
        .expression_type(i.lambda)
        .ok_or_else(|| failure("A2 actual lambda Fn0 type", i.lambda))?;
    let origin = owned
        .callable_origin(i.callee)
        .ok_or_else(|| failure("A2 actual callback public provenance", i.callee))?;
    let reference = one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, i.callee)),
        "A2 actual callback Name reference",
    )?;
    need(
        origin.expression() == i.callee
            && origin.span() == ast::span(parsed, i.callee)
            && origin.origin() == CallableOrigin::Lambda(i.lambda)
            && descriptor.expression() == i.lambda
            && !descriptor.move_owned()
            && owned.captures_of(i.lambda).next().is_none()
            && reference.target() == &ReferenceTarget::Symbol(callback)
            && typed.expression_type(i.callee) == Some(function)
            && typed.symbol_type(callback) == Some(function)
            && owned.formed_owned_locals().contains(&callback)
            && matches!(typed.types().get(function),Some(TypeKind::Function{move_only:false,parameters,return_type})if parameters.is_empty()&&*return_type==invoke.return_type()&&typed.types().get(*return_type)==Some(&TypeKind::Builtin(BuiltinType::Unit)))
            && invoke.target() == CallableTarget::FunctionValue
            && invoke.receiver().is_none()
            && invoke.arguments().is_empty()
            && invoke.instance().type_arguments().is_empty()
            && invoke.borrow_return().is_none(),
        "A2 exact original zero-capture Fn0 Unit callback ABI",
        (descriptor, invoke, reference),
    )?;
    let callback_drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(callback)),
        "A2 unique actual callback cleanup",
    )?;
    need(
        callback_drop.point() == DropPoint::CallReturn(i.invoke)
            && callback_drop.value_origin() == i.callback
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Temporary(i.lambda)),
        "A2 exact callback last-use CallReturn and no duplicate lambda Drop",
        callback_drop,
    )?;
    let call = typed
        .call(i.scan_call)
        .ok_or_else(|| failure("A2 actual entry scanner call", i.scan_call))?;
    need(
        call.target() == CallableTarget::Source(root(names, i.scan.name)?)
            && call.arguments().is_empty(),
        "A2 original entry calls original scanner",
        call,
    )?;
    let birth = typed
        .call(i.birth)
        .ok_or_else(|| failure("A2 actual internal make() birth", i.birth))?;
    let ty = birth.return_type();
    need(
        birth.target() == CallableTarget::Source(root(names, i.make.name)?)
            && birth.arguments().is_empty()
            && birth.result_source() == CallableResultSource::Owned
            && birth.borrow_return().is_none()
            && typed.expression_type(i.birth) == Some(ty)
            && typed.symbol_type(source) == Some(ty)
            && owned.formed_owned_locals().contains(&source)
            && matches!(typed.types().get(ty),Some(TypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|t|typed.types().get(*t)==Some(&TypeKind::Builtin(BuiltinType::String))))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Temporary(i.birth)),
        "A2 actual concrete Map owner born inside true LambdaBody",
        birth,
    )?;
    let source_drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(source)),
        "A2 unique actual internal Map cleanup",
    )?;
    need(
        source_drop.value_origin() == i.source
            && source_drop.point()
                == if let Some(site) = &i.site {
                    DropPoint::LoopExit(site.statement)
                } else {
                    DropPoint::CallReturn(i.observe_call.unwrap())
                },
        "A2 exact internal Map current last-use cleanup",
        source_drop,
    )?;
    need(
        typed.map_iterations().len() == usize::from(c.provider)
            && owned.map_iterations().len() == usize::from(c.provider),
        "A2 exact internal For/control plan set",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("A2 actual internal Map ownership plan", site))?;
        let d = p.descriptor();
        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.source)),
            "A2 actual internal For source Name",
        )?;
        need(
            typed.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && d.source_type() == ty
                && reference.target() == &ReferenceTarget::Symbol(source)
                && p.source_access() == MapIterationSourceAccess::Owned
                && matches!(p.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==source),
            "A2 For source is actual lambda-local owned result",
            p,
        )?;
        owner::single(
            &owner::Input {
                site: i.site.as_ref(),
                birth: i.birth,
                birth_span: i.birth_span,
            },
            source,
            p,
            owned,
            source_drop,
        )?;
        let read = typed
            .call(site.read)
            .ok_or_else(|| failure("A2 actual read Call", site.read))?;
        need(
            read.target() == CallableTarget::Source(callable(names, i.reader)?)
                && read.arguments().len() == 2,
            "A2 actual selected reader",
            read,
        )?;
        for n in 0..2 {
            let binding = root(names, site.bindings[n])?;
            let component = &p.loans()[n];
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[n])),
                "A2 actual component Name",
            )?;
            let loan = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[n]),
                "A2 actual synchronous reader loan",
            )?;
            let end = one(
                owned
                    .loan_ends()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[n]),
                "A2 actual synchronous reader endpoint",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &ReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && component.statement() == site.statement
                    && component.source() == site.source
                    && component.binding() == binding
                    && component.kind() == LoanKind::Shared
                    && Some(component.origin()) == p.source()
                    && read.arguments()[n].argument_index() == n
                    && read.arguments()[n].parameter_index() == n
                    && read.arguments()[n].mode() == ParameterMode::Borrow
                    && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && end.point() == LoanEndPoint::CallReturn(site.read),
                "A2 original ordered K/V Shared/reader Borrow lineage and endpoint",
                (component, loan, end),
            )?;
        }
        for (kind, point, mut actions) in [
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
            actions.extend(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(IterationCleanupAction::Drop),
            );
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "A2 actual complete provider exit",
            )?;
            need(
                p.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && owned.cleanup_conditions().get(exit.condition())
                        == Some(&CleanupCondition::Always)
                    && owned
                        .cleanup_steps()
                        .iter()
                        .filter_map(|(at, a)| (*at == point).then_some(*a))
                        .collect::<Vec<_>>()
                        == actions,
                "A2 full internal normal cleanup action order/condition",
                exit,
            )?;
        }
    } else {
        let observe = i.observe_call.unwrap();
        let call = typed
            .call(observe)
            .ok_or_else(|| failure("A2 actual control observe", observe))?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(observe).unwrap().payload()
        else {
            unreachable!()
        };
        let arg = arguments[0].value;
        let loan = one(
            owned
                .loans()
                .iter()
                .filter(|l| l.call() == observe && l.argument() == arg),
            "A2 actual control Map loan",
        )?;
        let end = one(
            owned
                .loan_ends()
                .iter()
                .filter(|l| l.call() == observe && l.argument() == arg),
            "A2 actual control Map loan end",
        )?;
        need(
            call.target() == CallableTarget::Source(callable(names, i.observe)?)
                && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==source)
                && end.point() == LoanEndPoint::CallReturn(observe)
                && source_drop.condition().is_none()
                && matches!(source_drop.owner().and_then(|id|owned.cleanup_conditions().owner_value(id)),Some(CleanupOwnerValue::Expression{expression,origin})if *expression==i.birth&&*origin==ast::span(parsed,i.birth)),
            "A2 control borrows actual internal Map and drops that exact birth",
            (call, loan, end, source_drop),
        )?;
    }
    Ok(Proof {
        callback_drop: callback_drop.value_origin(),
        source_drop: source_drop.value_origin(),
    })
}
pub(super) use unit::check as unit;
