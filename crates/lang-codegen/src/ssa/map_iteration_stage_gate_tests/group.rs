//! Group whole-set：原五次 fresh 分析保留，Single 以真实 MemoryDocument Unit 准入。
//! 五次 fresh 实际管线全收集后断言；FE 红不能变成 Unsupported，查询红不能阻止 lower。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! 只读真实 Group/Name AST 身份；不调用生产 qualification。
    use super::super::fixture::{File, LoopSite};
    use super::{
        FOR_DIRECT, FOR_GROUP,
        fixture::{Fixture, Stopped, stopped},
    };
    use lang_frontend::{
        ast::ExpressionId,
        parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement},
        source::Span,
    };

    #[derive(Debug)]
    pub(super) struct Source {
        pub grouped: bool,
        pub name: ExpressionId,
        pub name_span: Span,
    }

    pub(super) fn source(fixture: &Fixture, file: &File, site: &LoopSite) -> Result<Source, Stopped> {
        let expressions = file.parsed.ast().expressions();
        let outer = expressions
            .get(site.source)
            .map_err(|error| stopped("Group AST source", error))?;
        let (grouped, name) = match outer.payload() {
            Expression::Name => (false, site.source),
            Expression::Group { expression } => (true, *expression),
            other => return Err(stopped("Group AST source shape", other)),
        };
        let inner = expressions
            .get(name)
            .map_err(|error| stopped("Group AST inner Name", error))?;
        let expected_for = if grouped { FOR_GROUP } else { FOR_DIRECT };
        let expected_source = if grouped { "(source)" } else { "source" };
        if inner.payload() != &Expression::Name
            || outer.span() != site.source_span
            || fixture.sources.slice(site.span) != Ok(expected_for)
            || fixture.sources.slice(site.source_span) != Ok(expected_source)
            || fixture.sources.slice(inner.span()) != Ok("source")
            || fixture.sources.source_name(site.span.source_id()) != Ok(file.path.as_str())
            || (grouped && (name == site.source || inner.span() == site.source_span))
        {
            return Err(stopped(
                "Group AST exact full For/source/inner Name",
                (site, outer, inner),
            ));
        }
        Ok(Source {
            grouped,
            name,
            name_span: inner.span(),
        })
    }

    pub(super) fn empty_entry(file: &File, name_span: Span) -> bool {
        file.parsed.ast().items().iter().any(|(_, node)| {
            let Item::Function {
                name: NameMarker::Present(name),
                form,
                ..
            } = node.payload()
            else {
                return false;
            };
            if *name != name_span {
                return false;
            }
            let block = match form {
                FunctionForm::Explicit {
                    body: FunctionBody::Block(block),
                    ..
                }
                | FunctionForm::ImplicitUnitBlock(block) => *block,
                _ => return false,
            };
            file.parsed.ast().statements().get(block).is_ok_and(
                |node| matches!(node.payload(), Statement::Block { elements } if elements.is_empty()),
            )
        })
    }
}
mod cleanup {
    //! 完整 normal 清理与 direct-own LoopExit owner 旁证；只读，不构造事实。
    use super::super::fixture::{File, LoopSite};
    use super::Report;
    use lang_frontend::{
        ownership_checking::{
            CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
            UnitIterationExitKind, UnitLoanTarget, UnitMapIterationOwnershipPlan,
        },
        parser::Statement,
        type_checking::UnitStatementId,
    };

