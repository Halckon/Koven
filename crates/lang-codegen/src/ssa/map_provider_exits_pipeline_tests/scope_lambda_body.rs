//! A2 preserves the two original P15 programs and their real lambda/body identities.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! The thunk is selected by the real lambda initializer and LambdaBody identity.
    use super::super::{
        births::ast::Site,
        fixture::{Failure, failure, need},
        query_inputs::one,
        temporary_arguments::ast::{Function, function},
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Input {
        pub entry: Function,
        pub scan: Function,
        pub make: Function,
        pub produce: Function,
        pub reader: Span,
        pub observe: Span,
        pub scan_call: ExpressionId,
        pub callback: Span,
        pub lambda: ExpressionId,
        pub body: StatementId,
        pub invoke: ExpressionId,
        pub callee: ExpressionId,
        pub source: Span,
        pub birth: ExpressionId,
        pub birth_span: Span,
        pub site: Option<Site>,
        pub observe_call: Option<ExpressionId>,
        pub done: ExpressionId,
    }
    pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn inside(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    fn local(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
    ) -> Result<(Span, ExpressionId), Failure> {
        one(
            parsed
                .ast()
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
            "lambda actual local declaration/initializer",
        )
    }
    fn call(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
    ) -> Result<ExpressionId, Failure> {
        one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { callee, .. }
                        if inside(scope, n.span())
                            && sources.slice(span(parsed, *callee)) == Ok(wanted) =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "lambda actual original Call",
        )
    }
    pub(super) fn input(c: &Case, sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
        let entry = function(sources, parsed, "entry", None)?;
        let scan = function(sources, parsed, "scan", None)?;
        let make = function(sources, parsed, "make", None)?;
        let produce = function(sources, parsed, "produce", Some("Int"))?;
        let named = |wanted| {
            one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok(wanted))).map(|n|n.span()),"lambda actual top-level consumer")
        };
        let reader = named("read")?;
        let observe = named("observe")?;
        let scan_call = call(sources, parsed, entry.span, "scan")?;
        let (callback, lambda) = local(sources, parsed, scan.span, "callback")?;
        let Expression::Lambda { body, .. } = parsed.ast().expressions().get(lambda).unwrap().payload()
        else {
            return Err(failure("lambda callback actual Lambda initializer", lambda));
        };
        let body = *body;
        let node = parsed.ast().statements().get(body).unwrap();
        need(
            matches!(node.payload(), Statement::LambdaBody { .. }),
            "lambda actual LambdaBody, not outer function",
            node,
        )?;
        let (source, birth) = local(sources, parsed, node.span(), "source")?;
        need(
            birth == call(sources, parsed, node.span(), "make")?,
            "lambda named Map initializer exact actual make()",
            birth,
        )?;
        let invoke = call(sources, parsed, scan.span, "callback")?;
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(invoke).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            arguments.is_empty() && !inside(span(parsed, lambda), span(parsed, invoke)),
            "lambda callback invoked outside its own body with no arguments",
            invoke,
        )?;
        let callee = *callee;
        let site = if c.provider {
            let (statement, n, names, source_expr, loop_body) = one(
                parsed
                    .ast()
                    .statements()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Statement::For {
                            binding: ForBinding::Destructuring { names, .. },
                            source,
                            body,
                            ..
                        } if inside(span(parsed, lambda), n.span()) => {
                            Some((id, n, names, *source, *body))
                        }
                        _ => None,
                    }),
                "lambda actual internal For",
            )?;
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("lambda actual K/V declarations", names));
            };
            let read = call(sources, parsed, n.span(), "read")?;
            let Expression::Call { arguments, .. } =
                parsed.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            let [key_arg, value_arg] = arguments.as_slice() else {
                return Err(failure("lambda actual ordered read arguments", arguments));
            };
            Some(Site {
                statement,
                span: n.span(),
                source: source_expr,
                body: loop_body,
                bindings: [*key, *value],
                read,
                arguments: [key_arg.value, value_arg.value],
            })
        } else {
            None
        };
        let observe_call = if c.provider {
            None
        } else {
            Some(call(sources, parsed, span(parsed, lambda), "observe")?)
        };
        let done = call(sources, parsed, span(parsed, lambda), "println")?;
        need(
            span(parsed, birth).end() < span(parsed, done).start()
                && span(parsed, done).end() < span(parsed, lambda).end(),
            "lambda Map birth and done belong to actual thunk body",
            (birth, done, lambda),
        )?;
        Ok(Input {
            entry,
            scan,
            make,
            produce,
            reader,
            observe,
            scan_call,
            callback,
            lambda,
            body,
            invoke,
            callee,
            source,
            birth,
            birth_span: span(parsed, birth),
            site,
            observe_call,
            done,
        })
    }
}
pub(super) mod facts;
pub(super) mod graph;
mod sources {
    //! P15 keeps the original no-capture Unit callback and its internal Map local.
    pub(crate) struct Case {
        pub label: &'static str,
        pub provider: bool,
        pub source: &'static str,
        pub golden: &'static str,
    }
    pub(crate) fn sources() -> [Case; 2] {
        [
            Case {
                label: "P15-for",
                provider: true,
                source: include_str!("../../../tests/fixtures/map-provider-scope-0331/P15-for.ko"),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-scope-0331/P15-for.stdout"
                ),
            },
            Case {
                label: "P15-control",
                provider: false,
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-scope-0331/P15-control.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-scope-0331/P15-control.stdout"
                ),
            },
        ]
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE A2 INPUT label={} rooted={unit} source={:?} golden={:?}",
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
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(c, &sources, &parsed)?;
    let mut report = Report::default();
    let mut proof = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            let checked = facts::unit((c, &sources, &parsed, &input), n, t, o);
            r.query(
                "A2 actual qualified callback/lambda/internal owner/complete cleanup",
                &checked,
            );
            proof = checked.ok();
        },
    );
    diagnostic!(
        "SCOPE A2 LOWER label={} rooted={unit} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    let query = proof
        .as_ref()
        .ok_or_else(|| failure("A2 same-analysis callback/internal cleanup proof", &report))
        .and_then(|p| graph::check(c, &sources, &parsed, &input, p, &program));
    report.query(
        "A2 real FunctionAddress target/thunk/internal current Map/full incoming/protected uses",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "SCOPE A2 FINAL label={} rooted={unit} report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("A2 independent actual identity queries", report))
    }
}
#[test]
fn a2_original_lambda_body_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    for c in sources() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            diagnostic!(
                "SCOPE A2 CASE label={} rooted={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
