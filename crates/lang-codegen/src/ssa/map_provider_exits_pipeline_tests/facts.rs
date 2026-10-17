//! 只读实际 For/source/K/V/双 Shared 绑定与来源模式；不构造 facts，不决定 lower 结果。
use super::{
    Report,
    fixture::{Exit, Fixture, LoopSite, loop_site},
};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropTarget, IterationSource, LoanKind, LoanTarget,
        MapIterationComponent, MapIterationSourceAccess, OwnershipBindingKind,
        OwnershipCheckedFile, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, IntrinsicTypeConstructor,
        MapIterationKind, ParameterMode, TypeKind, TypedFile, UnitCallTarget, UnitExpressionId,
        UnitStatementId, UnitTypeKind,
    },
};

fn expected(
    fixture: &Fixture,
) -> (
    IntrinsicTypeConstructor,
    MapIterationKind,
    OwnershipBindingKind,
) {
    if fixture.map {
        (
            IntrinsicTypeConstructor::Map,
            MapIterationKind::Map,
            OwnershipBindingKind::Owned,
        )
    } else {
        (
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
            OwnershipBindingKind::Shared,
        )
    }
}

fn site(fixture: &Fixture, report: &mut Report) -> Option<LoopSite> {
    let site = loop_site(fixture);
    report.query("exactly one actual For with direct Name source", &site);
    let site = site.ok()?;
    report.check(
        "actual For/source/binding source text and path",
        fixture.sources.slice(site.source_span) == Ok("source")
            && fixture.sources.slice(site.bindings[0]) == Ok("key")
            && fixture.sources.slice(site.bindings[1]) == Ok("value")
            && fixture.sources.source_name(site.span.source_id()) == Ok(fixture.path)
            && site.span.source_id() == fixture.parsed.source_id(),
        (site.body, &site),
    );
    Some(site)
}