    pub(super) fn unit(
        file: &File,
        site: &LoopSite,
        plan: &UnitMapIterationOwnershipPlan,
        owned: &CompilationUnitOwnership,
        is_owned: bool,
        report: &mut Report,
    ) {
        let statement = plan.descriptor().statement();
        let unit = statement.source_unit();
        let body = file
            .parsed
            .ast()
            .statements()
            .get(site.statement)
            .ok()
            .and_then(|node| {
                if let Statement::For { body, .. } = node.payload() {
                    Some(UnitStatementId::new(unit, *body))
                } else {
                    None
                }
            });
        let falls = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() == UnitIterationExitKind::Fallthrough)
            .collect::<Vec<_>>();
        let ends = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() == UnitIterationExitKind::Exhaustion)
            .collect::<Vec<_>>();
        report.check(
            "unit exactly normal body and exhaustion",
            plan.exits().len() == 2 && falls.len() == 1 && ends.len() == 1,
            plan.exits(),
        );
        let ([fall], [end]) = (falls.as_slice(), ends.as_slice()) else {
            return;
        };
        let descriptor = plan.descriptor();
        report.check(
            "unit complete normal body actions and point",
            Some(fall.point()) == body.map(UnitDropPoint::AfterStatement)
                && fall.actions()
                    == [
                        UA::EndBinding {
                            statement,
                            symbol: descriptor.value_binding(),
                        },
                        UA::EndBinding {
                            statement,
                            symbol: descriptor.key_binding(),
                        },
                    ]
                && end.point() == UnitDropPoint::LoopExit(statement),
            (fall, end),
        );
        for exit in [fall, end] {
            // unit getter 是同表公共查询，不声称独立存储。
            report.check(
                "unit complete same-point cleanup getter",
                owned.iteration_cleanup_at(exit.point()) == Some(exit.actions()),
                (exit, owned.iteration_cleanup_at(exit.point())),
            );
        }
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            report.check("unit source cleanup root exists", false, plan.source());
            return;
        };
        let source = place.root();
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(source))
            .collect::<Vec<_>>();
        if !is_owned {
            report.check(
                "unit Borrow source has no owner drop",
                drops.is_empty()
                    && end.actions() == [UA::FinishProvider(statement), UA::EndSource(statement)],
                (drops, end),
            );
            return;
        }
        report.check(
            "unit direct Map own single source-qualified drop",
            drops.len() == 1,
            &drops,
        );
        let [drop] = drops.as_slice() else {
            return;
        };
        report.check(
            "unit direct Map own only LoopExit drop after source end",
            drop.point() == end.point()
                && end.actions()
                    == [
                        UA::FinishProvider(statement),
                        UA::EndSource(statement),
                        UA::Drop(**drop),
                    ],
            (drop, end),
        );
        // 原 owner/phi/incoming/条件/无 capture 义务由同轮 group::ssa 的真实图承接。
        // 不构造旧 FE 表，也不把同型实体视为同一 owner。
    }
}
mod facts {
    //! 全部四个真实 For 的只读身份合同；Group 集合排序独立于生产能力门。
    use super::super::fixture::loop_sites;
    use super::{
        Report, ast, cleanup,
        fixture::{Fixture, Location, Stopped, location, stopped},
    };
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        name_resolution::{
            SourceUnitKey, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
            OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
        },
        type_checking::{
            BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
            ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) use super::ast::empty_entry;

    fn kind(constructor: IntrinsicTypeConstructor) -> Option<MapIterationKind> {
        match constructor {
            IntrinsicTypeConstructor::Map => Some(MapIterationKind::Map),
            IntrinsicTypeConstructor::MutableMap => Some(MapIterationKind::MutableMap),
            _ => None,
        }
    }

    fn role_counts(roles: &[(bool, MapIterationKind)]) -> bool {
        roles.len() == 4
            && [false, true].into_iter().all(|grouped| {
                [MapIterationKind::Map, MapIterationKind::MutableMap]
                    .into_iter()
                    .all(|kind| {
                        roles
                            .iter()
                            .filter(|role| **role == (grouped, kind))
                            .count()
                            == 1
                    })
            })
    }

