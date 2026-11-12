//! SPEC-0331 最小 provider 资格的普通源码阶段门。
//! 原 Single 列使用 MemoryDocument Unit；源码字节、六/五观察格与全部阶段义务保留。
//! whole-set 在同一 sealed view 逐真实根完成 SSA/LLVM，不拼接 Program，不执行 native。

use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod facts {
    //! 公共只读事实 oracle；query 失败只记录，不能短路已经具备资格的 lower。
    use super::{
        Report,
        fixture::{FOR_TEXT, File, Fixture, loop_sites},
    };
    use lang_frontend::{
        name_resolution::{
            SourceUnitKey, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
            OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement},
        source::Span,
        type_checking::{
            BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
            ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };

    fn kind(constructor: IntrinsicTypeConstructor) -> Option<MapIterationKind> {
        match constructor {
            IntrinsicTypeConstructor::Map => Some(MapIterationKind::Map),
            IntrinsicTypeConstructor::MutableMap => Some(MapIterationKind::MutableMap),
            _ => None,
        }
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

    pub(super) fn unit(
        fixture: &Fixture,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        whole_set: bool,
        report: &mut Report,
    ) {
        let count = if whole_set { 2 } else { 1 };
        let memory = matches!(names.names().index().source_units(), [source]
            if source.key() == &SourceUnitKey::MemoryDocument);
        report.check(
            "unit exact typed/owned Map counts",
            typed.map_iterations().len() == count && owned.map_iterations().len() == count,
            (typed.map_iterations(), owned.map_iterations()),
        );
        let mut all_sites = Vec::new();
        for source_names in names.names().source_units() {
            let source_unit = source_names.source_unit();
            let Some(file) = fixture
                .files
                .iter()
                .find(|file| file.parsed.source_id() == source_names.resolution().source_id())
            else {
                report.check("unit parsed source identity", false, source_unit);
                continue;
            };
            let sites = match loop_sites(file) {
                Ok(sites) => sites,
                Err(error) => {
                    report.check("AST query", false, error);
                    continue;
                }
            };
            report.check(
                "unit AST fixture loop count",
                sites.len()
                    == if memory {
                        count
                    } else {
                        usize::from(file.constructor.is_some())
                    },
                (&file.path, &sites),
            );
            for site in sites {
                let statement = UnitStatementId::new(source_unit, site.statement);
                let source_expression = UnitExpressionId::new(source_unit, site.source);
                let descriptor = typed.map_iteration(statement);
                let plan = owned.map_iteration(statement);
                all_sites.push((statement, site.span));
                report.check(
                    "unit complete AST For/path/source",
                    fixture.sources.slice(site.span) == Ok(FOR_TEXT)
                        && fixture.sources.slice(site.source_span) == Ok("source")
                        && fixture.sources.source_name(site.span.source_id()) == Ok(file.path.as_str()),
                    &site,
                );
                let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
                    report.check("unit Map facts exist", false, (descriptor, plan));
                    continue;
                };
                report.check(
                    "unit descriptor identity",
                    descriptor.statement() == statement
                        && descriptor.source() == source_expression
                        && Some(descriptor.source_type()) == typed.expression_type(source_expression)
                        && plan.descriptor() == descriptor
                        && typed.sequential_iteration(statement).is_none()
                        && owned.iteration(statement).is_none(),
                    (descriptor, plan.descriptor()),
                );
                let source_type = typed.types().get(descriptor.source_type());
                report.check("unit intrinsic distinct K/V", matches!(source_type,
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
                let parameters = typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|signature| signature.callable())
                    .flat_map(|callable| callable.parameters())
                    .filter(|parameter| source.is_some() && parameter.symbol() == source)
                    .collect::<Vec<_>>();
                report.check(
                    "unit direct Name and unique real Borrow parameter",
                    file.parsed.ast().expressions().get(site.source).is_ok_and(|node| {
                    matches!(node.payload(), Expression::Name)
                })
                        && source_symbol.is_some_and(|symbol| {
                            symbol.kind() == SymbolKind::ValueParameter && !symbol.is_synthetic()
                                && symbol.span().source_id() == file.parsed.source_id()
                        })
                        && matches!(parameters.as_slice(), [parameter]
                            if parameter.mode() == ParameterMode::Borrow && parameter.ty() == descriptor.source_type()),
                    (&references, source_symbol, parameters),
                );
                report.check(
                    "unit actual Borrow source origin",
                    matches!(plan.source(), Some(UnitLoanTarget::Place(place))
                    if Some(place.root()) == source && place.is_root())
                        && matches!(plan.source_kind(), IterationSource::BorrowedSource(_))
                        && plan.source_access() == UnitIterationSourceAccess::Shared
                        && source.is_some_and(|source| {
                            source.source_unit() == source_unit
                                && typed.symbol_type(source) == Some(descriptor.source_type())
                                && owned.binding_kind(source) == Some(OwnershipBindingKind::Shared)
                        }),
                    (source, plan.source_kind(), plan.source_access()),
                );
                report.check(
                    "unit separate component identities",
                    descriptor.key_binding() != descriptor.value_binding()
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
                        "unit real binding/type/Shared loan",
                        binding.source_unit() == source_unit
                            && symbol.is_some_and(|symbol| symbol.id() == binding.symbol())
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
            }
        }
        report.check(
            "unit exact AST source-set count",
            all_sites.len() == count,
            &all_sites,
        );
        if whole_set {
            report.check(
                "unit both Map kinds in unused functions",
                typed
                    .map_iterations()
                    .iter()
                    .any(|descriptor| descriptor.kind() == MapIterationKind::Map)
                    && typed
                        .map_iterations()
                        .iter()
                        .any(|descriptor| descriptor.kind() == MapIterationKind::MutableMap),
                typed.map_iterations(),
            );
            report.check(
                "unit actual source grouping",
                all_sites.len() == 2
                    && (all_sites[0].0.source_unit() == all_sites[1].0.source_unit()) == memory,
                &all_sites,
            );
        }
    }
}
#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::sequential_for_lowering_tests::source_cfg_tests::fixture_6 as fixture;
mod group;
mod roots {
    //! 同一 sealed view 逐真实声明 lower；结果逐根验证，不拼接 Program 或重跑 frontend。
    use super::{
        Report,
        fixture::{Fixture, LowerOutcome, Stopped, stopped},
    };
    use crate::ssa::{
        LoweringError,
        model::{FunctionId, Origin, Program},
    };
    use lang_frontend::{
        name_resolution::{DeclarationId, SymbolKind, UnitDeclaration, ValidatedCompilationUnitNames},
        parser::{Item, NameMarker},
        source::Span,
        type_checking::{CompilationUnitTypes, UnitCallTarget},
    };

    pub(super) fn collect(
        fixture: &Fixture,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        selected: DeclarationId,
        report: &mut Report,
        mut lower: impl FnMut(DeclarationId, &mut Report) -> Result<(Program, FunctionId), LoweringError>,
    ) -> Result<LowerOutcome, Stopped> {
        let declarations = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() == SymbolKind::Function)
            .collect::<Vec<_>>();
        let source_roots = fixture
            .files
            .iter()
            .flat_map(|file| {
                file.parsed.roots().iter().map(|root| {
                    file.parsed.ast().items().get(*root).ok().and_then(|node| {
                        let Item::Function {
                            name: NameMarker::Present(name),
                            ..
                        } = node.payload()
                        else {
                            return None;
                        };
                        Some((*root, *name, node.span()))
                    })
                })
            })
            .collect::<Vec<_>>();
        report.check(
            "unit every original function root has one real declaration",
            !source_roots.is_empty()
                && source_roots.len() == declarations.len()
                && source_roots.iter().all(|root| {
                    root.is_some_and(|(item, name, _)| {
                        declarations
                            .iter()
                            .filter(|declaration| {
                                declaration.root() == item && declaration.name_span() == name
                            })
                            .count()
                            == 1
                    })
                })
                && declarations
                    .iter()
                    .filter(|declaration| declaration.id() == selected)
                    .count()
                    == 1,
            (&source_roots, &declarations, selected),
        );
        let mut selected_outcome = None;
        for declaration in &declarations {
            let expected = fixture.function_location(declaration.name_span());
            check_dependencies(fixture, names, typed, declaration, report);
            // query 失败不能取消已获准的 lower；每根错误也不能遮住其余根。
            let lowered = lower(declaration.id(), report);
            report.check(
                "unit real declaration lower actually attempted",
                true,
                (declaration.id(), declaration.name(), lowered.as_ref().err()),
            );
            if let Ok((program, entry)) = &lowered {
                let actual_sources = program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .filter_map(|function| {
                        declarations.iter().find_map(|candidate| {
                            fixture
                                .function_location(candidate.name_span())
                                .filter(|location| function.origin == Origin::Source(location.span))
                                .map(|location| (candidate.id(), function.id(), location))
                        })
                    })
                    .collect::<Vec<_>>();
                // 这些原 fixture 只调用 builtin println，各根没有源码 callable 依赖。
                // 每个 Program 必须包含自己的根，不能借别的根 Program 补漏。
                report.check(
                    "unit per-root actual source dependency coverage",
                    matches!(actual_sources.as_slice(), [(root, function, location)]
                        if *root == declaration.id() && *function == *entry
                            && Some(location) == expected.as_ref()),
                    (declaration.id(), &actual_sources),
                );
            }
            let outcome = LowerOutcome::new(&fixture.sources, expected, lowered);
            report.check(
                "unit per-root source identity SSA and complete LLVM",
                outcome.matches(),
                (declaration.id(), declaration.name(), &outcome),
            );
            if declaration.id() == selected {
                selected_outcome = Some(outcome);
            }
        }
        selected_outcome.ok_or_else(|| stopped("unit selected root outcome", selected))
    }

    fn check_dependencies(
        fixture: &Fixture,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        declaration: &UnitDeclaration,
        report: &mut Report,
    ) {
        let source = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|source| source.id() == declaration.source_unit());
        let file = source.and_then(|source| {
            fixture
                .files
                .iter()
                .find(|file| file.parsed.source_id() == source.source_id())
        });
        let expected = fixture.function_location(declaration.name_span());
        report.check(
            "unit root retains original source-qualified AST identity",
            file.is_some_and(|file| {
                file.parsed.roots().contains(&declaration.root())
                    && file.parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                        matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                            if *name == declaration.name_span())
                            && expected.as_ref().is_some_and(|expected| expected.span == node.span())
                    })
            }),
            (declaration, &expected),
        );
        let mut calls = Vec::new();
        let mut complete = true;
        for call in typed
            .calls()
            .iter()
            .filter(|call| call.expression().source_unit() == declaration.source_unit())
        {
            let span = file.and_then(|file| {
                file.parsed
                    .ast()
                    .expressions()
                    .get(call.expression().expression())
                    .ok()
                    .map(|node| node.span())
            });
            match (span, expected.as_ref()) {
                (Some(span), Some(expected)) if within(span, expected.span) => calls.push(call),
                (Some(_), Some(_)) => {}
                _ => complete = false,
            }
        }
        report.check(
            "unit original root has only actual builtin println dependencies",
            complete
                && calls.iter().all(|call| {
                    matches!(call.target(), UnitCallTarget::External(_)) && call.prints_line()
                }),
            (declaration.id(), calls),
        );
    }

    fn within(inner: Span, outer: Span) -> bool {
        inner.source_id() == outer.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }
}
mod unit {
    //! Basic/const 各自取得真实 capability；只读 query 不改写同轮阶段产物。
    use super::{
        Pathway, Report, facts,
        fixture::{Fixture, LowerOutcome, Stopped, clean, stopped},
        roots,
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
        whole_set: bool,
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
        if whole_set {
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
                facts::unit(fixture, &names, typed.types(), &owned, whole_set, report);
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
                roots::collect(
                    fixture,
                    &names,
                    typed.types(),
                    entry.id(),
                    report,
                    |root, _| crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, root),
                )
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
                facts::unit(
                    fixture,
                    &names,
                    typed.types(),
                    owned.ownership(),
                    whole_set,
                    report,
                );
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
                roots::collect(
                    fixture,
                    &names,
                    typed.types(),
                    entry.id(),
                    report,
                    |root, _| {
                        crate::ssa::unit_lower::constant::lower_const_owned_unit_with_entry(&view, root)
                    },
                )
            }
        }
    }
}