pub(super) fn single(
    fixture: &Fixture,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    println!(
        "SPEC0331 EXIT RAW SINGLE maps={:#?} drops={:#?} cleanup_steps={:#?} conditions={:#?}",
        owned.map_iterations(),
        owned.drops(),
        owned.cleanup_steps(),
        owned.cleanup_conditions()
    );
    println!("SPEC0331 EXIT RAW SINGLE CALL LOANS {:#?}", owned.loans());
    let Some(site) = site(fixture, report) else {
        return;
    };
    report.check(
        "single exact typed/owned Map counts",
        typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
        (typed.map_iterations(), owned.map_iterations()),
    );
    let descriptor = typed.map_iteration(site.statement);
    let plan = owned.map_iteration(site.statement);
    let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
        report.check("single real Map facts exist", false, (descriptor, plan));
        return;
    };
    report.check(
        "single actual For/source and same typed descriptor",
        descriptor.statement() == site.statement
            && descriptor.source() == site.source
            && Some(descriptor.source_type()) == typed.expression_type(site.source)
            && plan.descriptor() == descriptor
            && typed.sequential_iteration(site.statement).is_none()
            && owned.iteration(site.statement).is_none(),
        (descriptor, plan.descriptor()),
    );
    let (constructor, kind, binding_kind) = expected(fixture);
    let source_type = typed.types().get(descriptor.source_type());
    report.check(
        "single real intrinsic Map K/V both String",
        matches!(source_type,
            Some(TypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && descriptor.kind() == kind
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                && typed.types().get(descriptor.key_type()) == Some(&TypeKind::Builtin(BuiltinType::String))
                && typed.types().get(descriptor.value_type()) == Some(&TypeKind::Builtin(BuiltinType::String))),
        source_type,
    );
    let references = names
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.span() == site.source_span
                && let ReferenceTarget::Symbol(symbol) = reference.target()
            {
                Some(*symbol)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let source = if let [source] = references.as_slice() {
        Some(*source)
    } else {
        None
    };
    let source_symbol =
        source.and_then(|id| names.symbols().iter().find(|symbol| symbol.id() == id));
    let access = if fixture.map {
        MapIterationSourceAccess::Owned
    } else {
        MapIterationSourceAccess::Shared
    };
    report.check(
        "single real named parameter source and original capability",
        matches!(plan.source(), Some(LoanTarget::Place(place)) if Some(place.root()) == source && place.is_root())
            && matches!(plan.source_kind(), IterationSource::BorrowedSource(_))
            && plan.source_access() == access
            && source.is_some_and(|source| owned.binding_kind(source) == Some(binding_kind))
            && source_symbol.is_some_and(|symbol| {
                symbol.kind() == SymbolKind::ValueParameter && !symbol.is_synthetic()
                    && symbol.name() == "source" && symbol.span().source_id() == fixture.parsed.source_id()
            }),
        (references, source_symbol, plan.source_kind(), plan.source_access()),
    );
    let mode = if fixture.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    for name in ["scan", "read"]
        .into_iter()
        .chain((fixture.exit == Exit::Return).then_some("operand"))
    {
        let callables = typed
            .callables()
            .iter()
            .filter(|callable| {
                names.symbols().iter().any(|symbol| {
                    symbol.id() == callable.symbol()
                        && symbol.name() == name
                        && symbol.kind() == SymbolKind::Function
                        && !symbol.is_synthetic()
                        && symbol.span().source_id() == fixture.parsed.source_id()
                })
            })
            .collect::<Vec<_>>();
        let passed = matches!(callables.as_slice(), [callable] if {
            if name == "scan" {
                source.is_some() && callable.parameter_symbols() == [source]
                    && matches!(callable.parameters(), [parameter]
                        if parameter.mode == mode && parameter.ty == descriptor.source_type())
            } else {
                callable.parameters().len() == 2 && callable.parameter_symbols().len() == 2
                    && callable.parameters().iter().all(|parameter| {
                        parameter.mode == ParameterMode::Borrow
                            && typed.types().get(parameter.ty) == Some(&TypeKind::Builtin(BuiltinType::String))
                    })
                    && callable.parameter_symbols().iter().all(|symbol| {
                        symbol.is_some_and(|symbol| owned.binding_kind(symbol) == Some(OwnershipBindingKind::Shared))
                    })
            }
        });
        report.check(
            "single actual scan/read callable parameter modes",
            passed,
            (name, callables),
        );
    }
    report.check(
        "single distinct K/V symbols and loans",
        descriptor.key_binding() != descriptor.value_binding()
            && plan.loans()[0] != plan.loans()[1],
        plan.loans(),
    );
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbols = names
            .symbols()
            .iter()
            .filter(|symbol| {
                symbol.span() == site.bindings[index]
                    && !symbol.is_synthetic()
                    && symbol.kind() == SymbolKind::ForBinding
            })
            .collect::<Vec<_>>();
        let binding = descriptor.bindings()[index];
        let component_type = [descriptor.key_type(), descriptor.value_type()][index];
        let loan = plan.loan(component);
        report.check(
            "single actual component symbol/type/Shared loan lineage",
            matches!(symbols.as_slice(), [symbol] if symbol.id() == binding)
                && typed.symbol_type(binding) == Some(component_type)
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && plan.bindings()[index].symbol() == binding
                && plan.bindings()[index].kind() == OwnershipBindingKind::Shared
                && loan == &plan.loans()[index]
                && loan.component() == component
                && loan.statement() == site.statement
                && loan.source() == site.source
                && loan.binding() == binding
                && loan.kind() == LoanKind::Shared
                && Some(loan.origin()) == plan.source(),
            (symbols, loan),
        );
    }
    single_exit(fixture, &site, names, typed, owned, report);
    // 仅观察这次 String 源的新 owner 拓扑，不套用旧 Int fixture 的 phi/Drop oracle。
    let phis = plan
        .closure_phis()
        .iter()
        .filter(|phi| Some(phi.symbol()) == source)
        .collect::<Vec<_>>();
    let incomings = plan
        .closure_phi_incomings()
        .iter()
        .filter(|incoming| {
            incoming
                .bindings()
                .iter()
                .any(|binding| phis.iter().any(|phi| phi.owner() == binding.target()))
        })
        .collect::<Vec<_>>();
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| {
            matches!(drop.target(), DropTarget::Named(symbol) if Some(symbol) == source)
                || drop
                    .owner()
                    .is_some_and(|owner| phis.iter().any(|phi| phi.owner() == owner))
        })
        .map(|drop| {
            (
                drop,
                super::fixture::location(&fixture.sources, drop.value_origin()),
            )
        })
        .collect::<Vec<_>>();
    println!(
        "SPEC0331 PROVIDER EXIT REAL SINGLE OWNER source={source:?} plan={plan:#?} source_phis={phis:#?} source_incomings={incomings:#?} source_drops={drops:#?}"
    );
}

