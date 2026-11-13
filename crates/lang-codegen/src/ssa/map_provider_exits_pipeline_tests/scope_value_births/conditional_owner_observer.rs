//! Original Unit If-owned Map observations; every case retains its positive oracle.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Real source AST anchors for the two original If-owned Map shapes.
    use super::super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
        temporary_arguments::ast::function,
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement, VariableKind},
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Input {
        pub scan: Span,
        pub flag: Span,
        pub conditional: ExpressionId,
        pub condition: ExpressionId,
        pub arms: [StatementId; 2],
        pub births: [ExpressionId; 2],
        pub make: Span,
        pub make_name: Span,
        pub observe: Span,
        pub reader: Span,
        pub local: Option<Span>,
        pub site: Option<(StatementId, ExpressionId)>,
        pub observes: Vec<(ExpressionId, ExpressionId)>,
    }
    pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
        p.ast().expressions().get(e).unwrap().span()
    }
    pub(super) fn inside(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
        let scan = function(s, p, "scan", Some("Boolean"))?;
        let make = function(s, p, "make", None)?;
        let observe = function(s, p, "observe", Some("Map<String,String>"))?;
        let reader = one(
            p.roots().iter().filter_map(|id| {
                let node = p.ast().items().get(*id).ok()?;
                let Item::Function { name: NameMarker::Present(name), parameters, .. } = node.payload() else { return None; };
                (s.slice(*name) == Ok("read") && parameters.len() == 2
                    && parameters.iter().zip(["key", "value"]).all(|(parameter, expected)| {
                        parameter.mode_marker.is_none()
                            && matches!(parameter.name, NameMarker::Present(name) if s.slice(name) == Ok(expected))
                            && p.ast().type_refs().get(parameter.type_ref).is_ok_and(|ty| s.slice(ty.span()) == Ok("String"))
                    })).then_some(node.span())
            }),
            "If original reader has two ordered String parameters",
        )?;
        let node = one(
            p.roots()
                .iter()
                .map(|id| p.ast().items().get(*id).unwrap())
                .filter(|n| n.span() == scan.span),
            "If original scanner AST",
        )?;
        let Item::Function { parameters, .. } = node.payload() else {
            unreachable!()
        };
        let NameMarker::Present(flag) = parameters[0].name else {
            return Err(failure("If actual flag parameter", parameters));
        };
        let (conditional, condition, arms) = one(
            p.ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::If {
                        condition,
                        then_branch,
                        else_branch: Some(no),
                        ..
                    } if inside(scan.span, n.span()) => Some((id, *condition, [*then_branch, *no])),
                    _ => None,
                }),
            "If original scanner conditional",
        )?;
        need(
            s.slice(span(p, condition)) == Ok("flag"),
            "If original flag condition",
            condition,
        )?;
        let mut births = Vec::new();
        for arm in arms {
            let n = p.ast().statements().get(arm).unwrap();
            let Statement::Expression { expression } = n.payload() else {
                return Err(failure("If actual direct arm expression", n));
            };
            let Expression::Call {
                callee, arguments, ..
            } = p.ast().expressions().get(*expression).unwrap().payload()
            else {
                return Err(failure("If actual make Call", expression));
            };
            need(
                s.slice(span(p, *callee)) == Ok("make") && arguments.is_empty(),
                "If arm really calls original make",
                expression,
            )?;
            births.push(*expression);
        }
        let local = if c.temporary {
            None
        } else {
            Some(one(
                p.ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            kind: VariableKind::Val,
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if inside(scan.span, n.span()) && *initializer == conditional => {
                            Some(*name)
                        }
                        _ => None,
                    }),
                "If actual named Map initializer",
            )?)
        };
        let site = if c.provider {
            Some(one(
                p.ast()
                    .statements()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Statement::For {
                            source,
                            binding: ForBinding::Destructuring { .. },
                            ..
                        } if inside(scan.span, n.span()) => Some((id, *source)),
                        _ => None,
                    }),
                "If actual For site",
            )?)
        } else {
            None
        };
        if c.temporary {
            need(
                site.is_some_and(|(_, source)| source == conditional),
                "If original temporary is the full If source",
                site,
            )?;
        }
        let mut observes = p
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if inside(scan.span, n.span()) && s.slice(span(p, *callee)) == Ok("observe") => {
                    Some((id, arguments))
                }
                _ => None,
            })
            .map(|(id, args)| {
                need(
                    args.len() == 1,
                    "If original observe has one argument",
                    args,
                )?;
                Ok((id, args[0].value))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        observes.sort_by_key(|(e, _)| span(p, *e).start());
        need(
            observes.len()
                == if c.temporary {
                    0
                } else if c.label == "P03-control" {
                    2
                } else {
                    1
                },
            "If exact original observe count",
            &observes,
        )?;
        Ok(Input {
            scan: scan.span,
            flag,
            conditional,
            condition,
            arms,
            births: [births[0], births[1]],
            make: make.span,
            make_name: make.name,
            observe: observe.span,
            reader,
            local,
            site,
            observes,
        })
    }

    pub(super) fn site(
        p: &ParsedFile,
        i: &Input,
    ) -> Result<super::super::super::births::ast::Site, Failure> {
        let (statement, source) = i
            .site
            .ok_or_else(|| failure("If actual provider AST site", i))?;
        let node = p.ast().statements().get(statement).unwrap();
        let Statement::For {
            binding: ForBinding::Destructuring { names, .. },
            body,
            ..
        } = node.payload()
        else {
            unreachable!()
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("If actual provider two named bindings", names));
        };
        let (read, arguments) = one(
            p.ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { arguments, .. }
                        if inside(node.span(), n.span())
                            && id != i.births[0]
                            && id != i.births[1] =>
                    {
                        Some((id, arguments))
                    }
                    _ => None,
                }),
            "If actual provider sole body reader Call",
        )?;
        let [a, b] = arguments.as_slice() else {
            return Err(failure("If actual reader two arguments", arguments));
        };
        Ok(super::super::super::births::ast::Site {
            statement,
            span: node.span(),
            source,
            body: *body,
            bindings: [*key, *value],
            read,
            arguments: [a.value, b.value],
        })
    }
}
mod facts {
    //! Same-analysis public facts, including the complete cleanup facts before lowering.
    use crate::test_support::diagnostics::diagnostic;
    mod provider {
        //! Full source-qualified component delivery and published provider cleanup.
        use super::*;
        use lang_frontend::ownership_checking::{
            LoanKind, MapIterationComponent, OwnershipBindingKind,
            UnitIterationCleanupAction as Action, UnitIterationExitKind,
        };
        pub(super) fn check(
            p: &ParsedFile,
            i: &Input,
            n: &ValidatedCompilationUnitNames,
            t: &CompilationUnitTypes,
            o: &CompilationUnitOwnership,
            drop: UnitDropFact,
        ) -> Result<(), Failure> {
            let site = super::super::ast::site(p, i)?;
            let unit = one(
                n.names().source_units().iter(),
                "If provider original source unit",
            )?
            .source_unit();
            let e = |id| UnitExpressionId::new(unit, id);
            let statement = UnitStatementId::new(unit, site.statement);
            let plan = o
                .map_iteration(statement)
                .ok_or_else(|| failure("If complete acquired provider plan", statement))?;
            let d = plan.descriptor();
            let target = one(
                n.names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == unit && inside(i.reader, d.name_span()))
                    .map(|d| d.id()),
                "If actual reader declaration",
            )?;
            let call = t
                .call(e(site.read))
                .ok_or_else(|| failure("If actual typed reader Call", e(site.read)))?;
            need(
                call.target() == UnitCallTarget::Declaration(target)
                    && call.arguments().len() == 2
                    && call.receiver().is_none(),
                "If exact actual reader ABI",
                call,
            )?;
            for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let symbol = root(n, site.bindings[index])?;
                let reference = one(
                    n.names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == span(p, site.arguments[index])),
                    "If original component reader reference",
                )?;
                let published = &plan.loans()[index];
                let loan = one(
                    o.loans().iter().filter(|l| {
                        l.call() == e(site.read) && l.argument() == e(site.arguments[index])
                    }),
                    "If exact component synchronous loan",
                )?;
                need(
                    d.bindings()[index] == symbol
                        && o.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                        && reference.target() == &UnitReferenceTarget::Symbol(symbol)
                        && published.statement() == statement
                        && published.source() == e(site.source)
                        && published.component() == component
                        && published.binding() == symbol
                        && published.kind() == LoanKind::Shared
                        && Some(published.origin()) == plan.source()
                        && call.arguments()[index].argument_index() == index
                        && call.arguments()[index].parameter_index() == index
                        && call.arguments()[index].mode() == ParameterMode::Borrow
                        && call.arguments()[index].parameter_type()
                            == [d.key_type(), d.value_type()][index]
                        && loan.kind() == LoanKind::Shared
                        && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==symbol)
                        && loan.begin_span() == span(p, site.arguments[index])
                        && loan.end_span() == span(p, site.read),
                    "If exact K/V canonical source/ordered reader delivery/loan bounds",
                    (published, loan),
                )?;
            }
            let mut exhausted = vec![
                Action::FinishProvider(statement),
                Action::EndSource(statement),
            ];
            if matches!(drop.target(), UnitDropTarget::Temporary(_)) {
                exhausted.push(Action::Drop(drop));
            }
            for (kind, point, actions) in [
                (
                    UnitIterationExitKind::Fallthrough,
                    UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                    vec![
                        Action::EndBinding {
                            statement,
                            symbol: d.value_binding(),
                        },
                        Action::EndBinding {
                            statement,
                            symbol: d.key_binding(),
                        },
                    ],
                ),
                (
                    UnitIterationExitKind::Exhaustion,
                    UnitDropPoint::LoopExit(statement),
                    exhausted,
                ),
            ] {
                let exit = one(
                    plan.exits().iter().filter(|v| v.kind() == kind),
                    "If exact normal provider exit",
                )?;
                need(
                    plan.exits().len() == 2
                        && exit.point() == point
                        && exit.actions() == actions
                        && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                    "If complete reverse components/providerEnd/ownerDrop cleanup",
                    exit,
                )?;
            }
            Ok(())
        }
    }
    use super::super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{
        Case,
        ast::{Input, inside, span},
    };
    use crate::ssa::unit_plan;
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, UnitCallArgumentOwnershipKind, UnitDropFact,
            UnitDropPoint, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, ParsedFile},
        source::Span,
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitCallableTarget,
            UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };
    #[derive(Debug)]
    pub(super) struct Proof {
        pub drop: UnitDropFact,
    }
    fn root(n: &ValidatedCompilationUnitNames, at: Span) -> Result<UnitSymbolId, Failure> {
        let (u, symbol) = one(
            n.names().source_units().iter().flat_map(|u| {
                u.resolution()
                    .symbols()
                    .iter()
                    .filter(move |s| s.span() == at && !s.is_synthetic())
                    .map(move |s| (u.source_unit(), s.id()))
            }),
            "If original source-qualified symbol",
        )?;
        one(
            n.names()
                .references()
                .iter()
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s)
                        if s.source_unit() == u && s.symbol() == symbol =>
                    {
                        Some(*s)
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter(),
            "If original referenced qualified symbol",
        )
    }
    pub(super) fn check(
        c: &Case,
        p: &ParsedFile,
        i: &Input,
        n: &ValidatedCompilationUnitNames,
        t: &CompilationUnitTypes,
        o: &CompilationUnitOwnership,
    ) -> Result<Proof, Failure> {
        let unit = one(n.names().source_units().iter(), "If original source unit")?.source_unit();
        let e = |id| UnitExpressionId::new(unit, id);
        let decl = |at: Span| {
            one(
                n.names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == unit && inside(at, d.name_span()))
                    .map(|d| d.id()),
                "If exact original callable declaration",
            )
        };
        let make = one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == i.make_name)
                .map(|d| d.id()),
            "If actual source-qualified make declaration",
        )?;
        let local = i.local.map(|at| root(n, at)).transpose()?;
        let drops = o
            .drops()
            .iter()
            .copied()
            .filter(|d| inside(i.scan, d.value_origin()))
            .collect::<Vec<_>>();
        diagnostic!(
            "SCOPE B IF FE label={} unit=true conditional={:?} condition={:?} arms={:?} if_type={:?} if_category={:?} condition_type={:?} local={local:?} local_type={:?} formed={:?} scan_drops={drops:#?} all_drops={:#?}",
            c.label,
            e(i.conditional),
            e(i.condition),
            i.arms,
            t.expression_type(e(i.conditional)),
            t.expression_category(e(i.conditional)),
            t.expression_type(e(i.condition)),
            local.and_then(|s| t.symbol_type(s)),
            o.formed_owned_locals(),
            o.drops()
        );
        for drop in &drops {
            diagnostic!(
                "SCOPE B IF DROP label={} unit=true target={:?} point={:?} origin={:?}",
                c.label,
                drop.target(),
                drop.point(),
                drop.value_origin()
            );
        }
        for birth in i.births {
            diagnostic!(
                "SCOPE B IF BIRTH label={} unit=true expression={:?} span={:?} type={:?} category={:?} call={:#?}",
                c.label,
                e(birth),
                span(p, birth),
                t.expression_type(e(birth)),
                t.expression_category(e(birth)),
                t.call(e(birth))
            );
        }
        if let Some((statement, source)) = i.site {
            let statement = UnitStatementId::new(unit, statement);
            diagnostic!(
                "SCOPE B IF PLAN label={} unit=true statement={statement:?} surface={:?} surface_type={:?} surface_category={:?} descriptor={:#?} ownership={:#?}",
                c.label,
                e(source),
                t.expression_type(e(source)),
                t.expression_category(e(source)),
                t.map_iteration(statement),
                o.map_iteration(statement)
            );
            if let Some(plan) = o.map_iteration(statement) {
                diagnostic!(
                    "SCOPE B IF CANONICAL label={} unit=true source_kind={:?} source={:?} access={:?} bindings={:?} loans={:#?} exits={:#?}",
                    c.label,
                    plan.source_kind(),
                    plan.source(),
                    plan.source_access(),
                    plan.bindings(),
                    plan.loans(),
                    plan.exits()
                );
                for exit in plan.exits() {
                    diagnostic!(
                        "SCOPE B IF EXIT label={} unit=true exit={exit:#?} published_actions={:#?}",
                        c.label,
                        o.iteration_cleanup_at(exit.point())
                    );
                }
            }
        }
        for &(call, arg) in &i.observes {
            diagnostic!(
                "SCOPE B IF OBSERVE label={} unit=true expression={:?} argument={:?} typed={:#?} contracts={:#?} loans={:#?}",
                c.label,
                e(call),
                e(arg),
                t.call(e(call)),
                o.call_argument_contracts_for(e(call)).collect::<Vec<_>>(),
                o.loans()
                    .iter()
                    .filter(|l| l.call() == e(call))
                    .collect::<Vec<_>>()
            );
        }
        let ty = t
            .expression_type(e(i.conditional))
            .ok_or_else(|| failure("If actual result type", e(i.conditional)))?;
        need(
            matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|v|t.types().get(*v)==Some(&UnitTypeKind::Builtin(BuiltinType::String))))
                && t.expression_category(e(i.conditional)).is_some()
                && t.expression_type(e(i.condition)).is_some_and(|ty| {
                    t.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                }),
            "If real concrete Map result and Boolean condition",
            ty,
        )?;
        let flag = root(n, i.flag)?;
        let reference = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, i.condition)),
            "If original actual flag reference",
        )?;
        need(
            reference.target() == &UnitReferenceTarget::Symbol(flag),
            "If condition refers to actual scanner flag",
            reference,
        )?;
        for birth in i.births {
            let call = t
                .call(e(birth))
                .ok_or_else(|| failure("If actual arm Call descriptor", e(birth)))?;
            let target = UnitCallableTarget::Declaration(make);
            let resolved = unit_plan::resolve_unit_call_instance(
                t,
                o,
                target,
                call.instance().type_arguments().to_vec(),
                call.receiver().map(|r| r.ty()),
                span(p, birth),
            )
            .map_err(|err| failure("If actual make Call instance", err))?;
            diagnostic!(
                "SCOPE B IF INSTANCE label={} unit=true expression={:?} instance={:?} resolved={:?} delegation={:?}",
                c.label,
                e(birth),
                call.instance(),
                resolved.key(),
                resolved.delegation()
            );
            need(
                call.expression() == e(birth)
                    && call.target() == UnitCallTarget::Declaration(make)
                    && call.arguments().is_empty()
                    && call.receiver().is_none()
                    && call.result_source() == CallableResultSource::Owned
                    && call.return_type() == ty
                    && t.expression_type(e(birth)) == Some(ty)
                    && t.expression_category(e(birth)) == Some(ExpressionCategory::Temporary)
                    && resolved.key().target() == target
                    && resolved.delegation().is_empty(),
                "If both original calls select actual make Owned result/instance",
                call,
            )?;
        }
        if let Some(root) = local {
            need(
                t.symbol_type(root) == Some(ty) && o.formed_owned_locals().contains(&root),
                "If original local forms from concrete If result",
                (root, o.formed_owned_locals()),
            )?;
            need(
                drops
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(root)),
                "If actual local has published named cleanup",
                &drops,
            )?;
        }
        need(
            t.map_iterations().len() == usize::from(c.provider)
                && o.map_iterations().len() == usize::from(c.provider),
            "If actual acquired For/control plan set",
            o.map_iterations(),
        )?;
        if let Some((statement, source)) = i.site {
            let statement = UnitStatementId::new(unit, statement);
            let plan = o
                .map_iteration(statement)
                .ok_or_else(|| failure("If actual acquired ownership plan", statement))?;
            need(
                t.map_iteration(statement) == Some(plan.descriptor())
                    && plan.descriptor().source() == e(source)
                    && plan.descriptor().source_type() == ty
                    && plan.loans().len() == 2
                    && !plan.exits().is_empty(),
                "If actual surface descriptor and complete nonempty published plan",
                plan,
            )?;
            if c.temporary {
                need(
                    plan.source_kind()
                        == &IterationSource::BorrowedSource(UnitLoanTarget::Temporary(e(
                            i.conditional
                        )))
                        && plan.source_access() == UnitIterationSourceAccess::Temporary
                        && drops
                            .iter()
                            .any(|d| d.target() == UnitDropTarget::Temporary(e(i.conditional))),
                    "If full conditional is the original temporary owner/source/drop",
                    plan,
                )?;
            } else {
                let local = local.ok_or_else(|| failure("If actual named For source", i))?;
                let reference = one(
                    n.names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == span(p, source)),
                    "If actual source Name reference",
                )?;
                need(
                    matches!(
                        p.ast().expressions().get(source).unwrap().payload(),
                        Expression::Name
                    ) && reference.target() == &UnitReferenceTarget::Symbol(local)
                        && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==local)
                        && plan.source_access() == UnitIterationSourceAccess::Owned,
                    "If actual named surface source and canonical local owner",
                    plan,
                )?;
            }
        }
        for &(call, arg) in &i.observes {
            let call_descriptor = t
                .call(e(call))
                .ok_or_else(|| failure("If real observe descriptor", e(call)))?;
            let contract = one(
                o.call_argument_contracts_for(e(call))
                    .filter(|v| v.argument() == e(arg)),
                "If actual observe contract",
            )?;
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|v| v.call() == e(call) && v.argument() == e(arg)),
                "If actual observe loan",
            )?;
            let local = local.ok_or_else(|| failure("If observe actual local owner", i))?;
            need(
                call_descriptor.target() == UnitCallTarget::Declaration(decl(i.observe)?)
                    && matches!(call_descriptor.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                    && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                    && contract.parameter_index() == 0
                    && contract.parameter_type() == ty
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==local)
                    && loan.begin_span() == span(p, arg)
                    && loan.end_span() == span(p, call),
                "If observe borrows the same original local through actual call completion",
                (call_descriptor, contract, loan),
            )?;
        }
        let drop = *one(drops.iter(), "If exact sole scanner owner cleanup fact")?;
        let (target, point, origin) = if c.temporary {
            let (statement, _) = i
                .site
                .ok_or_else(|| failure("If temporary actual For", i))?;
            (
                UnitDropTarget::Temporary(e(i.conditional)),
                UnitDropPoint::LoopExit(UnitStatementId::new(unit, statement)),
                span(p, i.conditional),
            )
        } else {
            let local = local.ok_or_else(|| failure("If actual named owner", i))?;
            let (call, _) = i
                .observes
                .last()
                .ok_or_else(|| failure("If actual final observe", i))?;
            (
                UnitDropTarget::Named(local),
                UnitDropPoint::CallReturn(e(*call)),
                i.local.unwrap(),
            )
        };
        need(
            drop.target() == target && drop.point() == point && drop.value_origin() == origin,
            "If exact original owner Drop target/final public point/value origin",
            drop,
        )?;
        if c.provider {
            provider::check(p, i, n, t, o, drop)?;
        }
        Ok(Proof { drop })
    }
}
mod graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    provider: bool,
    temporary: bool,
}
pub(crate) fn cases() -> [Case; 4] {
    macro_rules! case {
        ($label:literal, $provider:literal, $temporary:literal) => {
            Case {
                label: $label,
                source: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".ko"
                )),
                golden: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".stdout"
                )),
                provider: $provider,
                temporary: $temporary,
            }
        };
    }
    [
        case!("P03-for", true, false),
        case!("P03-control", false, false),
        case!("P04-for", true, true),
        case!("P04-control", false, false),
    ]
}
fn observe(c: &Case) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE B IF INPUT label={} unit=true source={:?} golden={:?}",
        c.label,
        c.source,
        c.golden
    );
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let p = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", p.diagnostics())?;
    let i = ast::input(c, &sources, &p)?;
    diagnostic!("SCOPE B IF AST label={} unit=true input={i:#?}", c.label);
    let mut report = Report::default();
    let mut observed = false;
    let mut proof = None;
    let lowered =
        pipeline::unit_observed(&sources, &p, "provider.ko", &mut report, |n, t, o, r| {
            observed = true;
            let checked = facts::check(c, &p, &i, n, t, o);
            r.query(
                "If original qualified type/call/owner/plan/drop observations",
                &checked,
            );
            proof = checked.ok();
        });
    diagnostic!(
        "SCOPE B IF LOWER label={} unit=true observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "If observer received the original Unit analysis",
        c.label,
    )?;
    let (program, _) = lowered?;
    let proof = proof
        .as_ref()
        .ok_or_else(|| failure("If same-analysis original observation proof", &report))?;
    let query = graph::check(c, &p, &i, proof, &program);
    report.query(
        "If actual branches/current owner/full provider and ordinary cleanup",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "SCOPE B IF FINAL label={} unit=true report={report:#?}",
        c.label
    );
    need(
        report.passed(),
        "If original positive same-analysis observation",
        report,
    )
}
#[test]
fn original_unit_if_owner_sources_report_real_facts() {
    let mut results = Vec::new();
    for c in cases() {
        let result = observe(&c);
        diagnostic!(
            "SCOPE B IF CASE label={} unit=true result={result:?}",
            c.label
        );
        results.push((c.label, result));
    }
    assert_eq!(results.len(), 4);
    assert_diagnostic!(results.iter().all(|(_, r)| r.is_ok()), "{results:#?}");
}
