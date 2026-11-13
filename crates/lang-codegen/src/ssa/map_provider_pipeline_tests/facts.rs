//! 只读实际 For/source/K/V/双 Shared 绑定与来源模式；不构造 facts，不决定 lower 结果。
use super::{
    Report,
    fixture::{FOR_TEXT, Fixture, LoopSite, loop_site},
};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
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
        fixture.sources.slice(site.span) == Ok(FOR_TEXT)
            && fixture.sources.slice(site.source_span) == Ok("source")
            && fixture.sources.slice(site.bindings[0]) == Ok("key")
            && fixture.sources.slice(site.bindings[1]) == Ok("value")
            && fixture.sources.source_name(site.span.source_id()) == Ok(fixture.path)
            && site.span.source_id() == fixture.parsed.source_id(),
        &site,
    );
    Some(site)
}

pub(super) fn unit(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
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
    for name in ["scan", "read"] {
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
    if report.identity_requested {
        report.fe_identity = Some(super::query_inputs::unit(fixture, names, typed, owned));
    }
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
    diagnostic!(
        "SPEC0331 PROVIDER REAL UNIT OWNER source={source:?} plan={plan:#?} source_drops={drops:#?}"
    );
}

pub(in crate::ssa) mod query_inputs {
    //! 同次 FE 公开 facts 到 SSA source origin 的只读桥；不调用生产 qualification。
    use super::super::{
        fixture::{Failure, Fixture, failure, loop_site},
        identity,
    };
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, OwnershipBindingKind, UnitDropPoint,
            UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind,
            UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, Item, NameMarker, Statement},
        source::Span,
        type_checking::{
            CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
        },
    };

    pub(in crate::ssa) fn need(
        ok: bool,
        stage: &'static str,
        detail: impl std::fmt::Debug,
    ) -> Result<(), Failure> {
        if ok {
            Ok(())
        } else {
            Err(failure(stage, detail))
        }
    }

    pub(in crate::ssa) fn one<T>(
        mut values: impl Iterator<Item = T>,
        stage: &'static str,
    ) -> Result<T, Failure> {
        let result = values
            .next()
            .ok_or_else(|| failure(stage, "missing unique entity"))?;
        need(values.next().is_none(), stage, "multiple matching entities")?;
        Ok(result)
    }

    #[derive(Clone, Debug)]
    pub(in crate::ssa) struct Inputs {
        pub scan: Span,
        pub read: Span,
        pub loop_span: Span,
        pub parameter: Span,
        pub parameter_slot: usize,
        pub call: Span,
        pub drop_origin: Option<Span>,
        pub owned: bool,
    }

    fn function_span(f: &Fixture, name: Span) -> Result<Span, Failure> {
        one(f.parsed.roots().iter().filter_map(|root| {
        let node = f.parsed.ast().items().get(*root).ok()?;
        matches!(node.payload(), Item::Function { name: NameMarker::Present(actual), .. } if *actual == name)
            .then_some(node.span())
    }), "FE exact function root")
    }

    fn call(f: &Fixture, outer: Span) -> Result<(ExpressionId, Span, [Span; 2]), Failure> {
        let (id, node) = one(
            f.parsed.ast().expressions().iter().filter(|(_, node)| {
                let s = node.span();
                s.source_id() == outer.source_id()
                    && outer.start() <= s.start()
                    && s.end() <= outer.end()
                    && matches!(node.payload(), Expression::Call { .. })
            }),
            "FE body actual call",
        )?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(failure("FE actual call shape", node));
        };
        need(arguments.len() == 2, "FE body arguments", arguments)?;
        let mut spans = Vec::new();
        for argument in arguments {
            let node = f
                .parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|e| failure("FE call argument AST", e))?;
            need(
                matches!(node.payload(), Expression::Name),
                "FE component argument is Name",
                node,
            )?;
            spans.push(node.span());
        }
        Ok((id, node.span(), [spans[0], spans[1]]))
    }

    pub(in crate::ssa) fn unit(
        f: &Fixture,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<Inputs, Failure> {
        let site = loop_site(f)?;
        let local = one(names.names().source_units().iter(), "FE unit source")?;
        let unit = local.source_unit();
        let statement = UnitStatementId::new(unit, site.statement);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("FE unit descriptor", statement))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("FE unit plan", statement))?;
        let Some(UnitLoanTarget::Place(path)) = p.source() else {
            return Err(failure("FE unit source root", p));
        };
        let source = path.root();
        need(
            source.source_unit() == unit,
            "FE parameter source-unit identity",
            source,
        )?;
        let symbol = one(
            local
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.id() == source.symbol()),
            "FE unit parameter symbol",
        )?;
        let (signature, slot) = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|d| d.callable())
                .flat_map(|c| {
                    c.parameters()
                        .iter()
                        .enumerate()
                        .filter_map(move |(i, p)| (p.symbol() == Some(source)).then_some((c, i)))
                }),
            "FE unit source actual parameter slot",
        )?;
        let parameter = signature
            .parameters()
            .get(slot)
            .ok_or_else(|| failure("FE unit original parameter slot", slot))?;
        let source_references = names
            .names()
            .references()
            .iter()
            .filter(|reference| reference.span() == site.source_span)
            .collect::<Vec<_>>();
        need(
            signature.receiver().is_none()
                && signature.parameters().len() == 1
                && path.is_root()
                && symbol.kind() == SymbolKind::ValueParameter
                && !symbol.is_synthetic()
                && symbol.span().source_id() == f.parsed.source_id()
                && parameter.symbol() == Some(source)
                && parameter.ty() == d.source_type()
                && typed.symbol_type(source) == Some(d.source_type())
                && p.descriptor() == d
                && d.source() == UnitExpressionId::new(unit, site.source)
                && matches!(p.source_kind(), IterationSource::BorrowedSource(origin)
                if Some(origin) == p.source())
                && matches!(source_references.as_slice(), [reference]
                if reference.source_unit() == unit
                    && reference.target() == &UnitReferenceTarget::Symbol(source))
                && parameter.mode()
                    == if f.map {
                        ParameterMode::Value
                    } else {
                        ParameterMode::Borrow
                    }
                && owned.binding_kind(source)
                    == Some(if f.map {
                        OwnershipBindingKind::Owned
                    } else {
                        OwnershipBindingKind::Shared
                    })
                && p.source_access()
                    == if f.map {
                        UnitIterationSourceAccess::Owned
                    } else {
                        UnitIterationSourceAccess::Shared
                    },
            "FE unit real parameter seed and source capability",
            (signature, parameter, p, source_references),
        )?;
        let (call_id, call_span, argument_spans) = call(f, site.span)?;
        let c = typed
            .call(UnitExpressionId::new(unit, call_id))
            .ok_or_else(|| failure("FE unit body call", call_id))?;
        let UnitCallTarget::Declaration(read) = c.target() else {
            return Err(failure("FE unit read target", c));
        };
        let declaration = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.id() == read),
            "FE unit read declaration",
        )?;
        need(
            declaration.source_unit() == unit && c.arguments().len() == 2 && c.receiver().is_none(),
            "FE unit read argument map",
            c,
        )?;
        for (i, argument) in c.arguments().iter().enumerate() {
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == argument_spans[i])
                .collect::<Vec<_>>();
            need(
                matches!(refs.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(d.bindings()[i]))
                    && argument.argument_index() == i
                    && argument.parameter_index() == i
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == [d.key_type(), d.value_type()][i],
                "FE unit read binding/type/Borrow",
                (argument, refs),
            )?;
        }
        let Statement::For { body, .. } = f
            .parsed
            .ast()
            .statements()
            .get(site.statement)
            .map_err(|e| failure("FE unit For body", e))?
            .payload()
        else {
            return Err(failure("FE actual For shape", site.statement));
        };
        need(p.exits().len() == 2, "FE unit only normal exits", p.exits())?;
        let fall = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == UnitIterationExitKind::Fallthrough),
            "FE unit fallthrough",
        )?;
        let end = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == UnitIterationExitKind::Exhaustion),
            "FE unit exhaustion",
        )?;
        let expected = [
            UA::EndBinding {
                statement,
                symbol: d.value_binding(),
            },
            UA::EndBinding {
                statement,
                symbol: d.key_binding(),
            },
        ];
        need(
            fall.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(unit, *body))
                && fall.actions() == expected
                && end.point() == UnitDropPoint::LoopExit(statement)
                && owned.iteration_cleanup_at(fall.point()) == Some(fall.actions())
                && owned.iteration_cleanup_at(end.point()) == Some(end.actions()),
            "FE unit actual cleanup points/order",
            (fall, end),
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(source))
            .collect::<Vec<_>>();
        // Unit 没有旧 FE closure-phi/selector 表。这里只读取真实 source/drop 事实；
        // 旧 seed/incoming/availability/retirement 义务由 query_owner 的完整 SSA 合同重建。
        let drop_origin = if f.map {
            let drop = *one(drops.iter().copied(), "FE unit source drop")?;
            need(
                drop.point() == end.point()
                    && drop.value_origin() == symbol.span()
                    && end.actions()
                        == [
                            UA::FinishProvider(statement),
                            UA::EndSource(statement),
                            UA::Drop(drop),
                        ],
                "FE unit actual source drop action",
                drop,
            )?;
            Some(drop.value_origin())
        } else {
            need(
                drops.is_empty()
                    && end.actions() == [UA::FinishProvider(statement), UA::EndSource(statement)],
                "FE unit borrowed source no drop",
                (drops, end),
            )?;
            None
        };
        finish(
            f,
            Inputs {
                scan: function_span(f, signature.name_span())?,
                read: function_span(f, declaration.name_span())?,
                loop_span: site.span,
                parameter: symbol.span(),
                parameter_slot: slot,
                call: call_span,
                drop_origin,
                owned: f.map,
            },
        )
    }

    fn finish(f: &Fixture, inputs: Inputs) -> Result<Inputs, Failure> {
        let functions = identity::source_functions(f)?;
        need(
            functions.contains(&("scan", inputs.scan))
                && functions.contains(&("read", inputs.read)),
            "FE actual symbols map to original scan/read roots",
            &inputs,
        )?;
        diagnostic!("SPEC0331 PROVIDER IDENTITY FE {inputs:#?}");
        Ok(inputs)
    }
}