pub(super) fn unit(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    println!(
        "SPEC0331 EXIT RAW UNIT maps={:#?} drops={:#?}",
        owned.map_iterations(),
        owned.drops()
    );
    println!(
        "SPEC0331 EXIT RAW UNIT CALL CONTRACTS {:#?} loans={:#?} deliveries={:#?}",
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(site) = site(fixture, report) else {
        return;
    };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit exactly one real source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let source_unit = source_names.source_unit();
    report.check(
        "unit source names belong to original parsed file",
        source_names.resolution().source_id() == fixture.parsed.source_id(),
        (source_unit, fixture.parsed.source_id()),
    );
    report.check(
        "unit exact typed/owned Map counts",
        typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
        (typed.map_iterations(), owned.map_iterations()),
    );
    let statement = UnitStatementId::new(source_unit, site.statement);
    let source_expression = UnitExpressionId::new(source_unit, site.source);
    let descriptor = typed.map_iteration(statement);
    let plan = owned.map_iteration(statement);
    let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
        report.check("unit real Map facts exist", false, (descriptor, plan));
        return;
    };
    report.check(
        "unit actual source-qualified For/source and same typed descriptor",
        descriptor.statement() == statement
            && descriptor.source() == source_expression
            && Some(descriptor.source_type()) == typed.expression_type(source_expression)
            && plan.descriptor() == descriptor
            && typed.sequential_iteration(statement).is_none()
            && owned.iteration(statement).is_none(),
        (descriptor, plan.descriptor()),
    );
    let (constructor, kind, binding_kind) = expected(fixture);
    let source_type = typed.types().get(descriptor.source_type());
    report.check(
        "unit real intrinsic Map K/V both String",
        matches!(source_type,
            Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && descriptor.kind() == kind
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                && typed.types().get(descriptor.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && typed.types().get(descriptor.value_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
        source_type,
    );
    let references = names
        .names()
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.span() == site.source_span
                && let UnitReferenceTarget::Symbol(symbol) = reference.target()
            {
                Some(*symbol)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let source = if let [source] = references.as_slice() {
        Some(*source)
    } else {
        None
    };
    let source_symbol = source.and_then(|id| {
        source_names
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == id.symbol())
    });
    let access = if fixture.map {
        UnitIterationSourceAccess::Owned
    } else {
        UnitIterationSourceAccess::Shared
    };
    report.check(
        "unit real named parameter source and original capability",
        matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if Some(place.root()) == source && place.is_root())
            && matches!(plan.source_kind(), IterationSource::BorrowedSource(_))
            && plan.source_access() == access
            && source.is_some_and(|source| source.source_unit() == source_unit && owned.binding_kind(source) == Some(binding_kind))
            && source_symbol.is_some_and(|symbol| {
                symbol.kind() == SymbolKind::ValueParameter && !symbol.is_synthetic()
                    && symbol.name() == "source" && symbol.span().source_id() == fixture.parsed.source_id()
            }),
        (references, source_symbol, plan.source_kind(), plan.source_access()),
    );
    let mode = if fixture.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    for name in ["scan", "read"]
        .into_iter()
        .chain((fixture.exit == Exit::Return).then_some("operand"))
    {
        let callables = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| {
                let symbol = signature.symbol();
                let matches = symbol.source_unit() == source_unit
                    && source_names.resolution().symbols().iter().any(|local| {
                        local.id() == symbol.symbol()
                            && local.name() == name
                            && local.kind() == SymbolKind::Function
                            && !local.is_synthetic()
                            && local.span().source_id() == fixture.parsed.source_id()
                    });
                if matches { signature.callable() } else { None }
            })
            .collect::<Vec<_>>();
        let passed = matches!(callables.as_slice(), [callable] if {
            if name == "scan" {
                source.is_some() && matches!(callable.parameters(), [parameter]
                    if parameter.symbol() == source && parameter.mode() == mode
                        && parameter.ty() == descriptor.source_type())
            } else {
                callable.parameters().len() == 2 && callable.parameters().iter().all(|parameter| {
                    parameter.mode() == ParameterMode::Borrow
                        && typed.types().get(parameter.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                        && parameter.symbol().is_some_and(|symbol| {
                            symbol.source_unit() == source_unit && owned.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                        })
                })
            }
        });
        report.check(
            "unit actual scan/read callable parameter modes",
            passed,
            (name, callables),
        );
    }
    report.check(
        "unit distinct K/V symbols and loans",
        descriptor.key_binding() != descriptor.value_binding()
            && plan.loans()[0] != plan.loans()[1],
        plan.loans(),
    );
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbols = source_names
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| {
                symbol.span() == site.bindings[index]
                    && !symbol.is_synthetic()
                    && symbol.kind() == SymbolKind::ForBinding
            })
            .collect::<Vec<_>>();
        let binding = descriptor.bindings()[index];
        let component_type = [descriptor.key_type(), descriptor.value_type()][index];
        let loan = plan.loan(component);
        report.check(
            "unit actual component symbol/type/Shared loan lineage",
            binding.source_unit() == source_unit
                && matches!(symbols.as_slice(), [symbol] if symbol.id() == binding.symbol())
                && typed.symbol_type(binding) == Some(component_type)
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && plan.bindings()[index].symbol() == binding
                && plan.bindings()[index].kind() == OwnershipBindingKind::Shared
                && loan == &plan.loans()[index]
                && loan.component() == component
                && loan.statement() == statement
                && loan.source() == source_expression
                && loan.binding() == binding
                && loan.kind() == LoanKind::Shared
                && Some(loan.origin()) == plan.source(),
            (symbols, loan),
        );
    }
    unit_exit(fixture, &site, names, typed, owned, report);
    // unit 只读取其真实公开 plan/drop；不伪造不存在的 closure phi 产品。
    let drops = owned
        .drops()
        .iter()
        .filter(
            |drop| matches!(drop.target(), UnitDropTarget::Named(symbol) if Some(symbol) == source),
        )
        .map(|drop| {
            (
                drop,
                super::fixture::location(&fixture.sources, drop.value_origin()),
            )
        })
        .collect::<Vec<_>>();
    println!(
        "SPEC0331 PROVIDER EXIT REAL UNIT OWNER source={source:?} plan={plan:#?} source_drops={drops:#?}"
    );
}