    pub(super) fn unit(
        fixture: &Fixture,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) -> Result<Location, Stopped> {
        diagnostic!(
            "SPEC0331 GROUP UNIT FACTS typed={:#?} ownership={owned:#?}",
            typed.map_iterations()
        );
        report.check(
            "unit exact four typed/owned Map sites",
            typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
            (typed.map_iterations(), owned.map_iterations()),
        );
        let memory = matches!(names.names().index().source_units(), [source]
            if source.key() == &SourceUnitKey::MemoryDocument);
        let mut all_sites = Vec::new();
        let mut groups = Vec::new();
        let mut roles = Vec::new();
        for source_names in names.names().source_units() {
            let source_unit = source_names.source_unit();
            let Some(file) = fixture
                .files
                .iter()
                .find(|file| file.parsed.source_id() == source_names.resolution().source_id())
            else {
                report.check(
                    "unit source unit maps to actual parsed source",
                    false,
                    source_unit,
                );
                continue;
            };
            let sites = loop_sites(file);
            report.check("unit actual AST For query", sites.is_ok(), &sites);
            let Ok(sites) = sites else {
                continue;
            };
            report.check(
                "unit per-file loop count",
                sites.len()
                    == if memory {
                        4
                    } else {
                        usize::from(file.constructor.is_some())
                    },
                (&file.path, &sites),
            );
            for site in sites {
                let statement = UnitStatementId::new(source_unit, site.statement);
                let source_expression = UnitExpressionId::new(source_unit, site.source);
                all_sites.push((statement, site.span));
                let shape = ast::source(fixture, file, &site);
                report.check(
                    "unit actual Group or direct Name AST",
                    shape.is_ok(),
                    &shape,
                );
                let Ok(shape) = shape else {
                    continue;
                };
                let inner = UnitExpressionId::new(source_unit, shape.name);
                let descriptor = typed.map_iteration(statement);
                let plan = owned.map_iteration(statement);
                let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
                    report.check(
                        "unit real Map descriptor/plan exist",
                        false,
                        (descriptor, plan),
                    );
                    continue;
                };
                roles.push((shape.grouped, descriptor.kind()));
                let identity = descriptor.statement() == statement
                    && descriptor.source() == source_expression
                    && Some(descriptor.source_type()) == typed.expression_type(source_expression)
                    && typed.expression_type(inner) == typed.expression_type(source_expression)
                    && plan.descriptor() == descriptor
                    && typed.sequential_iteration(statement).is_none()
                    && owned.iteration(statement).is_none();
                report.check(
                    "unit qualified outer Group/inner Name descriptor identity",
                    identity,
                    (&shape, inner, descriptor, plan.descriptor()),
                );
                let source_type = typed.types().get(descriptor.source_type());
                report.check("unit intrinsic distinct Int/String K/V", matches!(source_type,
                    Some(UnitTypeKind::Intrinsic { constructor, arguments })
                    if kind(*constructor) == Some(descriptor.kind())
                        && file.constructor.is_none_or(|expected| expected == (*constructor, descriptor.kind()))
                        && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                        && typed.types().get(descriptor.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                        && typed.types().get(descriptor.value_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), source_type);
                let references = names
                    .names()
                    .references()
                    .iter()
                    .filter_map(|reference| {
                        if reference.span() == shape.name_span
                            && let UnitReferenceTarget::Symbol(symbol) = reference.target()
                        {
                            Some(*symbol)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let source = match references.as_slice() {
                    [source] => Some(*source),
                    _ => None,
                };
                let symbol = source.and_then(|id| {
                    source_names
                        .resolution()
                        .symbols()
                        .iter()
                        .find(|s| s.id() == id.symbol())
                });
                let parameters = typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|s| s.callable())
                    .flat_map(|c| c.parameters())
                    .filter(|p| source.is_some() && p.symbol() == source)
                    .collect::<Vec<_>>();
                let is_owned = !shape.grouped && descriptor.kind() == MapIterationKind::Map;
                let mode = if is_owned {
                    ParameterMode::Value
                } else {
                    ParameterMode::Borrow
                };
                let binding = if is_owned {
                    OwnershipBindingKind::Owned
                } else {
                    OwnershipBindingKind::Shared
                };
                let access = if is_owned {
                    UnitIterationSourceAccess::Owned
                } else {
                    UnitIterationSourceAccess::Shared
                };
                let parameter = symbol.is_some_and(|symbol| {
                    symbol.kind() == SymbolKind::ValueParameter
                        && !symbol.is_synthetic()
                        && symbol.span().source_id() == file.parsed.source_id()
                }) && source.is_some_and(|source| {
                    source.source_unit() == source_unit
                        && typed.symbol_type(source) == Some(descriptor.source_type())
                        && owned.binding_kind(source) == Some(binding)
                }) && matches!(parameters.as_slice(), [parameter] if parameter.mode() == mode
                        && parameter.ty() == descriptor.source_type());
                report.check(
                    "unit inner Name unique qualified root/mode parameter",
                    parameter,
                    (&references, symbol, parameters, mode, binding),
                );
                let origin = matches!(plan.source(), Some(UnitLoanTarget::Place(place))
                        if Some(place.root()) == source && place.is_root())
                    && matches!(plan.source_kind(), IterationSource::BorrowedSource(origin)
                        if Some(origin) == plan.source())
                    && plan.source_access() == access;
                report.check(
                    "unit actual qualified source-kind/root/access",
                    origin,
                    (plan.source(), plan.source_kind(), plan.source_access()),
                );
                report.check(
                    "unit distinct component bindings/loans",
                    descriptor.key_binding() != descriptor.value_binding()
                        && !source.is_some_and(|s| descriptor.bindings().contains(&s))
                        && plan.loans()[0] != plan.loans()[1],
                    plan.loans(),
                );
                for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                    .into_iter()
                    .enumerate()
                {
                    let symbol = source_names.resolution().symbols().iter().find(|symbol| {
                        symbol.span() == site.bindings[index]
                            && !symbol.is_synthetic()
                            && symbol.kind() == SymbolKind::ForBinding
                    });
                    let binding = descriptor.bindings()[index];
                    let component_type = [descriptor.key_type(), descriptor.value_type()][index];
                    let loan = plan.loan(component);
                    report.check(
                        "unit actual component qualified statement/source/binding/origin",
                        binding.source_unit() == source_unit
                            && symbol.is_some_and(|s| s.id() == binding.symbol())
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
                        (symbol, loan),
                    );
                }
                cleanup::unit(file, &site, plan, owned, is_owned, report);
                if shape.grouped {
                    groups.push((descriptor.statement(), site.span));
                    report.check(
                        "unit candidate Group source identity",
                        identity && parameter && origin,
                        (statement, source_expression, inner),
                    );
                }
            }
        }
        report.check(
            "unit exact four actual AST sites and original source grouping",
            all_sites.len() == 4
                && all_sites.iter().enumerate().all(|(i, a)| {
                    all_sites
                        .iter()
                        .skip(i + 1)
                        .all(|b| a.0 != b.0 && (a.0.source_unit() == b.0.source_unit()) == memory)
                }),
            &all_sites,
        );
        report.check(
            "unit one of each direct/Group Map kind",
            role_counts(&roles),
            &roles,
        );
        groups.sort_by_key(|(statement, _)| *statement);
        report.check(
            "unit two distinct Group sites sorted by actual UnitStatementId",
            groups.len() == 2 && groups[0].0 != groups[1].0 && groups[0].1 != groups[1].1,
            &groups,
        );
        let [(first, span), _] = groups.as_slice() else {
            return Err(stopped("unit exact Group oracle set", &groups));
        };
        let expected = location(&fixture.sources, *span)
            .ok_or_else(|| stopped("unit full Group For location", (first, span)))?;
        report.check(
            "unit first qualified Group full For/path/text",
            true,
            (first, &expected),
        );
        Ok(expected)
    }
}
mod fixture {
    //! Group 原位置 oracle 与 selected-entry 结果；全部根另在同次 report 中逐一验证。
    use super::super::fixture::LowerOutcome as RootOutcome;
    pub(super) use super::super::fixture::{Fixture, Location, Stopped, clean, location, stopped};

    #[derive(Debug)]
    pub(super) struct LowerOutcome {
        expected_group: Result<Location, Stopped>,
        selected_entry: RootOutcome,
    }

    impl LowerOutcome {
        pub fn new(expected_group: Result<Location, Stopped>, selected_entry: RootOutcome) -> Self {
            Self {
                expected_group,
                selected_entry,
            }
        }

        pub fn matches(&self) -> bool {
            self.expected_group
                .as_ref()
                .is_ok_and(|original| original.text == super::FOR_GROUP)
                && self.selected_entry.matches()
        }
    }
}
mod ssa;
mod unit {
    //! Basic/const 各自取得真实 capability；只读 query 不改写同轮阶段产物。
    use super::super::roots;
    use super::{
        Pathway, Report, facts,
        fixture::{Fixture, LowerOutcome, Stopped, clean, stopped},
        ssa,
    };
    use lang_frontend::{
        name_resolution::{SymbolKind, index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
            const_owned_compilation_unit_view, owned_compilation_unit_view,
        },
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };

    pub(super) fn collect(
        fixture: &Fixture,
        pathway: Pathway,
        reverse_inputs: bool,
        entry_name: &str,
        report: &mut Report,
    ) -> Result<LowerOutcome, Stopped> {
        let sources = &fixture.sources;
        let inputs = fixture.inputs(pathway, reverse_inputs)?;
        report.check(
            "unit lex/parse clean and actual input order",
            true,
            inputs
                .iter()
                .map(|input| input.source_id())
                .collect::<Vec<_>>(),
        );
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(sources, &inputs)
            .map_err(|error| stopped("unit index infrastructure", error))?;
        clean(sources, "unit index diagnostics", index.diagnostics())?;
        let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
            .map_err(|error| stopped("unit names infrastructure", error))?;
        clean(sources, "unit names diagnostics", names.diagnostics())?;
        let names = names
            .validate()
            .map_err(|error| stopped("unit names qualification", error))?;
        fixture.check_input_identity(pathway, &names, report);
        let entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == entry_name && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            return Err(stopped("unit real entry identity", entries));
        };
        report.check("unit names and real entry", true, entry);
        let expected_entry = fixture.function_location(entry.name_span());
        report.check(
            "unit real entry full source origin",
            expected_entry.is_some(),
            &expected_entry,
        );
        {
            let entry_source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|source| source.id() == entry.source_unit())
                .map(|source| source.source_id());
            let file = fixture
                .files
                .iter()
                .find(|file| Some(file.parsed.source_id()) == entry_source);
            report.check(
                "unit entry empty and calls no Map function",
                file.is_some_and(|file| facts::empty_entry(file, entry.name_span())),
                (entry, entry_source),
            );
        }
        let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
            .map_err(|error| stopped("unit raw infrastructure", error))?;
        report.check(
            "unit raw diagnostics",
            raw.diagnostics().is_empty(),
            raw.diagnostics(),
        );
        {
            let complete =
                complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                    .map_err(|error| stopped("unit complete admission", error))?;
            let same = complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw);
            report.check(
                "unit complete original analysis",
                same,
                raw.is_compatible_with(sources, &inputs, &names, &environment),
            );
            if !same {
                return Err(stopped(
                    "unit complete identity",
                    "not the original raw product",
                ));
            }
        }
        clean(sources, "unit raw diagnostics", raw.diagnostics())?;
        match pathway {
            Pathway::Single | Pathway::Basic => {
                let typed = raw
                    .validate()
                    .map_err(|error| stopped("unit basic type qualification", error))?;
                report.check(
                    "unit basic type qualification",
                    true,
                    "original raw consumed",
                );
                let owned =
                    check_compilation_unit_ownership(sources, &inputs, &names, &environment, &typed)
                        .map_err(|error| stopped("unit basic ownership infrastructure", error))?;
                clean(
                    sources,
                    "unit basic ownership diagnostics",
                    owned.diagnostics(),
                )?;
                if !owned.deferred().is_empty() {
                    return Err(stopped("unit basic ownership deferred", owned.deferred()));
                }
                report.check(
                    "unit basic ownership original identity",
                    owned.is_compatible_with(&typed),
                    owned.deferred(),
                );
                let expected = facts::unit(fixture, &names, typed.types(), &owned, report);
                // validate() 公开入口包括完整 iterations_are_valid；view factory 另验来源身份。
                let owned = owned
                    .validate()
                    .map_err(|error| stopped("unit basic full ownership validator", error))?;
                report.check("unit basic full ownership validator", true, "validate: Ok");
                let view =
                    owned_compilation_unit_view(sources, &inputs, &names, &environment, &typed, &owned)
                        .map_err(|error| stopped("unit basic owned view identity", error))?;
                report.check(
                    "unit basic view preserves original inputs",
                    std::ptr::eq(view.sources(), sources)
                        && std::ptr::eq(view.inputs(), inputs.as_slice())
                        && std::ptr::eq(view.names(), &names)
                        && std::ptr::eq(view.types(), typed.types())
                        && std::ptr::eq(view.ownership(), owned.ownership()),
                    "source/input/name/type/ownership references",
                );
                let outcome = roots::collect(
                    fixture,
                    &names,
                    typed.types(),
                    entry.id(),
                    report,
                    |root, report| {
                        let lowered = crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, root);
                        if let Ok((program, entry)) = &lowered {
                            ssa::collect(
                                ssa::Frontend {
                                    fixture,
                                    names: &names,
                                    typed: typed.types(),
                                    owned: owned.ownership(),
                                },
                                root,
                                program,
                                *entry,
                                report,
                            );
                        }
                        lowered
                    },
                )?;
                Ok(LowerOutcome::new(expected, outcome))
            }
            Pathway::Const => {
                // 没有常量声明也必须取得专用 capability；绝不借 basic view 或转换伪造。
                let typed = raw
                    .validate_constants()
                    .map_err(|error| stopped("unit const type qualification", error))?;
                report.check(
                    "unit const type qualification",
                    true,
                    "original raw consumed by validate_constants",
                );
                let owned = check_compilation_unit_constant_ownership(
                    sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                )
                .map_err(|error| stopped("unit const ownership infrastructure", error))?;
                clean(
                    sources,
                    "unit const ownership diagnostics",
                    owned.ownership().diagnostics(),
                )?;
                if !owned.ownership().deferred().is_empty() {
                    return Err(stopped(
                        "unit const ownership deferred",
                        owned.ownership().deferred(),
                    ));
                }
                report.check(
                    "unit const ownership original identity",
                    owned.is_compatible_with(&typed),
                    owned.ownership().deferred(),
                );
                let expected = facts::unit(fixture, &names, typed.types(), owned.ownership(), report);
                let owned = owned
                    .validate()
                    .map_err(|error| stopped("unit const full ownership validator", error))?;
                report.check(
                    "unit const full ownership validator",
                    true,
                    "constant ownership validate: Ok",
                );
                let view = const_owned_compilation_unit_view(
                    sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                )
                .map_err(|error| stopped("unit const owned view identity", error))?;
                report.check(
                    "unit const view preserves original capability",
                    std::ptr::eq(view.sources(), sources)
                        && std::ptr::eq(view.inputs(), inputs.as_slice())
                        && std::ptr::eq(view.names(), &names)
                        && std::ptr::eq(view.types(), typed.types())
                        && std::ptr::eq(view.ownership(), owned.ownership())
                        && std::ptr::eq(view.constant_ownership(), &owned),
                    "const references, no cast",
                );
                let outcome = roots::collect(
                    fixture,
                    &names,
                    typed.types(),
                    entry.id(),
                    report,
                    |root, report| {
                        let lowered =
                            crate::ssa::unit_lower::constant::lower_const_owned_unit_with_entry(
                                &view, root,
                            );
                        if let Ok((program, entry)) = &lowered {
                            ssa::collect(
                                ssa::Frontend {
                                    fixture,
                                    names: &names,
                                    typed: typed.types(),
                                    owned: owned.ownership(),
                                },
                                root,
                                program,
                                *entry,
                                report,
                            );
                        }
                        lowered
                    },
                )?;
                Ok(LowerOutcome::new(expected, outcome))
            }
        }
    }
}