pub(in crate::ssa) mod query_owner {
    //! 真实 Unit source 的非可选所有权运输与退役；不读取旧 FE 图或生产 carrier。
    //! 身份从参数、provider operands 和每条有向 edge 建立，不做同类型或双向 union 归根。
    use super::super::{
        fixture::{Failure, failure},
        query_inputs::{Inputs, need, one},
        query_ssa::{Shape, instructions},
    };
    use crate::ssa::model::*;
    use std::collections::BTreeSet;

    fn carried(f: &Function, edge: &Edge, source: EntityId) -> Result<EntityId, Failure> {
        let column = one(
            edge.arguments
                .iter()
                .enumerate()
                .filter_map(|(index, &argument)| (argument == source).then_some(index)),
            "SSA owner contract unique directed transport",
        )?;
        f.block(edge.target)
            .and_then(|block| block.parameters.get(column))
            .copied()
            .ok_or_else(|| failure("SSA owner contract actual target column", (edge, source)))
    }

    fn edge_columns(
        f: &Function,
        edge: &Edge,
        expected: &[(EntityId, EntityId); 3],
    ) -> Result<(), Failure> {
        let target = f
            .block(edge.target)
            .ok_or_else(|| failure("SSA owner contract target", edge))?;
        need(
            edge.arguments.len() == expected.len()
                && target.parameters.len() == expected.len()
                && expected
                    .iter()
                    .map(|(source, _)| source)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 3
                && expected
                    .iter()
                    .map(|(_, target)| target)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 3,
            "SSA owner/source/cursor are the complete three distinct columns",
            (edge, expected),
        )?;
        for (index, (&source, &parameter)) in
            edge.arguments.iter().zip(&target.parameters).enumerate()
        {
            let source_data = f
                .entity(source)
                .ok_or_else(|| failure("SSA owner contract source entity", source))?;
            let target_data = f
                .entity(parameter)
                .ok_or_else(|| failure("SSA owner contract target entity", parameter))?;
            need(
                expected.contains(&(source, parameter))
                    && source != parameter
                    && source_data.ty == target_data.ty
                    && target_data.definition
                        == Definition::BlockParameter {
                            block: target.id,
                            index,
                        },
                "SSA every actual column has its exact source and definition",
                (edge, index, source, parameter),
            )?;
        }
        Ok(())
    }