// 首采只用公开 products；条件/owner/phi 完整打印，不预设 normal 的 2-phi/3-incoming。
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropPoint, IterationCleanupAction as A, IterationExitKind as E,
    UnitDropPoint, UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
};

fn single_exit(
    f: &Fixture,
    site: &LoopSite,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    let (Some(d), Some(p)) = (
        typed.map_iteration(site.statement),
        owned.map_iteration(site.statement),
    ) else {
        return;
    };
    let expected = match f.exit {
        Exit::Continue => E::Continue(site.transfer),
        Exit::Break => E::Break(site.transfer),
        Exit::Return => E::Return(site.transfer),
    };
    report.check(
        "single real transfer expression and independent exhaustion only",
        p.exits().len() == 2
            && p.exits()
                .iter()
                .filter(|e| {
                    e.kind() == expected && e.point() == DropPoint::ControlTransfer(site.transfer)
                })
                .count()
                == 1
            && p.exits()
                .iter()
                .filter(|e| {
                    e.kind() == E::Exhaustion && e.point() == DropPoint::LoopExit(site.statement)
                })
                .count()
                == 1,
        (
            site.transfer,
            super::fixture::location(&f.sources, site.transfer_span),
            p.exits(),
        ),
    );
    let locals = names
        .symbols()
        .iter()
        .filter(|s| s.span() == site.local && s.name() == "local" && !s.is_synthetic())
        .collect::<Vec<_>>();
    let local = match locals.as_slice() {
        [s] => Some(s.id()),
        _ => None,
    };
    let construction = typed.construction(site.guard);
    report.check(
        "single real Guard initializer is owned Resource local",
        local.is_some_and(|s| owned.formed_owned_locals().contains(&s))
            && construction.is_some_and(|c| {
                c.expression() == site.guard
                    && c.arguments().is_empty()
                    && Some(c.result_type()) == typed.expression_type(site.guard)
                    && local.is_some_and(|s| typed.symbol_type(s) == Some(c.result_type()))
                    && typed.is_resource_type(c.result_type()) == Some(true)
            }),
        (
            locals,
            construction,
            super::fixture::location(&f.sources, site.guard_span),
        ),
    );
    let guards = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.target(), DropTarget::Named(s) if Some(s) == local))
        .collect::<Vec<_>>();
    report.check("single Guard lexical transfer Drop identity (never ASAP)",
        matches!(guards.as_slice(), [drop] if drop.point() == DropPoint::ControlTransfer(site.transfer)
            && drop.value_origin() == site.local
            && drop.owner().is_some_and(|owner| matches!(owned.cleanup_conditions().owner_value(owner), Some(CleanupOwnerValue::Expression { expression, origin }) if *expression == site.guard && *origin == site.guard_span))),
        &guards);
    report.check(
        "single components are never formed owned locals or Named Drop owners",
        !owned
            .formed_owned_locals()
            .iter()
            .any(|symbol| d.bindings().contains(symbol))
            && !owned.drops().iter().any(
                |drop| matches!(drop.target(), DropTarget::Named(s) if d.bindings().contains(&s)),
            ),
        owned.drops(),
    );
    let c = typed.call(site.call);
    let target = c.and_then(|c| {
        if let CallableTarget::Source(s) = c.target() {
            Some(s)
        } else {
            None
        }
    });
    let callee = target.and_then(|id| names.symbols().iter().find(|s| s.id() == id));
    report.check(
        "single body read/operand call selects exact actual source callee",
        c.is_some_and(|c| c.receiver().is_none() && c.arguments().len() == 2)
            && callee.is_some_and(|s| {
                s.name() == f.exit.callee()
                    && !s.is_synthetic()
                    && s.kind() == SymbolKind::Function
                    && s.span().source_id() == f.parsed.source_id()
            }),
        (
            site.call,
            super::fixture::location(&f.sources, site.call_span),
            c,
            callee,
        ),
    );
    if let Some(c) = c {
        for (i, argument) in c.arguments().iter().enumerate() {
            let refs = names
                .references()
                .iter()
                .filter(|r| site.arguments.get(i) == Some(&r.span()))
                .collect::<Vec<_>>();
            report.check("single body actual ordered component Borrow argument",
                i < 2 && matches!(refs.as_slice(), [r] if r.target() == &ReferenceTarget::Symbol(d.bindings()[i]))
                && argument.argument_index() == i && argument.parameter_index() == i
                && argument.mode() == ParameterMode::Borrow && [d.key_type(), d.value_type()].get(i) == Some(&argument.parameter_type()),
                (argument, refs));
        }
    }
    let result = typed
        .expression_type(site.call)
        .and_then(|ty| typed.types().get(ty));
    report.check(
        "single actual operand/read result type",
        result
            == Some(&TypeKind::Builtin(if f.exit == Exit::Return {
                BuiltinType::Int
            } else {
                BuiltinType::Unit
            })),
        result,
    );
    for exit in p.exits() {
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, a)| (*point == exit.point()).then_some(*a))
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 EXIT SINGLE POINT kind={:?} point={:?} condition={:?} resolved_condition={:?} complete={:#?}",
            exit.kind(),
            exit.point(),
            exit.condition(),
            owned.cleanup_conditions().get(exit.condition()),
            full
        );
        report.check(
            "single complete same-point cleanup is exact published sequence",
            full == exit.actions(),
            (&full, exit),
        );
        let tags = exit
            .actions()
            .iter()
            .filter_map(|action| match action {
                A::EndBinding { statement, symbol } => Some((
                    *statement,
                    if *symbol == d.value_binding() {
                        "value"
                    } else if *symbol == d.key_binding() {
                        "key"
                    } else {
                        "foreign-binding"
                    },
                )),
                A::FinishProvider(s) => Some((*s, "finish")),
                A::EndSource(s) => Some((*s, "source")),
                A::EndElement(s) => Some((*s, "illegal-element")),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = if exit.kind() == E::Exhaustion {
            vec!["finish", "source"]
        } else if matches!(exit.kind(), E::Continue(_)) {
            vec!["value", "key"]
        } else {
            vec!["value", "key", "finish", "source"]
        };
        report.check(
            "single same-provider exact component/terminal action order",
            tags.iter().all(|(s, _)| *s == site.statement)
                && tags.iter().map(|(_, tag)| *tag).collect::<Vec<_>>() == expected,
            &tags,
        );
        let guard_positions = exit
            .actions()
            .iter()
            .enumerate()
            .filter_map(|(i, a)| matches!(a, A::Drop(drop) if guards.contains(&drop)).then_some(i))
            .collect::<Vec<_>>();
        let value_position = exit.actions().iter().position(
            |a| matches!(a, A::EndBinding { symbol, .. } if *symbol == d.value_binding()),
        );
        report.check(
            "single Guard Drop precedes component ends only on transfer",
            if exit.kind() == E::Exhaustion {
                guard_positions.is_empty()
            } else {
                matches!(guard_positions.as_slice(), [g] if value_position.is_some_and(|v| *g < v))
            },
            (guard_positions, value_position),
        );
    }
}