use super::{Pathway, Report, SourceCase};
use fixture::{Fixture, LowerOutcome, Stopped};
use lang_frontend::type_checking::{IntrinsicTypeConstructor, MapIterationKind};

const FOR_DIRECT: &str = "for ((key, value) in source) { val copied: Int = key; println(value) }";
const FOR_GROUP: &str = "for ((key, value) in (source)) { val copied: Int = key; println(value) }";
const CASES: [SourceCase; 5] = [
    SourceCase {
        path: "entry.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/entry.ko"),
        constructor: None,
    },
    SourceCase {
        path: "z_direct_map.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/z_direct_map.ko"),
        constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
    },
    SourceCase {
        path: "a_direct_mutable.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/a_direct_mutable.ko"),
        constructor: Some((
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
        )),
    },
    SourceCase {
        path: "y_group_map.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/y_group_map.ko"),
        constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
    },
    SourceCase {
        path: "b_group_mutable.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/b_group_mutable.ko"),
        constructor: Some((
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
        )),
    },
];

#[derive(Debug)]
struct Observation {
    label: String,
    report: Report,
    result: Result<LowerOutcome, Stopped>,
}

impl Observation {
    fn passed(&self) -> bool {
        self.report.passed() && self.result.as_ref().is_ok_and(LowerOutcome::matches)
    }
}

