//! Source mode and transparent Group must preserve the original provider program.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(super) mod facts {
    //! Independent observations of the original outer syntax and canonical public owner.
    use super::*;
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{CompilationUnitOwnership, UnitDropTarget, UnitLoanTarget},
        parser::{Expression, ParsedFile},
        type_checking::{CompilationUnitTypes, ParameterMode, UnitExpressionId},
    };

    // Deliberately inspect the source AST, without the production transparent-source helper.
    fn chain(parsed: &ParsedFile, outer: ExpressionId) -> Vec<ExpressionId> {
        let mut ids = vec![outer];
        while let Expression::Group { expression } = parsed
            .ast()
            .expressions()
            .get(*ids.last().unwrap())
            .unwrap()
            .payload()
        {
            assert!(!ids.contains(expression));
            ids.push(*expression);
        }
        ids
    }

    pub(in super::super) fn unit(
        _sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) {
        for d in typed.map_iterations() {
            let plan = owned.map_iteration(d.statement());
            report.check(
                "unit original qualified descriptor",
                plan.is_some_and(|p| p.descriptor() == d),
                d,
            );
            let Some(plan) = plan else {
                continue;
            };
            let ids = chain(parsed, d.source().expression());
            let leaf = UnitExpressionId::new(d.source().source_unit(), *ids.last().unwrap());
            let node = parsed.ast().expressions().get(leaf.expression()).unwrap();
            report.check("unit outer AST and entire Group type/category chain", matches!(parsed.ast().statements().get(d.statement().statement()).unwrap().payload(), Statement::For { source, .. } if *source == d.source().expression()) && ids.iter().all(|&id| { let id=UnitExpressionId::new(d.source().source_unit(), id); typed.expression_type(id) == Some(d.source_type()) && typed.expression_category(id) == typed.expression_category(d.source()) }), &ids);
            report.check(
                "unit outer loan sources and canonical origins",
                plan.loans().iter().all(|loan| {
                    loan.statement() == d.statement()
                        && loan.source() == d.source()
                        && Some(loan.origin()) == plan.source()
                }),
                plan.loans(),
            );
            match plan.source().unwrap() {
                UnitLoanTarget::Temporary(owner) => {
                    let drops = owned
                        .drops()
                        .iter()
                        .filter(|f| f.target() == UnitDropTarget::Temporary(leaf))
                        .collect::<Vec<_>>();
                    report.check(
                        "unit canonical Call owns original temporary Drops",
                        *owner == leaf
                            && matches!(node.payload(), Expression::Call { .. })
                            && !drops.is_empty()
                            && drops.iter().all(|f| f.value_origin() == node.span()),
                        &drops,
                    );
                }
                UnitLoanTarget::Place(place) => {
                    let references = names
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == node.span())
                        .collect::<Vec<_>>();
                    report.check("unit canonical Name resolves to original qualified place root", matches!(node.payload(), Expression::Name) && matches!(references.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(place.root())), &references);
                    let symbol = names
                        .names()
                        .source_units()
                        .iter()
                        .find(|s| s.source_unit() == place.root().source_unit())
                        .and_then(|s| s.resolution().symbols().get(place.root().symbol().index()))
                        .unwrap();
                    if symbol.kind() == SymbolKind::ValueParameter {
                        let parameters = typed
                            .signatures()
                            .declarations()
                            .iter()
                            .filter_map(|s| s.callable())
                            .flat_map(|c| c.parameters())
                            .filter(|p| p.symbol() == Some(place.root()))
                            .collect::<Vec<_>>();
                        report.check("unit parameter mode agrees with owned/shared public access", matches!(parameters.as_slice(), [p] if matches!((p.mode(),plan.source_access()), (ParameterMode::Value,lang_frontend::ownership_checking::UnitIterationSourceAccess::Owned) | (ParameterMode::Borrow,lang_frontend::ownership_checking::UnitIterationSourceAccess::Shared))), parameters);
                    }
                }
                other => report.check("unit source retains existing representation", false, other),
            }
        }
    }
}
pub(super) mod graph {
    //! The new owned MutableMap rows must root and drop the original parameter on every exit.
    mod entries {
        //! The original outer source is evaluated before the actual temporary provider entry.
        use crate::ssa::{
            map_provider_exits_pipeline_tests::{
                fixture::{Failure, failure, need},
                query_graph::{Graph, result},
                query_inputs::one,
            },
            model::*,
        };
        use lang_frontend::parser::{Expression, ParsedFile, Statement};

        pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
            parsed: &ParsedFile,
            program: &Program,
        ) -> Result<(), Failure> {
            for (_, statement) in parsed.ast().statements().iter() {
                let Statement::For { source, .. } = statement.payload() else {
                    continue;
                };
                let outer = parsed
                    .ast()
                    .expressions()
                    .get(*source)
                    .map_err(|e| failure("temporary Entry original outer source", e))?;
                let mut leaf = outer;
                while let Expression::Group { expression } = leaf.payload() {
                    leaf = parsed
                        .ast()
                        .expressions()
                        .get(*expression)
                        .map_err(|e| failure("temporary Entry complete original Group chain", e))?;
                }
                if !matches!(leaf.payload(), Expression::Call { .. }) {
                    continue;
                }
                need(
                    outer.span().source_id() == parsed.source_id()
                        && leaf.span().source_id() == outer.span().source_id()
                        && outer.span().start() <= leaf.span().start()
                        && leaf.span().end() <= outer.span().end()
                        && outer.span().end() < statement.span().end(),
                    "temporary Entry retains exact outer AfterExpression and canonical Call spans",
                    (outer.span(), leaf.span(), statement.span()),
                )?;
                let mut selected = 0;
                for module in &program.modules {
                    for function in &module.functions {
                        if !function.instructions.iter().any(|op| {
                            op.origin == Origin::Source(statement.span())
                                && matches!(op.operation, Operation::MapIterationFirst { .. })
                        }) {
                            continue;
                        }
                        selected += 1;
                        let graph = Graph::with_abort(function)?;
                        let first = one(
                            graph.instructions.iter().copied().filter(|op| {
                                op.origin == Origin::Source(statement.span())
                                    && matches!(op.operation, Operation::MapIterationFirst { .. })
                            }),
                            "temporary Entry unique actual original First",
                        )?;
                        let producer = one(
                            graph.instructions.iter().copied().filter(|op| {
                                op.origin == Origin::Source(leaf.span())
                                    && matches!(op.results.as_slice(), [EntityId::Value(value)]
                                    if function.entity(EntityId::Value(*value)).is_some_and(|v|
                                        module.map_container(v.ty.semantic_type()).is_some()))
                            }),
                            "temporary Entry exact canonical Call owner producer",
                        )?;
                        need(
                            matches!(
                                producer.operation,
                                Operation::DirectCall { .. }
                                    | Operation::CallableInvoke { .. }
                                    | Operation::MapConstruct { .. }
                                    | Operation::MapConsume { .. }
                            ),
                            "temporary Entry is the original call/intrinsic result, never a substitute",
                            producer,
                        )?;
                        let Operation::MapIterationFirst { source: lease } = first.operation else {
                            unreachable!()
                        };
                        let begin = graph.definition(EntityId::Loan(lease))?;
                        let Operation::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        } = begin.operation
                        else {
                            return Err(failure("temporary Entry actual Shared source lease", begin));
                        };
                        let root = graph.definition(EntityId::Place(place))?;
                        let Operation::RootPlace { owner } = root.operation else {
                            return Err(failure("temporary Entry actual original owner root", root));
                        };
                        graph.anchored(EntityId::Value(owner), &[result(producer)?])?;
                        need(
                            producer.block == root.block
                                && root.block == begin.block
                                && begin.block == first.block
                                && graph.position(producer)? < graph.position(root)?
                                && graph.position(root)? < graph.position(begin)?
                                && graph.position(begin)? < graph.position(first)?
                                && root.origin == Origin::Source(statement.span())
                                && begin.origin == Origin::Source(statement.span()),
                            "temporary Entry follows complete outer source evaluation before acquisition",
                            (outer.span(), producer, root, begin, first),
                        )?;
                        let edge = graph.branch(first.block)?;
                        let header_owner = graph.transported(edge, EntityId::Value(owner))?;
                        let header_lease = graph.transported(edge, EntityId::Loan(lease))?;
                        let header_cursor = graph.transported(edge, result(first)?)?;
                        graph.anchored(header_owner, &[result(producer)?])?;
                        let has = one(
                            graph.ops(edge.target)?.into_iter().filter(|op| {
                                op.origin == Origin::Source(statement.span())
                                    && matches!(op.operation, Operation::MapIterationHasNext { .. })
                            }),
                            "temporary Entry actual header HasNext",
                        )?;
                        need(
                            matches!(has.operation, Operation::MapIterationHasNext { source, cursor }
                                if EntityId::Loan(source) == header_lease
                                    && EntityId::Value(cursor) == header_cursor)
                                && edge
                                    .arguments
                                    .iter()
                                    .filter(|&&id| id == EntityId::Value(owner))
                                    .count()
                                    == 1
                                && edge
                                    .arguments
                                    .iter()
                                    .filter(|&&id| id == EntityId::Loan(lease))
                                    .count()
                                    == 1,
                            "temporary Entry exact owner/lease/cursor columns and all owner predecessors",
                            (edge, header_owner, has),
                        )?;
                    }
                }
                need(
                    selected > 0,
                    "temporary Entry original source has an actual selected provider",
                    statement.span(),
                )?;
            }
            Ok(())
        }
    }
    use super::super::{
        fixture::{failure, need},
        query_graph::Graph,
    };
    use super::*;
    use crate::ssa::model::*;
    pub(in super::super) use entries::check as temporary_entries;
    use lang_frontend::parser::{Item, NameMarker};
    use std::collections::{BTreeMap, BTreeSet};

    pub(in super::super) fn owned_map(
        sources: &SourceMap,
        parsed: &lang_frontend::parser::ParsedFile,
        program: &Program,
        mutable: bool,
    ) -> Result<(), Failure> {
        let scan = parsed
            .ast()
            .items()
            .iter()
            .find_map(|(_, n)| match n.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } if sources.slice(*name) == Ok("scan") => Some(n.span()),
                _ => None,
            });
        let scan = scan.ok_or_else(|| failure("original owned scan AST", parsed.source_id()))?;
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.origin == Origin::Source(scan))
            .ok_or_else(|| failure("original scan", scan))?;
        let &anchor = function
            .block(function.entry_block().unwrap())
            .unwrap()
            .parameters
            .first()
            .ok_or_else(|| failure("actual owned source parameter", function.id))?;
        let ty = function.entity(anchor).unwrap().ty;
        let module = program.module(function.id.module()).unwrap();
        need(
            matches!(ty,EntityType::Value(t) if matches!(module.type_kind(t),Some(SsaTypeKind::MapContainer {kind,..}) if *kind==if mutable{MapContainerKind::MutableMap}else{MapContainerKind::Map})),
            "actual owned Map parameter ABI",
            ty,
        )?;
        let graph = Graph::with_abort(function)?;
        let roots = graph
            .instructions
            .iter()
            .filter(
                |i| matches!(i.operation,Operation::RootPlace{owner} if EntityId::Value(owner)==anchor),
            )
            .collect::<Vec<_>>();
        need(
            roots.len() == 1,
            "owned Map original parameter root",
            &roots,
        )?;
        let first = super::super::query_inputs::one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. })),
            "actual provider First",
        )?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let birth = graph.definition(EntityId::Loan(source))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("provider original source acquisition", birth));
        };
        let root = graph.definition(EntityId::Place(place))?;
        need(
            roots[0].id == root.id,
            "actual provider First roots the original parameter",
            (root, first),
        )?;
        let mut drops = BTreeSet::new();
        for i in &graph.instructions {
            if let Operation::Drop { owner } = i.operation
                && function.entity(EntityId::Value(owner)).unwrap().ty == ty
            {
                transport(&graph, EntityId::Value(owner), anchor)?;
                drops.insert(i.id);
            }
        }
        need(!drops.is_empty(), "owned Map real Drop", &drops)?;
        // A cycle may execute arbitrarily often; source Drop must be outside every cycle.
        for instruction in graph.instructions.iter().filter(|i| drops.contains(&i.id)) {
            let block = instruction.block;
            let mut pending = graph
                .edges
                .iter()
                .filter(|(from, _)| *from == block)
                .map(|(_, e)| e.target)
                .collect::<Vec<_>>();
            let mut seen = BTreeSet::new();
            while let Some(next) = pending.pop() {
                need(
                    next != block,
                    "source Drop is outside all CFG cycles",
                    instruction,
                )?;
                if seen.insert(next) {
                    pending.extend(
                        graph
                            .edges
                            .iter()
                            .filter(|(from, _)| *from == next)
                            .map(|(_, e)| e.target),
                    );
                }
            }
        }

        let mut pending = vec![(function.entry_block().unwrap(), 0, BTreeSet::new())];
        let mut returns = 0;
        while let Some((block, count, mut visited)) = pending.pop() {
            if !visited.insert(block) {
                continue;
            }
            let count = count
                + graph
                    .ops(block)?
                    .iter()
                    .filter(|i| drops.contains(&i.id))
                    .count();
            if matches!(graph.term(block)?.kind, TerminatorKind::Return { .. }) {
                returns += 1;
                need(
                    count == 1,
                    "every normal return drops original Map once",
                    (block, count),
                )?;
            }
            for (_, edge) in graph.edges.iter().filter(|(from, _)| *from == block) {
                pending.push((edge.target, count, visited.clone()));
            }
        }
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &graph,
            module,
            &BTreeSet::from([anchor]),
        )?;
        let owners = edge_family(&graph, anchor)?;
        let leases = edge_family(&graph, EntityId::Loan(source))?;
        let mut expected = owners
            .iter()
            .chain(&leases)
            .copied()
            .collect::<BTreeSet<_>>();
        expected.insert(EntityId::Place(place));
        // resource_forms shares this observer and has an original post-For
        // readMap(source). Only that exact source call may add argument capabilities.
        for (_, node) in parsed.ast().expressions().iter() {
            if node.span().source_id() != scan.source_id()
                || node.span().start() <= scan.start()
                || node.span().end() >= scan.end()
            {
                continue;
            }
            let lang_frontend::parser::Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                continue;
            };
            let callee_span = parsed.ast().expressions().get(*callee).unwrap().span();
            if sources.slice(callee_span) != Ok("readMap") {
                continue;
            }
            let declaration = super::super::query_inputs::one(parsed.roots().iter().filter_map(|id| {
                let declaration = parsed.ast().items().get(*id).ok()?;
                matches!(declaration.payload(), Item::Function { name: NameMarker::Present(name), .. }
                    if sources.slice(*name) == Ok("readMap")).then_some(declaration.span())
            }), "owned source original post-provider readMap declaration")?;
            let target = super::super::query_graph::function(module, declaration)?;
            let call = super::super::query_inputs::one(graph.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(node.span())
                    && matches!(op.operation, Operation::DirectCall { callee, .. } if callee == target.id)
            }), "owned source exact original post-provider readMap call")?;
            let [argument] = arguments.as_slice() else {
                return Err(failure("owned source original readMap argument", arguments));
            };
            let argument_span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .unwrap()
                .span();
            let Operation::DirectCall {
                receiver: None,
                arguments,
                ..
            } = &call.operation
            else {
                return Err(failure("owned source actual readMap ABI", call));
            };
            let [argument @ EntityId::Loan(_)] = arguments.as_slice() else {
                return Err(failure("owned source actual readMap Shared argument", call));
            };
            let root = super::super::query_inputs::one(graph.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(argument_span)
                    && matches!(op.operation, Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)))
            }), "owned source original readMap argument exact current RootPlace")?;
            let Operation::RootPlace { owner } = root.operation else {
                unreachable!()
            };
            expected.extend(
                super::super::scope_future_observer::graph::observe_capabilities(
                    &graph,
                    call,
                    *argument,
                    owner,
                    argument_span,
                    node.span(),
                )?,
            );
        }
        super::super::query_inventory::check(&graph, &[ty.semantic_type()], &expected)?;
        need(returns > 0, "actual normal exit checked", returns)
    }

    fn edge_family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
        let mut members = BTreeSet::from([anchor]);
        loop {
            let before = members.len();
            for (_, edge) in &g.edges {
                for (&argument, &parameter) in
                    edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
                {
                    if members.contains(&argument) {
                        members.insert(parameter);
                    }
                }
            }
            if before == members.len() {
                break;
            }
        }
        for &member in &members {
            g.anchored(member, &[anchor])?;
        }
        Ok(members)
    }
    fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
        let mut pending = vec![current];
        let mut parents = BTreeMap::new();
        while let Some(entity) = pending.pop() {
            if entity == anchor || parents.contains_key(&entity) {
                continue;
            }
            let data = g
                .function
                .entity(entity)
                .ok_or_else(|| failure("owner entity", entity))?;
            let Definition::BlockParameter { block, .. } = data.definition else {
                return Err(failure("unrelated owner", data));
            };
            need(
                data.ty == g.function.entity(anchor).unwrap().ty,
                "owner type",
                data,
            )?;
            let incoming = g.incoming(block);
            need(!incoming.is_empty(), "full predecessor set", block)?;
            let values = incoming
                .iter()
                .map(|(_, e)| g.argument(e, entity))
                .collect::<Result<Vec<_>, _>>()?;
            pending.extend(values.iter().copied());
            parents.insert(entity, values);
        }
        let mut reached = BTreeSet::from([anchor]);
        loop {
            let before = reached.len();
            for (entity, values) in &parents {
                if values.iter().any(|v| reached.contains(v)) {
                    reached.insert(*entity);
                }
            }
            if reached.len() == before {
                break;
            }
        }
        need(
            parents.keys().all(|e| reached.contains(e)),
            "all incoming reach original owner",
            parents,
        )
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    direct: Option<String>,
}
fn owned_mutable(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        let line = if line.contains("MutableMap<") {
            line.to_owned()
        } else {
            line.replace("Map<String, String>", "MutableMap<String, String>")
        };
        if line.contains("val result = source.consume()") {
            continue;
        }
        let line = line
            .replace("return result", "return source")
            .replace("mapOf<String, String>()", "mutableMapOf<String, String>()");
        result.push_str(&line);
        result.push('\n');
    }
    result
}
fn grouped(text: &str) -> String {
    let mut sources = SourceMap::new();
    let id = sources.add_source("provider.ko", text).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let mut edits = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| {
            let Statement::For { source, .. } = n.payload() else {
                return None;
            };
            let span = parsed.ast().expressions().get(*source).unwrap().span();
            Some((span.start(), span.end()))
        })
        .collect::<Vec<_>>();
    edits.sort();
    let mut result = text.to_owned();
    for (start, end) in edits.into_iter().rev() {
        result.insert_str(end, ")))");
        result.insert_str(start, "(((");
    }
    result
}
pub(crate) fn sources() -> Vec<Case> {
    let normal_map = include_str!("../../../tests/fixtures/map-provider-0331/map.ko");
    let normal_map_golden = include_str!("../../../tests/fixtures/map-provider-0331/map.stdout");
    let mut direct = vec![
        (
            "map-normal".to_owned(),
            normal_map.to_owned(),
            normal_map_golden.to_owned(),
        ),
        (
            "borrow-map-normal".to_owned(),
            normal_map.replace("own source:", "borrow source:"),
            normal_map_golden.to_owned(),
        ),
        (
            "borrow-mutable-normal".to_owned(),
            include_str!("../../../tests/fixtures/map-provider-0331/mutable-map.ko").to_owned(),
            include_str!("../../../tests/fixtures/map-provider-0331/mutable-map.stdout").to_owned(),
        ),
    ];
    direct.extend(
        super::sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::borrowed::sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::conditional::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::temporary::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::temporary_exits::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::local_sources::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::nested::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::nested_exits::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    let mut cases = Vec::new();
    let mut mutable = vec![(
        "own-mutable-normal".to_owned(),
        owned_mutable(normal_map),
        normal_map_golden.to_owned(),
    )];
    mutable.extend(
        super::sources()
            .into_iter()
            .filter(|(l, _, _)| l.starts_with("map-"))
            .map(|(l, s, g)| {
                (
                    format!("own-mutable-{}", l.trim_start_matches("map-")),
                    owned_mutable(s),
                    g.to_owned(),
                )
            }),
    );
    for (l, s, g) in mutable {
        cases.push(Case {
            label: l.clone(),
            source: s.clone(),
            golden: g.clone(),
            direct: None,
        });
        direct.push((l, s, g));
    }
    cases.extend(direct.into_iter().map(|(l, s, g)| Case {
        label: format!("group-{l}"),
        source: grouped(&s),
        golden: g,
        direct: Some(s),
    }));
    assert_eq!(cases.len(), 41);
    assert_eq!(cases.iter().filter(|c| c.direct.is_some()).count(), 37);
    let labels = cases
        .iter()
        .map(|c| c.label.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(labels.len(), 41);
    for label in [
        "own-mutable-normal",
        "own-mutable-continue",
        "own-mutable-break",
        "own-mutable-return",
    ] {
        assert!(labels.contains(label), "missing {label}");
    }
    cases
}
fn observe(text: &str, unit: bool, expected_owned_mutable: bool) -> Result<String, Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| facts::unit(&sources, &parsed, names, typed, owned, report),
    );
    diagnostic!(
        "SOURCE FORMS rooted={unit} report={report:#?} outcome={:?}",
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    report.query(
        "Unit temporary Entry keeps outer source and actual producer/owner transport",
        &graph::temporary_entries(&parsed, &program),
    );
    if expected_owned_mutable {
        graph::owned_map(&sources, &parsed, &program, true)?;
    }
    assert_diagnostic!(report.passed(), "{report:#?}");
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))
}
#[test]
fn owned_mutable_and_group_sources_preserve_verified_programs() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let owned_mutable = case.label.starts_with("own-mutable-")
                || case.label.starts_with("group-own-mutable-");
            let direct = case
                .direct
                .as_ref()
                .map(|s| observe(s, unit, owned_mutable));
            let result = observe(&case.source, unit, owned_mutable);
            let same = match (&direct, &result) {
                (Some(Ok(a)), Ok(b)) => a == b,
                (None, Ok(_)) => true,
                _ => false,
            };
            diagnostic!(
                "SOURCE FORMS CASE {} rooted={unit} equivalent={same} result={:?}",
                case.label,
                result.as_ref().map(|s| s.len())
            );
            results.push((case.label.clone(), unit, same, result.map(|s| s.len())));
        }
    }
    diagnostic!("SOURCE FORMS total={}", results.len());
    assert_diagnostic!(
        results.len() == 82 && results.iter().all(|(_, _, same, r)| *same && r.is_ok()),
        "{results:#?}"
    );
}
