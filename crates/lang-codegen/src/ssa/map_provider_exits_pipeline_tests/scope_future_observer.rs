//! Original D6 positive observations with same-analysis owner and actual provider proofs.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Exact original AST declarations, future Name/Return and actual Calls.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{Expression, Item, NameMarker, ParameterModeMarker, ParsedFile, Statement},
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Input {
        pub scan: Span,
        pub scan_name: Span,
        pub entry: Span,
        pub factory: Span,
        pub factory_name: Span,
        pub observer: Span,
        pub observer_name: Span,
        pub source: Span,
        pub birth: Option<ExpressionId>,
        pub successor: Option<(Span, ExpressionId)>,
        pub returned: Option<(ExpressionId, ExpressionId)>,
        pub put: Option<ExpressionId>,
        pub observes: Vec<(ExpressionId, ExpressionId)>,
        pub scan_call: ExpressionId,
        pub scan_argument: Option<ExpressionId>,
        pub entry_birth: Option<ExpressionId>,
        pub result: Option<Span>,
        pub entry_observe: Option<(ExpressionId, ExpressionId)>,
        pub site: Option<(StatementId, ExpressionId)>,
    }
    #[derive(Debug)]
    pub(super) struct Site {
        pub statement: StatementId,
        pub span: Span,
        pub source: ExpressionId,
        pub body: StatementId,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub arguments: [ExpressionId; 2],
        pub reader: Span,
        pub reader_name: Span,
    }
    pub(super) fn site(p: &ParsedFile, i: &Input) -> Result<Site, Failure> {
        let (statement, source) = i.site.ok_or_else(|| failure("D original For site", i))?;
        let node = p.ast().statements().get(statement).unwrap();
        let Statement::For {
            binding: lang_frontend::parser::ForBinding::Destructuring { names, .. },
            body,
            ..
        } = node.payload()
        else {
            return Err(failure("D original destructuring For", node));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("D original key/value bindings", names));
        };
        let (read, callee, arguments) = one(
            p.ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call {
                        callee, arguments, ..
                    } if inside(node.span(), n.span()) => Some((id, *callee, arguments)),
                    _ => None,
                }),
            "D sole actual For reader Call",
        )?;
        let [a, b] = arguments.as_slice() else {
            return Err(failure("D reader exact two slots", arguments));
        };
        need(
            matches!(
                p.ast().expressions().get(callee).unwrap().payload(),
                Expression::Name
            ),
            "D reader actual direct Name",
            callee,
        )?;
        let (reader, reader_name) = one(
            p.roots().iter().filter_map(|id| {
                let n = p.ast().items().get(*id).ok()?;
                match n.payload() {
                    Item::Function {
                        name: NameMarker::Present(marker),
                        parameters,
                        ..
                    } if parameters.len() == 2
                        && inside(n.span(), *marker)
                        && marker.start() < i.scan.start() =>
                    {
                        Some((n.span(), *marker))
                    }
                    _ => None,
                }
            }),
            "D original two-parameter reader declaration",
        )?;
        // The selected call target is checked against this exact declaration by both FE views.
        Ok(Site {
            statement,
            span: node.span(),
            source,
            body: *body,
            bindings: [*key, *value],
            read,
            arguments: [a.value, b.value],
            reader,
            reader_name,
        })
    }
    pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
        p.ast().expressions().get(e).unwrap().span()
    }
    pub(super) fn inside(o: Span, i: Span) -> bool {
        o.source_id() == i.source_id() && o.start() < i.start() && i.end() < o.end()
    }
    fn function(
        s: &SourceMap,
        p: &ParsedFile,
        name: &str,
        arity: usize,
    ) -> Result<(Span, Span), Failure> {
        one(
            p.roots().iter().filter_map(|id| {
                let n = p.ast().items().get(*id).ok()?;
                match n.payload() {
                    Item::Function {
                        name: NameMarker::Present(marker),
                        parameters,
                        ..
                    } if s.slice(*marker) == Ok(name) && parameters.len() == arity => {
                        Some((n.span(), *marker))
                    }
                    _ => None,
                }
            }),
            "D original root function with actual arity",
        )
    }
    fn calls(s: &SourceMap, p: &ParsedFile, scope: Span, name: &str) -> Vec<ExpressionId> {
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { callee, .. }
                    if inside(scope, n.span()) && s.slice(span(p, *callee)) == Ok(name) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .collect()
    }
    pub(super) fn argument(p: &ParsedFile, call: ExpressionId) -> Result<ExpressionId, Failure> {
        let Expression::Call { arguments, .. } = p.ast().expressions().get(call).unwrap().payload()
        else {
            return Err(failure("D original Call", call));
        };
        let [arg] = arguments.as_slice() else {
            return Err(failure("D original one argument", arguments));
        };
        need(
            arg.mode_marker.is_none() && arg.named_prefix.is_none(),
            "D original plain argument",
            arg,
        )?;
        Ok(arg.value)
    }
    fn local(
        s: &SourceMap,
        p: &ParsedFile,
        scope: Span,
        name: &str,
    ) -> Result<(Span, ExpressionId), Failure> {
        one(
            p.ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer,
                        ..
                    } if inside(scope, n.span()) && s.slice(*marker) == Ok(name) => {
                        Some((*marker, *initializer))
                    }
                    _ => None,
                }),
            "D original local marker/initializer",
        )
    }
    pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
        let (scan, scan_name) = function(s, p, "scan", usize::from(c.code != 21))?;
        let entry = function(s, p, "entry", 0)?.0;
        let (factory, factory_name) =
            function(s, p, if c.code == 21 { "makeMutable" } else { "make" }, 0)?;
        function(s, p, "read", 2)?;
        let observer_text = if c.code == 21 {
            "observeMutable"
        } else {
            "observe"
        };
        let (observer, observer_name) = function(s, p, observer_text, 1)?;
        let (source, birth) = if c.code == 21 {
            let (marker, e) = local(s, p, scan, "source")?;
            need(
                s.slice(span(p, e)) == Ok("makeMutable()"),
                "D actual MutableMap birth",
                e,
            )?;
            (marker, Some(e))
        } else {
            let node = one(
                p.roots()
                    .iter()
                    .filter_map(|id| p.ast().items().get(*id).ok())
                    .filter(|n| n.span() == scan),
                "D actual scan AST",
            )?;
            let Item::Function { parameters, .. } = node.payload() else {
                unreachable!()
            };
            let [parameter] = parameters.as_slice() else {
                return Err(failure("D own source arity", parameters));
            };
            let NameMarker::Present(marker) = parameter.name else {
                return Err(failure("D own source name", parameter));
            };
            need(
                s.slice(marker) == Ok("source")
                    && matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_)))
                    && s.slice(p.ast().type_refs().get(parameter.type_ref).unwrap().span())
                        == Ok("Map<String,String>"),
                "D actual own Map source syntax",
                parameter,
            )?;
            (marker, None)
        };
        let successor = if c.code == 20 {
            let pair = local(s, p, scan, "successor")?;
            need(
                matches!(
                    p.ast().expressions().get(pair.1).unwrap().payload(),
                    Expression::Name
                ) && s.slice(span(p, pair.1)) == Ok("source"),
                "D successor actual MoveName source",
                pair,
            )?;
            Some(pair)
        } else {
            None
        };
        let returned = if c.code == 19 {
            let pair = one(
                p.ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Expression::Return {
                            value: Some(value), ..
                        } if inside(scan, n.span()) => Some((id, *value)),
                        _ => None,
                    }),
                "D actual Return source",
            )?;
            need(
                matches!(
                    p.ast().expressions().get(pair.1).unwrap().payload(),
                    Expression::Name
                ) && s.slice(span(p, pair.1)) == Ok("source"),
                "D return exact source Name",
                pair,
            )?;
            Some(pair)
        } else {
            None
        };
        let put = if c.code == 21 {
            Some(one(
                calls(s, p, scan, "source.put").into_iter(),
                "D actual future put",
            )?)
        } else {
            None
        };
        let mut observes = calls(s, p, scan, observer_text)
            .into_iter()
            .map(|call| Ok((call, argument(p, call)?)))
            .collect::<Result<Vec<_>, Failure>>()?;
        observes.sort_by_key(|(e, _)| span(p, *e).start());
        need(
            observes.len()
                == if c.provider {
                    usize::from(c.code != 19)
                } else {
                    if c.code == 19 { 1 } else { 2 }
                },
            "D exact original observer count",
            &observes,
        )?;
        let scan_call = one(
            calls(s, p, entry, "scan").into_iter(),
            "D actual caller scan",
        )?;
        let scan_argument = if c.code != 21 {
            Some(argument(p, scan_call)?)
        } else {
            None
        };
        let entry_birth = if let Some(arg) = scan_argument {
            need(
                s.slice(span(p, arg)) == Ok("make()"),
                "D original nested owned factory argument",
                arg,
            )?;
            Some(arg)
        } else {
            None
        };
        let (result, entry_observe) = if c.code == 19 {
            let (marker, init) = local(s, p, entry, "result")?;
            need(
                init == scan_call,
                "D original result initialized by actual scan",
                init,
            )?;
            let call = one(
                calls(s, p, entry, "observe").into_iter(),
                "D actual result observer",
            )?;
            (Some(marker), Some((call, argument(p, call)?)))
        } else {
            (None, None)
        };
        let sites = p
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Statement::For { source, .. } if inside(scan, n.span()) => Some((id, *source)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let site = if c.provider {
            Some(one(sites.into_iter(), "D original acquired For")?)
        } else {
            need(sites.is_empty(), "D control has no For", &sites)?;
            None
        };
        if let Some((_, e)) = site {
            need(
                s.slice(span(p, e)) == Ok("source"),
                "D actual For source Name",
                e,
            )?;
        }
        Ok(Input {
            scan,
            scan_name,
            entry,
            factory,
            factory_name,
            observer,
            observer_name,
            source,
            birth,
            successor,
            returned,
            put,
            observes,
            scan_call,
            scan_argument,
            entry_birth,
            result,
            entry_observe,
            site,
        })
    }
}
pub(crate) mod facts;
pub(in crate::ssa) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    code: u8,
    provider: bool,
}
pub(crate) fn cases() -> [Case; 6] {
    macro_rules! case {
        ($label:literal,$code:literal,$provider:literal) => {
            Case {
                label: $label,
                code: $code,
                provider: $provider,
                source: include_str!(concat!(
                    "../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".ko"
                )),
                golden: include_str!(concat!(
                    "../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".stdout"
                )),
            }
        };
    }
    [
        case!("P19-for", 19, true),
        case!("P19-control", 19, false),
        case!("P20-for", 20, true),
        case!("P20-control", 20, false),
        case!("P21-for", 21, true),
        case!("P21-control", 21, false),
    ]
}
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE D FUTURE INPUT label={} rooted={unit} source={:?} golden={:?}",
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
    diagnostic!(
        "SCOPE D FUTURE AST label={} rooted={unit} input={i:#?}",
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
            let result = facts::unit(c, &p, &i, n, t, o);
            r.query(
                "D original qualified future owner/return/move/put facts",
                &result,
            );
            proof = result.ok();
        },
    );
    diagnostic!(
        "SCOPE D FUTURE LOWER label={} rooted={unit} observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    need(observed, "D same original analysis observer ran", c.label)?;
    let (program, entry) = lowered?;
    let graph = proof
        .as_ref()
        .ok_or_else(|| failure("D mandatory original FE identity proof", &report))
        .and_then(|proof| graph::check(c, &p, &i, proof, &program, entry));
    report.query(
        "D original complete provider/control owner delivery and cleanup",
        &graph,
    );
    report.query(
        "D actual SSA verifier",
        &crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e)),
    );
    report.query(
        "D actual LLVM verifier",
        &crate::llvm::render_verified_program_with_entry(&program, entry)
            .map(|_| ())
            .map_err(|e| failure("LLVM", e)),
    );
    diagnostic!(
        "SCOPE D FUTURE FINAL label={} rooted={unit} report={report:#?}",
        c.label
    );
    need(
        report.passed(),
        "D original positive ordinary observation",
        report,
    )
}
#[test]
fn original_future_owner_handoffs_report_real_facts() {
    let mut outcomes = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            diagnostic!(
                "SCOPE D FUTURE CASE label={} rooted={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

pub(in crate::ssa) mod body {
//! Exact P17 checked assignment and P18 clone owner, with all component operand uses.
use crate::ssa::map_provider_exits_pipeline_tests::scope_lowering::graph::*;
use lang_frontend::parser::Expression;
fn end<'a>(
    g: &Graph<'a>,
    family: &BTreeSet<EntityId>,
    at: Span,
    after: &Instruction,
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let op=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if family.contains(&EntityId::Loan(loan)))),"scope unique synchronous actual loan End")?;
    need(
        op.origin == Origin::Source(at),
        "scope exact original synchronous loan endpoint",
        op,
    )?;
    before(g, after, op)?;
    for e in op.operation.entities() {
        uses.permit(e, op);
    }
    Ok(op)
}
/// Only actual SharedReborrow operations and actual block columns may link an
/// argument to the previously proved component. Every created child has its End.
fn component(
    g: &Graph<'_>,
    argument: EntityId,
    roots: &BTreeSet<EntityId>,
    spans: (Span, Span),
    call: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    if roots.contains(&argument) {
        uses.permit(argument, call);
        return Ok(());
    }
    let begin = g.definition(argument)?;
    let Operation::SharedReborrow { source } = begin.operation else {
        return Err(failure(
            "scope component argument actual Shared lineage",
            begin,
        ));
    };
    need(
        begin.origin == Origin::Source(spans.0),
        "scope actual selected component argument birth",
        begin,
    )?;
    component(g, EntityId::Loan(source), roots, spans, begin, uses)?;
    let aliases = family(g, &[argument])?;
    protect(uses, &aliases);
    uses.permit(argument, call);
    before(g, begin, call)?;
    end(g, &aliases, spans.1, call, uses)?;
    Ok(())
}
fn owner_argument<'a>(
    g: &Graph<'a>,
    argument: EntityId,
    owner: ValueId,
    at: Span,
    call: &Instruction,
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let begin = g.definition(argument)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("scope exact owner argument Shared birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation == Operation::RootPlace { owner }
            && root.origin == Origin::Source(at)
            && begin.origin == root.origin,
        "scope exact original owner/argument RootPlace identity",
        (root, begin),
    )?;
    before(g, root, begin)?;
    before(g, begin, call)?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(argument);
    uses.permit(argument, call);
    Ok(begin)
}
fn literal(
    g: &Graph<'_>,
    argument: EntityId,
    span: Span,
    bytes: &[u8],
    call: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let begin = g.definition(argument)?;
    let Operation::BorrowBegin { place, .. } = begin.operation else {
        return Err(failure("scope literal actual Shared argument", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("scope literal actual root", root));
    };
    let producer = g.definition(EntityId::Value(owner))?;
    need(
        matches!(&producer.operation,Operation::StringLiteral{bytes:actual,..}if actual==bytes)
            && producer.origin == Origin::Source(span),
        "scope actual control literal bytes/origin",
        producer,
    )?;
    owner_argument(g, argument, owner, span, call, uses)?;
    let stop = end(
        g,
        &BTreeSet::from([argument]),
        call.origin.span(),
        call,
        uses,
    )?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::Drop { owner }),
        "scope literal unique original owner Drop",
    )?;
    need(
        drop.origin == Origin::Source(span),
        "scope literal actual cleanup origin",
        drop,
    )?;
    before(g, stop, drop)?;
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
pub(in crate::ssa) fn check(
    c: &Case,
    _sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    if !matches!(c.gate, 17 | 18) {
        return Ok(());
    }
    let read = i.read.as_ref().unwrap();
    let invoke = at(g, ast::span(parsed, read.id), |o| {
        matches!(o, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &invoke.operation
    else {
        return Err(failure("scope exact actual read ABI", invoke));
    };
    need(
        *callee == function(module, i.reader.as_ref().unwrap().span)?.id
            && arguments.len() == 2
            && invoke.results.is_empty(),
        "scope body exact original read target and K/V slot order",
        invoke,
    )?;
    let mut components = Vec::new();
    let mut roots = Vec::new();
    if let Some(site) = &i.site {
        for predicate in [0, 1] {
            let projection = at(g, site.span, |op| {
                if predicate == 0 {
                    matches!(op, Operation::MapIterationKeyLoan { .. })
                } else {
                    matches!(op, Operation::MapIterationValueLoan { .. })
                }
            })?;
            let aliases = family(g, &[result(projection)?])?;
            protect(uses, &aliases);
            roots.push(projection);
            components.push(aliases);
        }
        for n in 0..2 {
            component(
                g,
                arguments[n],
                &components[n],
                (
                    ast::span(parsed, read.arguments[n]),
                    ast::span(parsed, read.id),
                ),
                invoke,
                uses,
            )?;
        }
    } else {
        for (n, bytes) in [b"key".as_slice(), b"value".as_slice()]
            .into_iter()
            .enumerate()
        {
            literal(
                g,
                arguments[n],
                ast::span(parsed, read.arguments[n]),
                bytes,
                invoke,
                uses,
            )?;
        }
    }
    if let Some(id) = i.assignment {
        checked(parsed, i, module, g, uses, id, invoke)?;
    }
    if let Some(id) = i.clone {
        let cloned = at(g, ast::span(parsed, id), |o| {
            matches!(o, Operation::StringClone { .. })
        })?;
        let Operation::StringClone { source } = cloned.operation else {
            unreachable!()
        };
        let Expression::Call { callee, .. } = parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            unreachable!()
        };
        if c.provider {
            component(
                g,
                EntityId::Loan(source),
                &components[0],
                (ast::span(parsed, *receiver), ast::span(parsed, id)),
                cloned,
                uses,
            )?;
            before(g, roots[0], cloned)?;
        } else {
            literal(
                g,
                EntityId::Loan(source),
                ast::span(parsed, *receiver),
                b"key",
                cloned,
                uses,
            )?;
        }
        let owner = value(cloned)?;
        let print = i.clone_print.as_ref().unwrap();
        let actual = at(g, ast::span(parsed, print.id), |o| {
            matches!(o, Operation::PrintString { .. })
        })?;
        let Operation::PrintString { value: borrow } = actual.operation else {
            unreachable!()
        };
        owner_argument(
            g,
            EntityId::Loan(borrow),
            owner,
            ast::span(parsed, print.arguments[0]),
            actual,
            uses,
        )?;
        let stop = end(
            g,
            &BTreeSet::from([EntityId::Loan(borrow)]),
            ast::span(parsed, print.id),
            actual,
            uses,
        )?;
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner }),
            "scope unique fresh cloned owner cleanup",
        )?;
        need(
            drop.origin == Origin::Source(i.cloned.unwrap()),
            "scope actual named clone Drop identity",
            drop,
        )?;
        before(g, cloned, actual)?;
        before(g, stop, drop)?;
        before(g, drop, invoke)?;
        uses.permit(EntityId::Value(owner), drop);
    }
    if let Some(site) = &i.site {
        let vend = end(g, &components[1], site.span, invoke, uses)?;
        let kend = end(g, &components[0], site.span, invoke, uses)?;
        let next = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationNext { .. })
        })?;
        before(g, vend, kend)?;
        before(g, kend, next)?;
        need(
            !g.edges.iter().any(|(_, edge)| {
                edge.target == g.branch(next.block).unwrap().target
                    && edge
                        .arguments
                        .iter()
                        .any(|e| components.iter().any(|family| family.contains(e)))
            }),
            "scope no live component on actual provider backedge",
            next,
        )?;
    }
    Ok(())
}
fn checked(
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    uses: &mut Uses,
    id: lang_frontend::ast::ExpressionId,
    read: &Instruction,
) -> Result<(), Failure> {
    let Expression::Assignment { value: rhs, .. } =
        parsed.ast().expressions().get(id).unwrap().payload()
    else {
        unreachable!()
    };
    let Expression::Binary {
        left: lhs,
        right: rhsarg,
        ..
    } = parsed.ast().expressions().get(*rhs).unwrap().payload()
    else {
        unreachable!()
    };
    let add = at(g, ast::span(parsed, *rhs), |o| {
        matches!(o, Operation::CheckedArithmetic { .. })
    })?;
    let Operation::CheckedArithmetic {
        operator: CheckedArithmeticOperator::Add,
        left,
        right,
    } = add.operation
    else {
        return Err(failure("scope actual assigned checked Add", add));
    };
    let zero = g.definition(EntityId::Value(left))?;
    let one_value = g.definition(EntityId::Value(right))?;
    need(
        zero.operation == Operation::Constant(ScalarConstant::Integer(0))
            && one_value.operation == Operation::Constant(ScalarConstant::Integer(1))
            && one_value.origin == Origin::Source(ast::span(parsed, *rhsarg))
            && ast::span(parsed, *lhs).start() < ast::span(parsed, *rhsarg).start(),
        "scope assigned n+1 uses actual current zero and source one",
        (zero, one_value),
    )?;
    let [EntityId::Value(sum), EntityId::Value(failed)] = add.results.as_slice() else {
        return Err(failure(
            "scope checked assignment result/failure identities",
            add,
        ));
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(add.block)?.kind
    else {
        return Err(failure(
            "scope checked assignment actual success/Abort CFG",
            add,
        ));
    };
    need(
        *condition == *failed
            && matches!(g.term(when_true.target)?.kind, TerminatorKind::Abort)
            && when_false.target != when_true.target,
        "scope failure aborts and successful assignment enters normal continuation",
        g.term(add.block)?,
    )?;
    g.incoming_from(when_false.target, &[add.block])?;
    g.incoming_from(when_true.target, &[add.block])?;
    before(g, add, read)?;
    let count = i.count_call.as_ref().unwrap();
    let call = at(g, ast::span(parsed, count.id), |o| {
        matches!(o, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(failure(
            "scope checked assigned scalar actual consumer ABI",
            call,
        ));
    };
    need(
        *callee == function(module, i.count.as_ref().unwrap().span)?.id && arguments.len() == 1,
        "scope checked current scalar exact selected count callable",
        call,
    )?;
    let begin = g.definition(arguments[0])?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("scope count actual current scalar Borrow", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("scope count assigned result actual place", root));
    };
    let sums = family(g, &[EntityId::Value(*sum)])?;
    need(
        sums.contains(&EntityId::Value(owner))
            && root.origin == Origin::Source(ast::span(parsed, count.arguments[0]))
            && begin.origin == root.origin,
        "scope actual assigned Add result reaches latest count argument",
        (root, begin),
    )?;
    for e in [EntityId::Value(left), EntityId::Value(right)] {
        uses.protect(e);
        uses.permit(e, add);
    }
    protect(uses, &sums);
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(arguments[0]);
    uses.permit(arguments[0], call);
    let stop = end(
        g,
        &BTreeSet::from([arguments[0]]),
        ast::span(parsed, count.id),
        call,
        uses,
    )?;
    before(g, read, call)?;
    if let Some(site) = &i.site {
        let next = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationNext { .. })
        })?;
        before(g, stop, next)?;
    }
    Ok(())
}
}
