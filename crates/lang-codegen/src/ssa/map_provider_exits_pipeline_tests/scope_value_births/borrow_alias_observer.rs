//! Original P05 canonical Borrow alias facts and complete ordinary runtime identities.
mod facts;
mod graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
    query_graph::{Graph, Uses, function, loan, result},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    lexer::lex,
    name_resolution::{NameResolution, SymbolId, UnitReferenceTarget},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, VariableKind, parse_file},
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;
pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    provider: bool,
}
pub(crate) fn cases() -> [Case; 2] {
    [
        Case {
            label: "P05-for",
            provider: true,
            source: include_str!("../../../../tests/fixtures/map-provider-scope-0331/P05-for.ko"),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P05-for.stdout"
            ),
        },
        Case {
            label: "P05-control",
            provider: false,
            source: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P05-control.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P05-control.stdout"
            ),
        },
    ]
}
#[derive(Debug)]
struct Input {
    scan: Span,
    scan_name: Span,
    body: StatementId,
    endpoint: StatementId,
    declaration: Span,
    entry: Span,
    reader: Span,
    reader_name: Span,
    source: Span,
    alias: Span,
    marker: Span,
    initializer: ExpressionId,
    site: Option<(StatementId, ExpressionId)>,
    call: Option<ExpressionId>,
    observe: Span,
}
fn span(p: &ParsedFile, id: ExpressionId) -> Span {
    p.ast().expressions().get(id).unwrap().span()
}
fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn input(c: &Case, sources: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
    let scan=one(p.roots().iter().filter_map(|id|p.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok("scan"))),"alias original scan declaration")?;
    let Item::Function {
        name: NameMarker::Present(scan_name),
        parameters,
        form,
        ..
    } = scan.payload()
    else {
        unreachable!()
    };
    let body = match form {
        lang_frontend::parser::FunctionForm::Explicit {
            body: lang_frontend::parser::FunctionBody::Block(body),
            ..
        }
        | lang_frontend::parser::FunctionForm::ImplicitUnitBlock(body) => *body,
        _ => return Err(failure("alias original scanner block", form)),
    };
    let entry = super::super::temporary_arguments::ast::function(sources, p, "entry", None)?.span;
    let (reader, reader_name) = one(
        p.roots()
            .iter()
            .filter_map(|id| p.ast().items().get(*id).ok())
            .filter_map(|n| match n.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } if sources.slice(*name) == Ok("read") => Some((n.span(), *name)),
                _ => None,
            }),
        "alias actual original reader",
    )?;
    let source = match parameters[0].name {
        NameMarker::Present(s) => s,
        _ => return Err(failure("alias original source parameter", parameters)),
    };
    let (alias, marker, initializer, declaration) = one(
        p.ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    kind: VariableKind::BorrowVal(marker),
                    name: NameMarker::Present(alias),
                    initializer,
                    ..
                } if inside(scan.span(), n.span()) => {
                    Some((*alias, *marker, *initializer, n.span()))
                }
                _ => None,
            }),
        "alias actual borrow val AST declaration",
    )?;
    let site = if c.provider {
        Some(one(
            p.ast()
                .statements()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    lang_frontend::parser::Statement::For {
                        binding: ForBinding::Destructuring { .. },
                        source,
                        ..
                    } if inside(scan.span(), n.span()) => Some((id, *source)),
                    _ => None,
                }),
            "alias actual original For",
        )?)
    } else {
        None
    };
    let call = if c.provider {
        None
    } else {
        Some(one(
            p.ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { callee, .. }
                        if inside(scan.span(), n.span())
                            && sources.slice(span(p, *callee)) == Ok("observe") =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "alias original observe Call",
        )?)
    };
    let endpoint = if c.provider {
        body
    } else {
        let call = call.ok_or_else(|| failure("alias control mandatory observe Call", scan))?;
        let node = p.ast().statements().get(body).unwrap();
        let lang_frontend::parser::Statement::Block { elements } = node.payload() else {
            return Err(failure("alias control actual scanner block", node));
        };
        let statement = one(
            elements.iter().copied().filter(|id| {
                matches!(p.ast().statements().get(*id).unwrap().payload(),
                    lang_frontend::parser::Statement::Expression { expression } if *expression == call)
            }),
            "alias control actual observe expression statement",
        )?;
        need(
            elements.last() == Some(&statement),
            "alias control observe statement is the final scanner operation",
            elements,
        )?;
        statement
    };
    let observe=one(p.roots().iter().filter_map(|id|p.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok("observe"))).map(|n|n.span()),"alias actual observe declaration")?;
    Ok(Input {
        scan: scan.span(),
        scan_name: *scan_name,
        body,
        endpoint,
        declaration,
        entry,
        reader,
        reader_name,
        source,
        alias,
        marker,
        initializer,
        site,
        call,
        observe,
    })
}
fn symbol(n: &NameResolution, s: Span) -> Result<SymbolId, Failure> {
    one(
        n.symbols()
            .iter()
            .filter(|v| v.span() == s && !v.is_synthetic()),
        "alias actual original symbol",
    )
    .map(|v| v.id())
}
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE B ALIAS INPUT label={} rooted={unit} source={:?} golden={:?}",
        c.label, c.source, c.golden
    );
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let p = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", p.diagnostics())?;
    let i = input(c, &sources, &p)?;
    println!(
        "SCOPE B ALIAS AST label={} rooted={unit} input={i:?}",
        c.label
    );
    let mut report = Report::default();
    let mut observed = false;
    let mut proof = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &p,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            observed = true;
            let checked = facts::unit(c, &sources, &p, &i, n, t, o);
            r.query(
                "alias exact qualified canonical storage/ends/complete plan contract",
                &checked,
            );
            proof = checked.ok();
            let data = (|| -> Result<(), Failure> {
                let u = one(
                    n.names().source_units().iter(),
                    "alias actual original unit",
                )?;
                let id = u.source_unit();
                let e = |v| lang_frontend::type_checking::UnitExpressionId::new(id, v);
                let local = symbol(u.resolution(), i.alias)?;
                let alias = one(
                    n.names()
                        .references()
                        .iter()
                        .filter_map(|r| match r.target() {
                            UnitReferenceTarget::Symbol(v)
                                if v.source_unit() == id && v.symbol() == local =>
                            {
                                Some(*v)
                            }
                            _ => None,
                        })
                        .collect::<BTreeSet<_>>()
                        .into_iter(),
                    "alias actual qualified alias",
                )?;
                let bindings = o
                    .borrow_results()
                    .bindings()
                    .iter()
                    .filter(|b| b.binding() == alias)
                    .collect::<Vec<_>>();
                let ends = o
                    .borrow_results()
                    .ends()
                    .iter()
                    .filter(|end| end.binding() == alias)
                    .collect::<Vec<_>>();
                println!(
                    "SCOPE B ALIAS FE label={} unit=true alias={alias:?} type={:?} initializer_type={:?} initializer_category={:?} bindings={:?} ends={:?} all_borrow_results={:#?}",
                    c.label,
                    t.symbol_type(alias),
                    t.expression_type(e(i.initializer)),
                    t.expression_category(e(i.initializer)),
                    bindings,
                    ends,
                    o.borrow_results()
                );
                for b in &bindings {
                    println!(
                        "SCOPE B ALIAS BINDING label={} unit=true binding={:?} initializer={:?} storage={:?} origin={:?} parent={:?} source_loan={:?} marker={:?}",
                        c.label,
                        b.binding(),
                        b.initializer(),
                        b.storage(),
                        b.origin(),
                        b.parent(),
                        b.source_loan(),
                        b.marker_span()
                    );
                }
                if let Some((statement, source)) = i.site {
                    let statement =
                        lang_frontend::type_checking::UnitStatementId::new(id, statement);
                    let plan = o.map_iteration(statement);
                    println!(
                        "SCOPE B ALIAS PLAN label={} unit=true descriptor={:?} ownership={plan:#?}",
                        c.label,
                        t.map_iteration(statement)
                    );
                    if let Some(plan) = plan {
                        println!(
                            "SCOPE B ALIAS CANONICAL label={} unit=true source={:?} access={:?} components={:?} exits={:#?}",
                            c.label,
                            plan.source(),
                            plan.source_access(),
                            plan.loans(),
                            plan.exits()
                        );
                    }
                    need(
                        plan.is_some() && t.expression_type(e(source)).is_some(),
                        "alias actual acquired qualified For plan exists",
                        plan,
                    )?;
                }
                if let Some(call) = i.call {
                    println!(
                        "SCOPE B ALIAS CONTROL label={} unit=true typed_call={:?} argument_contracts={:?} loans={:?}",
                        c.label,
                        t.call(e(call)),
                        o.call_argument_contracts_for(e(call)).collect::<Vec<_>>(),
                        o.loans()
                            .iter()
                            .filter(|v| v.call() == e(call))
                            .collect::<Vec<_>>()
                    );
                }
                need(
                    bindings.len() == 1
                        && !ends.is_empty()
                        && bindings[0].initializer() == e(i.initializer)
                        && bindings[0].marker_span() == i.marker,
                    "alias qualified observation contains true AST binding and nonzero real ends",
                    (bindings, ends),
                )?;
                Ok(())
            })();
            r.query(
                "alias same-analysis qualified public observation collected",
                &data,
            );
        },
    );
    println!(
        "SCOPE B ALIAS LOWER label={} rooted={unit} observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "alias observer actually received original analysis",
        c.label,
    )?;
    let (program, _) = lowered?;
    let query = proof
        .as_ref()
        .ok_or_else(|| failure("alias same-analysis actual binding endpoint proof", &report))
        .and_then(|proof| graph::check(c, &sources, &p, &i, proof, &program));
    report.query(
        "alias actual parameter/alias/provider/all incoming/protected uses/scope cleanup",
        &query,
    );

    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if report.passed() {
        Ok(())
    } else {
        Err(failure(
            "alias original same-analysis observation query",
            report,
        ))
    }
}
fn control(p: &ParsedFile, i: &Input, program: &Program) -> Result<BTreeSet<EntityId>, Failure> {
    let module = one(program.modules.iter(), "alias actual original SSA module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::new(scan)?;
    let source = g.block(scan.entry_block().unwrap())?.parameters[0];
    println!(
        "SCOPE B ALIAS SSA function={:?} parameters={:?} edges={:?} instructions={:#?}",
        scan.id,
        g.block(scan.entry_block().unwrap())?.parameters,
        g.edges,
        g.instructions
    );
    need(
        g.blocks.len() == 1
            && g.edges.is_empty()
            && matches!(scan.entity(source).unwrap().ty,EntityType::Loan{kind:LoanKind::Shared,target}if module.map_container(target).is_some()),
        "alias control actual Borrow Map parameter/straight-line CFG",
        source,
    )?;
    let birth=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::SharedReborrow{source:parent}if EntityId::Loan(parent)==source)),"alias actual independent Shared reborrow birth")?;
    let alias = loan(birth)?;
    let call = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(span(p, i.call.unwrap()))
                && matches!(op.operation, Operation::DirectCall { .. })
        }),
        "alias actual observe DirectCall",
    )?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(failure("alias control actual observe ABI", call));
    };
    need(
        *callee == function(module, i.observe)?.id
            && arguments.len() == 1
            && g.position(birth)? < g.position(call)?,
        "alias actual selected observe after alias birth",
        call,
    )?;
    let mut uses = Uses::default();
    uses.protect(source);
    uses.permit(source, birth);
    uses.protect(EntityId::Loan(alias));
    let Expression::Call {
        arguments: original_arguments,
        ..
    } = p
        .ast()
        .expressions()
        .get(i.call.unwrap())
        .unwrap()
        .payload()
    else {
        return Err(failure("alias original observe is a Call", i.call));
    };
    let argument_span = span(
        p,
        one(
            original_arguments.iter(),
            "alias original observe sole argument",
        )?
        .value,
    );
    let mut lineage = BTreeSet::from([EntityId::Loan(alias)]);
    let mut children = Vec::new();
    let mut current = arguments[0];
    let mut later = g.position(call)?;
    while current != EntityId::Loan(alias) {
        need(
            lineage.insert(current),
            "alias exact observe ancestor chain is acyclic",
            current,
        )?;
        let child = g.definition(current)?;
        let Operation::SharedReborrow { source: parent } = child.operation else {
            return Err(failure(
                "alias observe argument descends from the actual original alias",
                child,
            ));
        };
        need(
            child.block == call.block
                && child.origin == Origin::Source(argument_span)
                && g.position(birth)? < g.position(child)?
                && g.position(child)? < later,
            "alias exact observed child is born after alias and before its consumer",
            child,
        )?;
        uses.permit(EntityId::Loan(parent), child);
        uses.protect(current);
        children.push(current);
        current = EntityId::Loan(parent);
        later = g.position(child)?;
    }
    uses.permit(arguments[0], call);
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::BorrowEnd { loan: alias }),
        "alias actual unique alias End",
    )?;
    need(
        g.position(call)? < g.position(end)?,
        "alias remains live through actual observe Call",
        (call, end),
    )?;
    uses.permit(EntityId::Loan(alias), end);
    let mut prior_end = g.position(call)?;
    for child in children {
        let end_child=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==child)),"alias actual child End")?;
        need(
            prior_end < g.position(end_child)?
                && g.position(end_child)? < g.position(end)?
                && end_child.origin == Origin::Source(span(p, i.call.unwrap())),
            "alias exact observed child chain closes in reverse order at original CallReturn before alias End",
            end_child,
        )?;
        prior_end = g.position(end_child)?;
        uses.permit(child, end_child);
    }
    need(!g.instructions.iter().any(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==source))&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())),"alias original source Borrow remains caller-owned",scan.id)?;
    uses.check(&g, None)?;
    Ok(lineage)
}
#[test]
fn original_alias_sources_report_real_facts() {
    let mut results = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            println!(
                "SCOPE B ALIAS CASE label={} rooted={unit} result={result:?}",
                c.label
            );
            results.push((c.label, unit, result));
        }
    }
    assert_eq!(results.len(), 4);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