fn observe(pathway: Pathway, reverse_inputs: bool) -> Observation {
    let mut report = Report::default();
    let fixture = match pathway {
        Pathway::Single => {
            let text = CASES.iter().map(|case| case.text).collect::<String>();
            diagnostic!("SPEC0331 GROUP INPUT single path=whole.ko text={text:?}");
            Fixture::single_whole(&text)
        }
        Pathway::Basic | Pathway::Const => {
            for case in CASES {
                diagnostic!(
                    "SPEC0331 GROUP INPUT pathway={pathway:?} reverse={reverse_inputs} path={} text={:?}",
                    case.path,
                    case.text
                );
            }
            Fixture::new(&CASES)
        }
    };
    let result = fixture
        .and_then(|fixture| unit::collect(&fixture, pathway, reverse_inputs, "entry", &mut report));
    Observation {
        label: format!("group-whole-set/{pathway:?}/reverse={reverse_inputs}"),
        report,
        result,
    }
}

#[test]
fn grouped_sources_reach_verified_llvm_with_actual_group_identity() {
    let mut observations = vec![observe(Pathway::Single, false)];
    for pathway in [Pathway::Basic, Pathway::Const] {
        for reverse in [false, true] {
            observations.push(observe(pathway, reverse));
        }
    }
    for observation in &observations {
        diagnostic!(
            "SPEC0331 GROUP OUTCOME {}\n{:#?}\n{:#?}",
            observation.label,
            observation.result,
            observation.report
        );
        if let Err(stopped) = &observation.result {
            diagnostic!(
                "LOWER_NOT_ATTEMPTED stage={} original={}",
                stopped.stage,
                stopped.original
            );
        }
    }
    assert_diagnostic!(
        observations.len() == 5 && observations.iter().all(Observation::passed),
        "Group whole-set must preserve every original FE identity query and reach verified LLVM on all five fresh paths.\n{observations:#?}"
    );
}
