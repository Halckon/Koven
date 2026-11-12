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
                        } if inside(scan.span, n.span()) && *initializer == conditional => Some(*name),
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
                        if inside(node.span(), n.span()) && id != i.births[0] && id != i.births[1] =>
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
mod facts;
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