    // Close the real definition inventory as well as the reachable graph: an unreferenced
    // block, instruction or map-typed entity cannot hide an alternate owner/loan definition.
    fn definitions(f: &Function, shape: &Shape) -> Result<BTreeSet<EntityId>, Failure> {
        let blocks = shape.blocks.into_iter().collect::<BTreeSet<_>>();
        need(
            f.blocks.len() == blocks.len()
                && f.blocks.iter().enumerate().all(|(index, block)| {
                    blocks.contains(&block.id)
                        && block.id.function() == f.id
                        && block.id.index() == index
                }),
            "SSA owner contract includes every actual block",
            &f.blocks,
        )?;
        let all = instructions(f, &shape.blocks)?;
        need(
            all.len() == f.instructions.len()
                && f.instructions
                    .iter()
                    .enumerate()
                    .all(|(index, instruction)| {
                        instruction.id.function() == f.id && instruction.id.index() == index
                    }),
            "SSA owner contract includes every actual instruction",
            &all,
        )?;
        let mut entities = BTreeSet::new();
        for block in &f.blocks {
            for (index, &entity) in block.parameters.iter().enumerate() {
                need(
                    entities.insert(entity)
                        && f.entity(entity).is_some_and(|data| {
                            data.definition
                                == Definition::BlockParameter {
                                    block: block.id,
                                    index,
                                }
                        }),
                    "SSA owner contract exact parameter definition",
                    (block.id, index, entity),
                )?;
            }
        }
        for instruction in all {
            for (index, &entity) in instruction.results.iter().enumerate() {
                need(
                    entities.insert(entity)
                        && f.entity(entity).is_some_and(|data| {
                            data.definition
                                == Definition::InstructionResult {
                                    instruction: instruction.id,
                                    index,
                                }
                        }),
                    "SSA owner contract exact instruction result definition",
                    (instruction, index, entity),
                )?;
            }
        }
        need(
            entities.len() == f.values.len() + f.places.len() + f.loans.len(),
            "SSA owner contract has no orphan entity definitions",
            &entities,
        )?;
        Ok(entities)
    }

