//! Source-qualified counterpart; no independent Unit owner table is invented.
use super::*;
fn root(names: &ValidatedCompilationUnitNames, marker: Span) -> Result<UnitSymbolId, Failure> {
    let (unit, symbol) = one(
        names.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "A2 actual qualified original local",
    )?;
    one(
        names
            .names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(id)
                    if id.source_unit() == unit && id.symbol() == symbol =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter(),
        "A2 actual qualified referenced local",
    )
}
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_lambda_body) fn check(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let (c, _, parsed, i) = input;
    let unit = one(
        names.names().source_units().iter(),
        "A2 actual original source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let declaration = |scope| {
        one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && ast::inside(scope, d.name_span()))
                .map(|d| d.id()),
            "A2 actual qualified original callable declaration",
        )
    };
    let callback = root(names, i.callback)?;
    let source = root(names, i.source)?;
    let descriptor = owned
        .closure(e(i.lambda))
        .ok_or_else(|| failure("A2 actual qualified closure", i.lambda))?;
    let origin = owned
        .callable_origin(e(i.callee))
        .ok_or_else(|| failure("A2 actual qualified callback provenance", i.callee))?;
    let invoke = typed
        .call(e(i.invoke))
        .ok_or_else(|| failure("A2 actual qualified FunctionValue Call", i.invoke))?;
    let function = typed
        .expression_type(e(i.lambda))
        .ok_or_else(|| failure("A2 qualified actual lambda Fn0 type", i.lambda))?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, i.callee)),
        "A2 qualified actual callback reference",
    )?;
    need(
        descriptor.expression() == e(i.lambda)
            && !descriptor.move_owned()
            && owned.captures_of(e(i.lambda)).next().is_none()
            && origin.expression() == e(i.callee)
            && origin.origin() == UnitCallableOrigin::Lambda(e(i.lambda))
            && origin.span() == ast::span(parsed, i.callee)
            && reference.target() == &UnitReferenceTarget::Symbol(callback)
            && typed.expression_type(e(i.callee)) == Some(function)
            && typed.symbol_type(callback) == Some(function)
            && owned.formed_owned_locals().contains(&callback)
            && matches!(typed.types().get(function),Some(UnitTypeKind::Function{move_only:false,parameters,return_type})if parameters.is_empty()&&*return_type==invoke.return_type()&&typed.types().get(*return_type)==Some(&UnitTypeKind::Builtin(BuiltinType::Unit)))
            && invoke.target() == UnitCallTarget::FunctionValue
            && invoke.receiver().is_none()
            && invoke.arguments().is_empty()
            && invoke.instance().type_arguments().is_empty()
            && invoke.borrow_return().is_none(),
        "A2 qualified actual no-capture Fn0 Unit callback ABI/provenance",
        (invoke, descriptor, origin),
    )?;
    let callback_drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(callback)),
        "A2 qualified unique original callback cleanup",
    )?;
    need(
        callback_drop.point() == UnitDropPoint::CallReturn(e(i.invoke))
            && callback_drop.value_origin() == i.callback
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(e(i.lambda))),
        "A2 qualified actual callable last-use cleanup",
        callback_drop,
    )?;
    let scan = typed
        .call(e(i.scan_call))
        .ok_or_else(|| failure("A2 qualified actual scanner call", i.scan_call))?;
    need(
        scan.target() == UnitCallTarget::Declaration(declaration(i.scan.span)?)
            && scan.arguments().is_empty(),
        "A2 qualified actual entry selects scanner",
        scan,
    )?;
    let birth = typed
        .call(e(i.birth))
        .ok_or_else(|| failure("A2 qualified internal actual make", i.birth))?;
    let ty = birth.return_type();
    need(
        birth.target() == UnitCallTarget::Declaration(declaration(i.make.span)?)
            && birth.arguments().is_empty()
            && birth.result_source() == CallableResultSource::Owned
            && birth.borrow_return().is_none()
            && typed.expression_type(e(i.birth)) == Some(ty)
            && typed.symbol_type(source) == Some(ty)
            && owned.formed_owned_locals().contains(&source)
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|t|typed.types().get(*t)==Some(&UnitTypeKind::Builtin(BuiltinType::String))))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(e(i.birth))),
        "A2 qualified actual named Map owner born in true lambda body",
        birth,
    )?;
    let source_drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(source)),
        "A2 qualified sole internal Map cleanup",
    )?;
    need(
        source_drop.value_origin() == i.source
            && source_drop.point()
                == if let Some(site) = &i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(unit, site.statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.observe_call.unwrap()))
                },
        "A2 qualified exact current internal Map endpoint",
        source_drop,
    )?;
    need(
        typed.map_iterations().len() == usize::from(c.provider)
            && owned.map_iterations().len() == usize::from(c.provider),
        "A2 qualified exact acquired provider/control set",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("A2 qualified actual internal Map plan", site))?;
        let d = p.descriptor();
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.source)),
            "A2 qualified actual internal source reference",
        )?;
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && d.source_type() == ty
                && reference.target() == &UnitReferenceTarget::Symbol(source)
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==source),
            "A2 qualified actual internal For uses original named owner",
            p,
        )?;
        let read = typed
            .call(e(site.read))
            .ok_or_else(|| failure("A2 qualified actual reader Call", site.read))?;
        need(
            read.target() == UnitCallTarget::Declaration(declaration(i.reader)?)
                && read.arguments().len() == 2,
            "A2 qualified selected original read",
            read,
        )?;
        for n in 0..2 {
            let binding = root(names, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[n])),
                "A2 qualified actual component reference",
            )?;
            let loan = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == e(site.read) && l.argument() == e(site.arguments[n])),
                "A2 qualified actual reader component loan",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &UnitReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == statement
                    && l.source() == e(site.source)
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && l.kind() == LoanKind::Shared
                    && read.arguments()[n].argument_index() == n
                    && read.arguments()[n].parameter_index() == n
                    && read.arguments()[n].mode() == ParameterMode::Borrow
                    && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && loan.end_span() == ast::span(parsed, site.read),
                "A2 qualified original K/V Shared/Borrow lineage and actual call bounds",
                (l, loan),
            )?;
        }
        for (kind, point, mut actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
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
            actions.extend(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(UnitIterationCleanupAction::Drop),
            );
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "A2 qualified actual normal exit",
            )?;
            need(
                p.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "A2 qualified complete ordered provider normal cleanup",
                exit,
            )?;
        }
    } else {
        let observe = i.observe_call.unwrap();
        let call = typed
            .call(e(observe))
            .ok_or_else(|| failure("A2 qualified actual control observe", observe))?;
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
                .filter(|l| l.call() == e(observe) && l.argument() == e(arg)),
            "A2 qualified actual control borrow",
        )?;
        let contract = one(
            owned
                .call_argument_contracts_for(e(observe))
                .filter(|a| a.argument() == e(arg)),
            "A2 qualified actual control Borrow contract",
        )?;
        need(
            call.target() == UnitCallTarget::Declaration(declaration(i.observe)?)
                && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==source)
                && loan.end_span() == ast::span(parsed, observe)
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.parameter_index() == 0
                && contract.parameter_type() == ty,
            "A2 qualified control borrows exact original current Map",
            (call, loan, contract),
        )?;
    }
    Ok(Proof {
        callback_drop: callback_drop.value_origin(),
        source_drop: source_drop.value_origin(),
    })
}
