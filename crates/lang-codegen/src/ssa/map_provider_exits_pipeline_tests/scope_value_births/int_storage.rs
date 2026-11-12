//! P06 original Int storage programs, independently queried through both public entry paths.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Original Int storage, scalar payloads, reader comparisons and For/control syntax.
    use super::super::super::{
        births::ast::Site,
        fixture::{Failure, failure, need},
        query_inputs::one,
        temporary_arguments::ast::function,
    };
    use super::Case;
    use lang_frontend::{
        ast::ExpressionId,
        parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Input {
        pub entry: Span,
        pub scan: Span,
        pub make: Span,
        pub reader: Span,
        pub make_name: Span,
        pub scan_name: Span,
        pub reader_name: Span,
        pub reader_params: [Span; 2],
        pub scanner_call: ExpressionId,
        pub source: Span,
        pub birth: ExpressionId,
        pub initial: Span,
        pub construction: ExpressionId,
        pub put: ExpressionId,
        pub payload: [ExpressionId; 2],
        pub consume: ExpressionId,
        pub returned: ExpressionId,
        pub site: Option<Site>,
        pub size: Option<ExpressionId>,
        pub compares: [ExpressionId; 2],
        pub compared: [ExpressionId; 2],
        pub literals: [ExpressionId; 2],
    }
    pub(super) fn span(p: &ParsedFile, id: ExpressionId) -> Span {
        p.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn inside(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    pub(super) fn input(c: &Case, sources: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
        let entry = function(sources, p, "entry", None)?.span;
        let scanner = function(sources, p, "scan", None)?;
        let factory = function(sources, p, "makeInt", None)?;
        let (reader, reader_name, reader_params) = one(
            p.roots()
                .iter()
                .filter_map(|id| p.ast().items().get(*id).ok())
                .filter_map(|n| match n.payload() {
                    Item::Function {
                        name: NameMarker::Present(name),
                        parameters,
                        ..
                    } if sources.slice(*name) == Ok("readInt") => Some((n.span(), *name, parameters)),
                    _ => None,
                }),
            "Int actual reader declaration",
        )?;
        let reader_params = reader_params
            .iter()
            .map(|p| match p.name {
                NameMarker::Present(s) => Ok(s),
                _ => Err(failure("Int actual reader parameter", p)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reader_params: [Span; 2] = reader_params
            .try_into()
            .map_err(|v| failure("Int exact two reader parameters", v))?;
        let call = |scope, wanted| {
            one(
                p.ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Expression::Call { callee, .. }
                            if inside(scope, n.span())
                                && sources.slice(span(p, *callee)) == Ok(wanted) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    }),
                "Int actual ordinary source Call",
            )
        };
        let local = |scope, wanted| {
            one(
                p.ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if inside(scope, n.span()) && sources.slice(*name) == Ok(wanted) => {
                            Some((*name, *initializer))
                        }
                        _ => None,
                    }),
                "Int actual named owner initializer",
            )
        };
        let (source, birth) = local(scanner.span, "source")?;
        need(
            birth == call(scanner.span, "makeInt")?,
            "Int source actual makeInt birth",
            birth,
        )?;
        let (initial, construction) = local(factory.span, "m")?;
        let member = |wanted| {
            one(p.ast().expressions().iter().filter_map(|(id,n)|match n.payload(){Expression::Call{callee,..}if inside(factory.span,n.span())&&matches!(p.ast().expressions().get(*callee).unwrap().payload(),Expression::Member{name_span,..}if sources.slice(*name_span)==Ok(wanted))=>Some(id),_=>None}),"Int original factory member Call")
        };
        let put = member("put")?;
        let consume = member("consume")?;
        let returned = one(
            p.ast().expressions().iter().filter_map(|(id, n)| {
                matches!(n.payload(),Expression::Return{value:Some(v),..}if *v==consume).then_some(id)
            }),
            "Int actual owned consume Return",
        )?;
        let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
        else {
            unreachable!()
        };
        let [a, b] = arguments.as_slice() else {
            return Err(failure("Int actual two put payloads", arguments));
        };
        let payload = [a.value, b.value];
        for (id, text) in payload.into_iter().zip(["1", "2"]) {
            need(
                sources.slice(span(p, id)) == Ok(text),
                "Int fixed actual put payload literal",
                id,
            )?;
        }
        let site = if c.provider {
            let (statement, n, names, source, body) = one(
                p.ast()
                    .statements()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Statement::For {
                            binding: ForBinding::Destructuring { names, .. },
                            source,
                            body,
                            ..
                        } if inside(scanner.span, n.span()) => Some((id, n, names, *source, *body)),
                        _ => None,
                    }),
                "Int actual original For",
            )?;
            let [NameMarker::Present(k), NameMarker::Present(v)] = names.as_slice() else {
                return Err(failure("Int actual K/V markers", names));
            };
            let read = call(n.span(), "readInt")?;
            let Expression::Call { arguments, .. } = p.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            let [karg, varg] = arguments.as_slice() else {
                return Err(failure("Int reader original slots", arguments));
            };
            Some(Site {
                statement,
                span: n.span(),
                source,
                body,
                bindings: [*k, *v],
                read,
                arguments: [karg.value, varg.value],
            })
        } else {
            None
        };
        let size = if c.provider {
            None
        } else {
            Some(one(p.ast().expressions().iter().filter_map(|(id,n)|matches!(n.payload(),Expression::Member{name_span,..}if inside(scanner.span,n.span())&&sources.slice(*name_span)==Ok("size")).then_some(id)),"Int actual control MapSize")?)
        };
        let mut comparison = p
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Binary {
                    left,
                    right,
                    operator: lang_frontend::parser::BinaryOperator::Equal,
                    ..
                } if inside(reader, n.span()) => Some((id, *left, *right)),
                _ => None,
            })
            .collect::<Vec<_>>();
        comparison.sort_by_key(|(id, _, _)| span(p, *id).start());
        let [a, b] = comparison.as_slice() else {
            return Err(failure("Int exact reader comparisons", comparison));
        };
        Ok(Input {
            entry,
            scan: scanner.span,
            make: factory.span,
            reader,
            make_name: factory.name,
            scan_name: scanner.name,
            reader_name,
            reader_params,
            scanner_call: call(entry, "scan")?,
            source,
            birth,
            initial,
            construction,
            put,
            payload,
            consume,
            returned,
            site,
            size,
            compares: [a.0, b.0],
            compared: [a.1, b.1],
            literals: [a.2, b.2],
        })
    }
}
mod facts;
mod graph;
mod sources {
    pub(crate) struct Case {
        pub label: &'static str,
        pub provider: bool,
        pub source: &'static str,
        pub golden: &'static str,
    }
    pub(crate) fn sources() -> [Case; 2] {
        [
            Case {
                label: "P06-for",
                provider: true,
                source: include_str!(
                    "../../../../../tests/fixtures/map-provider-scope-0331/P06-for.ko"
                ),
                golden: include_str!(
                    "../../../../../tests/fixtures/map-provider-scope-0331/P06-for.stdout"
                ),
            },
            Case {
                label: "P06-control",
                provider: false,
                source: include_str!(
                    "../../../../../tests/fixtures/map-provider-scope-0331/P06-control.ko"
                ),
                golden: include_str!(
                    "../../../../../tests/fixtures/map-provider-scope-0331/P06-control.stdout"
                ),
            },
        ]
    }
}
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE B INT INPUT label={} rooted={unit} source={:?} golden={:?}",
        c.label,
        c.source,
        c.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let p = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", p.diagnostics())?;
    let input = ast::input(c, &sources, &p)?;
    let mut report = Report::default();
    let mut proof = None;
    let lower = pipeline::unit_input_observed(
        &sources,
        &p,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            let result = facts::unit((c, &sources, &p, &input), n, t, o);
            r.query(
                "Int actual qualified creation/Copy types/consume/reader/current cleanup",
                &result,
            );
            proof = result.ok();
        },
    );
    diagnostic!(
        "SCOPE B INT LOWER label={} rooted={unit} report={report:#?} outcome={:?}",
        c.label,
        lower.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lower?;
    let query = proof
        .as_ref()
        .ok_or_else(|| failure("Int same-analysis cleanup proof", &report))
        .and_then(|proof| graph::check(c, &p, &input, proof, &program));
    report.query(
        "Int actual factory/Copy comparisons/current owner/full incoming/all protected uses",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "SCOPE B INT FINAL label={} rooted={unit} report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("Int actual deep identity queries", report))
    }
}
#[test]
fn original_int_storage_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    for c in sources() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            diagnostic!(
                "SCOPE B INT CASE label={} rooted={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