    pub(in crate::ssa) fn check(
        module: &Module,
        f: &Function,
        input: &Inputs,
        shape: &Shape,
    ) -> Result<(), Failure> {
        let entities = definitions(f, shape)?;
        let [pre, header, body, exit] = shape.blocks;
        let edge = |from, to| {
            one(
                shape.edges.iter().filter_map(|(actual, edge)| {
                    (*actual == from && edge.target == to).then_some(edge)
                }),
                "SSA owner contract unique real predecessor",
            )
        };
        let entry = edge(pre, header)?;
        let taken = edge(header, body)?;
        let back = edge(body, header)?;
        let exhausted = edge(header, exit)?;
        let header_owner = carried(f, entry, shape.seed)?;
        let body_owner = carried(f, taken, header_owner)?;
        let exit_owner = carried(f, exhausted, header_owner)?;
        let exit_source = carried(f, exhausted, EntityId::Loan(shape.header_source))?;
        let exit_cursor = carried(f, exhausted, EntityId::Value(shape.header_cursor))?;
        let output = |id| -> Result<EntityId, Failure> {
            let instruction = f
                .instruction(id)
                .ok_or_else(|| failure("SSA owner contract provider instruction", id))?;
            one(
                instruction.results.iter().copied(),
                "SSA owner contract provider result",
            )
        };
        let first_cursor = output(shape.operations[0])?;
        let next_cursor = output(shape.operations[4])?;
        // This is the complete incoming relation, not an existence or alias-set assertion.
        // Both header predecessors must carry the same source lineage; the body cannot
        // replace it with another same-typed owner or add a second availability column.
        edge_columns(
            f,
            entry,
            &[
                (shape.seed, header_owner),
                (
                    EntityId::Loan(shape.lease),
                    EntityId::Loan(shape.header_source),
                ),
                (first_cursor, EntityId::Value(shape.header_cursor)),
            ],
        )?;
        edge_columns(
            f,
            taken,
            &[
                (header_owner, body_owner),
                (
                    EntityId::Loan(shape.header_source),
                    EntityId::Loan(shape.body_source),
                ),
                (
                    EntityId::Value(shape.header_cursor),
                    EntityId::Value(shape.body_cursor),
                ),
            ],
        )?;
        edge_columns(
            f,
            back,
            &[
                (body_owner, header_owner),
                (
                    EntityId::Loan(shape.body_source),
                    EntityId::Loan(shape.header_source),
                ),
                (next_cursor, EntityId::Value(shape.header_cursor)),
            ],
        )?;
        edge_columns(
            f,
            exhausted,
            &[
                (header_owner, exit_owner),
                (EntityId::Loan(shape.header_source), exit_source),
                (EntityId::Value(shape.header_cursor), exit_cursor),
            ],
        )?;
        let source_chain = [shape.seed, header_owner, body_owner, exit_owner];
        let seed_type = f
            .entity(shape.seed)
            .ok_or_else(|| failure("SSA owner contract actual parameter", shape.seed))?
            .ty;
        let map_type = seed_type.semantic_type();
        need(
            source_chain.into_iter().collect::<BTreeSet<_>>().len() == 4
                && module.type_ownership(map_type) == Some(Ownership::MoveOnly)
                && source_chain.iter().all(|&entity| {
                    matches!(
                        (entity, input.owned),
                        (EntityId::Value(_), true) | (EntityId::Loan(_), false)
                    ) && f.entity(entity).is_some_and(|data| data.ty == seed_type)
                })
                && seed_type
                    == if input.owned {
                        EntityType::Value(map_type)
                    } else {
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map_type,
                        }
                    },
            "SSA nonoptional owner or caller Borrow survives every real incoming",
            source_chain,
        )?;
        for (&entity, span) in source_chain.iter().zip([
            input.scan,
            input.loop_span,
            input.loop_span,
            input.loop_span,
        ]) {
            need(
                f.entity(entity)
                    .is_some_and(|data| data.origin == Origin::Source(span)),
                "SSA source seed and carrier origins match the original function/For",
                (entity, span),
            )?;
        }
        let all = instructions(f, &shape.blocks)?;
        let mut expected_maps = source_chain.into_iter().collect::<BTreeSet<_>>();
        for entity in [
            EntityId::Loan(shape.lease),
            EntityId::Loan(shape.header_source),
            EntityId::Loan(shape.body_source),
            exit_source,
        ] {
            need(
                expected_maps.insert(entity),
                "SSA caller/owner and provider lease stay distinct",
                entity,
            )?;
        }
        if input.owned {
            let root = one(all.iter().copied().filter(|instruction|
            matches!(instruction.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == shape.seed)),
            "SSA owner contract exact parameter RootPlace")?;
            let place = one(
                root.results.iter().copied(),
                "SSA owner contract unique root place",
            )?;
            need(
                matches!(place, EntityId::Place(_))
                    && f.entity(place)
                        .is_some_and(|data| data.ty == EntityType::Place(map_type))
                    && expected_maps.insert(place),
                "SSA owner contract actual source storage identity",
                root,
            )?;
        }
        let actual_maps = entities
            .into_iter()
            .filter(|&entity| {
                f.entity(entity)
                    .is_some_and(|data| data.ty.semantic_type() == map_type)
            })
            .collect::<BTreeSet<_>>();
        need(
            actual_maps == expected_maps,
            "SSA no extra map owner/loan/place definition can hide replacement or copying",
            (actual_maps, expected_maps),
        )?;
        let drops = all.iter().copied().filter(|instruction|
        matches!(instruction.operation, Operation::Drop { owner } if source_chain.contains(&EntityId::Value(owner))))
        .collect::<Vec<_>>();
        if input.owned {
            let drop = one(
                drops.into_iter(),
                "SSA owner contract unique exhaustion Drop",
            )?;
            need(
                matches!(drop.operation, Operation::Drop { owner } if EntityId::Value(owner) == exit_owner)
                    && drop.block == exit
                    && drop.origin.span() == input.parameter
                    && input.drop_origin == Some(input.parameter),
                "SSA availability is discharged by the exact Unit source Drop",
                drop,
            )?;
        } else {
            need(
                drops.is_empty() && input.drop_origin.is_none(),
                "SSA caller Borrow has no Drop",
                drops,
            )?;
        }
        // Existing query_ssa additionally rejects every source-chain use except the
        // exact RootPlace/Drop (owned) or seed SharedReborrow (borrowed), and enforces
        // EndSource immediately before Drop. Thus no use can revive the exhausted owner.
        for (block, expected) in [(pre, entry), (body, back)] {
            need(
                matches!(f.block(block).and_then(|block| block.terminator.as_ref()),
                Some(Terminator { kind: TerminatorKind::Branch(actual), .. }) if actual == expected),
                "SSA owner entry/backedge are unconditional exact transports",
                block,
            )?;
        }
        need(
            matches!(f.block(exit).and_then(|block| block.terminator.as_ref()),
            Some(Terminator { kind: TerminatorKind::Return { values }, .. }) if values.is_empty())
                && !shape.edges.iter().any(|(from, _)| *from == exit)
                && f.return_types.is_empty()
                && f.borrow_return.is_none()
                && f.carrier_return.is_none(),
            "SSA exhausted owner cannot escape by return or regain availability on an outgoing edge",
            f.block(exit),
        )
    }
}