fn unit_exit(
    f: &Fixture,
    site: &LoopSite,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    let [source] = names.names().source_units() else {
        return;
    };
    let unit = source.source_unit();
    let statement = UnitStatementId::new(unit, site.statement);
    let transfer = UnitExpressionId::new(unit, site.transfer);
    let guard = UnitExpressionId::new(unit, site.guard);
    let call = UnitExpressionId::new(unit, site.call);
    let (Some(d), Some(p)) = (
        typed.map_iteration(statement),
        owned.map_iteration(statement),
    ) else {
        return;
    };
    let expected = match f.exit {
        Exit::Continue => UE::Continue(transfer),
        Exit::Break => UE::Break(transfer),
        Exit::Return => UE::Return(transfer),
    };
    report.check(
        "unit real transfer expression and independent exhaustion only",
        p.exits().len() == 2
            && p.exits()
                .iter()
                .filter(|e| {
                    e.kind() == expected && e.point() == UnitDropPoint::ControlTransfer(transfer)
                })
                .count()
                == 1
            && p.exits()
                .iter()
                .filter(|e| {
                    e.kind() == UE::Exhaustion && e.point() == UnitDropPoint::LoopExit(statement)
                })
                .count()
                == 1,
        (
            transfer,
            super::fixture::location(&f.sources, site.transfer_span),
            p.exits(),
        ),
    );
    let locals = source
        .resolution()
        .symbols()
        .iter()
        .filter(|s| s.span() == site.local && s.name() == "local" && !s.is_synthetic())
        .collect::<Vec<_>>();
    let local = match locals.as_slice() {
        [s] => owned
            .formed_owned_locals()
            .iter()
            .copied()
            .find(|symbol| symbol.source_unit() == unit && symbol.symbol() == s.id()),
        _ => None,
    };
    let construction = typed.construction(guard);
    report.check(
        "unit real Guard initializer is owned Resource local",
        local.is_some_and(|s| owned.formed_owned_locals().contains(&s))
            && construction.is_some_and(|c| {
                c.expression() == guard
                    && c.arguments().is_empty()
                    && Some(c.result_type()) == typed.expression_type(guard)
                    && local.is_some_and(|s| typed.symbol_type(s) == Some(c.result_type()))
                    && typed.is_resource_type(c.result_type()) == Some(true)
            }),
        (
            locals,
            construction,
            super::fixture::location(&f.sources, site.guard_span),
        ),
    );
    let guards = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(s) if Some(s) == local))
        .collect::<Vec<_>>();
    report.check("unit Guard lexical transfer Drop target/origin (no single owner API)",
        matches!(guards.as_slice(), [drop] if drop.point() == UnitDropPoint::ControlTransfer(transfer) && drop.value_origin() == site.local), &guards);
    report.check(
        "unit components are never formed owned locals or Named Drop owners",
        !owned.drops().iter().any(
            |drop| matches!(drop.target(), UnitDropTarget::Named(s) if d.bindings().contains(&s)),
        ),
        owned.drops(),
    );
    let c = typed.call(call);
    let target = c.and_then(|c| {
        if let UnitCallTarget::Declaration(d) = c.target() {
            Some(d)
        } else {
            None
        }
    });
    let callee = target.and_then(|id| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == id)
    });
    report.check(
        "unit body read/operand call selects exact actual source declaration",
        c.is_some_and(|c| c.receiver().is_none() && c.arguments().len() == 2)
            && callee.is_some_and(|d| {
                d.name() == f.exit.callee()
                    && d.kind() == SymbolKind::Function
                    && d.source_unit() == unit
                    && f.parsed.roots().contains(&d.root())
            }),
        (
            call,
            super::fixture::location(&f.sources, site.call_span),
            c,
            callee,
        ),
    );
    if let Some(c) = c {
        for (i, argument) in c.arguments().iter().enumerate() {
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| site.arguments.get(i) == Some(&r.span()))
                .collect::<Vec<_>>();
            report.check("unit body actual ordered component Borrow argument",
                i < 2 && matches!(refs.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(d.bindings()[i]))
                && argument.argument_index() == i && argument.parameter_index() == i
                && argument.mode() == ParameterMode::Borrow && [d.key_type(), d.value_type()].get(i) == Some(&argument.parameter_type()),
                (argument, refs));
        }
    }
    let result = typed
        .expression_type(call)
        .and_then(|ty| typed.types().get(ty));
    report.check(
        "unit actual operand/read result type",
        result
            == Some(&UnitTypeKind::Builtin(if f.exit == Exit::Return {
                BuiltinType::Int
            } else {
                BuiltinType::Unit
            })),
        result,
    );
    for exit in p.exits() {
        let full = owned.iteration_cleanup_at(exit.point());
        println!(
            "SPEC0331 EXIT UNIT POINT kind={:?} point={:?} complete={full:#?}",
            exit.kind(),
            exit.point()
        );
        report.check(
            "unit complete same-point cleanup is exact public sequence",
            full == Some(exit.actions()),
            (full, exit),
        );
        let tags = exit
            .actions()
            .iter()
            .filter_map(|action| match action {
                UA::EndBinding { statement, symbol } => Some((
                    *statement,
                    if *symbol == d.value_binding() {
                        "value"
                    } else if *symbol == d.key_binding() {
                        "key"
                    } else {
                        "foreign-binding"
                    },
                )),
                UA::FinishProvider(s) => Some((*s, "finish")),
                UA::EndSource(s) => Some((*s, "source")),
                UA::EndElement(s) => Some((*s, "illegal-element")),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = if exit.kind() == UE::Exhaustion {
            vec!["finish", "source"]
        } else if matches!(exit.kind(), UE::Continue(_)) {
            vec!["value", "key"]
        } else {
            vec!["value", "key", "finish", "source"]
        };
        report.check(
            "unit same-provider exact component/terminal action order",
            tags.iter().all(|(s, _)| *s == statement)
                && tags.iter().map(|(_, tag)| *tag).collect::<Vec<_>>() == expected,
            &tags,
        );
        let guard_positions = exit
            .actions()
            .iter()
            .enumerate()
            .filter_map(|(i, a)| matches!(a, UA::Drop(drop) if guards.contains(&drop)).then_some(i))
            .collect::<Vec<_>>();
        let value_position = exit.actions().iter().position(
            |a| matches!(a, UA::EndBinding { symbol, .. } if *symbol == d.value_binding()),
        );
        report.check(
            "unit Guard Drop precedes component ends only on transfer",
            if exit.kind() == UE::Exhaustion {
                guard_positions.is_empty()
            } else {
                matches!(guard_positions.as_slice(), [g] if value_position.is_some_and(|v| *g < v))
            },
            (guard_positions, value_position),
        );
    }
}