use fixture::{Fixture, SourceCase};
use lang_frontend::type_checking::{IntrinsicTypeConstructor, MapIterationKind};

const MAP: SourceCase = SourceCase {
    path: "map.ko",
    text: "fun scan(source: Map<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
    constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
};
const MUTABLE: SourceCase = SourceCase {
    path: "mutable.ko",
    text: "fun scan(source: MutableMap<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
    constructor: Some((
        IntrinsicTypeConstructor::MutableMap,
        MapIterationKind::MutableMap,
    )),
};
const WHOLE: [SourceCase; 3] = [
    SourceCase {
        path: "entry.ko",
        text: "fun entry(): Unit {}\n",
        constructor: None,
    },
    SourceCase {
        path: "z_map.ko",
        text: "fun unusedMap(source: Map<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
        constructor: MAP.constructor,
    },
    SourceCase {
        path: "a_mutable.ko",
        text: "fun unusedMutable(source: MutableMap<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
        constructor: MUTABLE.constructor,
    },
];

#[derive(Clone, Copy, Debug)]
enum Pathway {
    Single,
    Basic,
    Const,
}

#[derive(Debug, Default)]
struct Report {
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}

impl Report {
    fn check(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed));
        self.details.push(format!("{name}: {detail:?}"));
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed)| *passed)
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    report: Report,
    result: Result<fixture::LowerOutcome, fixture::Stopped>,
}

impl Observation {
    fn passed(&self) -> bool {
        self.report.passed()
            && self
                .result
                .as_ref()
                .is_ok_and(fixture::LowerOutcome::matches)
    }
}

fn observe(
    label: &str,
    cases: &[SourceCase],
    pathway: Pathway,
    reverse_inputs: bool,
    entry_name: &str,
    whole_set: bool,
) -> Observation {
    let mut report = Report::default();
    let result = Fixture::new(cases).and_then(|fixture| {
        unit::collect(
            &fixture,
            pathway,
            reverse_inputs,
            entry_name,
            whole_set,
            &mut report,
        )
    });
    Observation {
        label: format!("{label}/{pathway:?}/reverse={reverse_inputs}"),
        report,
        result,
    }
}

fn assert_observations(observations: &[Observation], expected: usize) {
    // 每条真实阶段链已结束，才输出和断言；首个失败不得遮住其他观察。
    for observation in observations {
        diagnostic!(
            "{}\n{:#?}\n{:#?}",
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
    assert_eq!(observations.len(), expected);
    assert_diagnostic!(
        observations.iter().all(Observation::passed),
        "Map 和 MutableMap Borrow 必须完整 frontend 成功；\
         二者必须返回真实入口并通过 SSA 与完整 LLVM；\
         前置失败保留原 stage，代表 LOWER_NOT_ATTEMPTED，不能冒充能力结果。\n{observations:#?}",
    );
}

#[test]
fn map_and_mutable_map_borrow_normal_verify_in_three_pathways() {
    let observations = [MAP, MUTABLE]
        .into_iter()
        .flat_map(|case| {
            [Pathway::Single, Pathway::Basic, Pathway::Const]
                .map(|pathway| observe(case.path, &[case], pathway, false, "scan", false))
        })
        .collect::<Vec<_>>();
    assert_observations(&observations, 6);
}

#[test]
fn whole_typed_source_set_verifies_unused_borrow_map_loops() {
    let mut observations = Vec::new();
    // 单文件仍检查全部同轮 facts；entry 为空，未调用函数由同一 view 逐根验证。
    let text = WHOLE.iter().map(|case| case.text).collect::<String>();
    let mut report = Report::default();
    let result = Fixture::single_whole(&text).and_then(|fixture| {
        unit::collect(&fixture, Pathway::Single, false, "entry", true, &mut report)
    });
    observations.push(Observation {
        label: "whole-file/Single".to_owned(),
        report,
        result,
    });
    for pathway in [Pathway::Basic, Pathway::Const] {
        for reverse_inputs in [false, true] {
            observations.push(observe(
                "whole-source-set",
                &WHOLE,
                pathway,
                reverse_inputs,
                "entry",
                true,
            ));
        }
    }
    assert_observations(&observations, 5);
}
