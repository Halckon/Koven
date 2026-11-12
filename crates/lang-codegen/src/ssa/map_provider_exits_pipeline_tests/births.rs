//! Ordinary intrinsic births and Unit FunctionValue results reach the original providers.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(super) mod ast {
    //! Original creation/consume/invoke syntax, owner declaration and actual provider site.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, Kind, Shape};
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
        },
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Site {
        pub statement: StatementId,
        pub span: Span,
        pub source: ExpressionId,
        pub body: StatementId,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub arguments: [ExpressionId; 2],
    }
    #[derive(Debug)]
    pub(super) struct Input {
        pub birth_span: Span,
        pub entry: Span,
        pub reader: Span,
        pub birth: ExpressionId,
        pub root: Option<Span>,
        pub construction: Option<ExpressionId>,
        pub initial: Option<Span>,
        pub put: Option<ExpressionId>,
        pub lambda: Option<ExpressionId>,
        pub callee: Option<ExpressionId>,
        pub factory: Option<Span>,
        pub produce: Option<ExpressionId>,
        pub argument: Option<ExpressionId>,
        pub site: Option<Site>,
        pub size: Option<ExpressionId>,
        pub after: Span,
    }
    pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn inside(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    fn named_function(
        sources: &SourceMap,
        parsed: &ParsedFile,
        name: &str,
    ) -> Result<(Span, StatementId), Failure> {
        let n=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(marker),..}if sources.slice(*marker)==Ok(name))),"birth original top-level function")?;
        let Item::Function { form, .. } = n.payload() else {
            unreachable!()
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return Err(failure("birth original callable block", n)),
        };
        Ok((n.span(), body))
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
            "birth actual source Call",
        )
    }
    fn local(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        name: &str,
    ) -> Result<(Span, ExpressionId), Failure> {
        one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer,
                        ..
                    } if inside(scope, n.span()) && sources.slice(*marker) == Ok(name) => {
                        Some((*marker, *initializer))
                    }
                    _ => None,
                }),
            "birth actual local declaration",
        )
    }
    pub(super) fn input(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<Input, Failure> {
        let (entry, _body) = named_function(sources, parsed, "entry")?;
        let (reader, _) = named_function(sources, parsed, "read")?;
        let (root, birth) = if case.shape == Shape::Direct {
            (
                None,
                call(
                    sources,
                    parsed,
                    entry,
                    match case.kind {
                        Kind::FunctionValue => "factory",
                        Kind::Consume => "initial.consume",
                        _ => {
                            if case.mutable {
                                "mutableMapOf"
                            } else {
                                "mapOf"
                            }
                        }
                    },
                )?,
            )
        } else {
            let (marker, value) = local(sources, parsed, entry, "source")?;
            (Some(marker), value)
        };
        let (construction, initial, put) = match case.kind {
            Kind::New => (Some(birth), None, None),
            Kind::Consume => {
                let (marker, value) = local(sources, parsed, entry, "initial")?;
                (
                    Some(value),
                    Some(marker),
                    Some(call(sources, parsed, entry, "initial.put")?),
                )
            }
            Kind::Updated => (
                Some(birth),
                root,
                Some(call(sources, parsed, entry, "source.put")?),
            ),
            Kind::FunctionValue => (None, None, None),
        };
        let (lambda, callee, factory, produce, argument) = if case.kind == Kind::FunctionValue {
            let (marker, lambda) = local(sources, parsed, entry, "factory")?;
            need(
                matches!(
                    parsed.ast().expressions().get(lambda).unwrap().payload(),
                    Expression::Lambda { .. }
                ),
                "birth actual callable is original lambda",
                lambda,
            )?;
            let Expression::Call {
                callee, arguments, ..
            } = parsed.ast().expressions().get(birth).unwrap().payload()
            else {
                return Err(failure("birth actual function-value Call", birth));
            };
            need(
                arguments.is_empty() && sources.slice(span(parsed, *callee)) == Ok("factory"),
                "birth actual zeroarg function-value use",
                birth,
            )?;
            (
                Some(lambda),
                Some(*callee),
                Some(marker),
                Some(call(sources, parsed, span(parsed, lambda), "produce")?),
                Some(call(sources, parsed, span(parsed, lambda), "argument")?),
            )
        } else {
            (None, None, None, None, None)
        };
        let loops = parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
            .collect::<Vec<_>>();
        need(
            loops.len() == usize::from(case.shape != Shape::Control),
            "birth exact original provider/control count",
            &loops,
        )?;
        let site = if let Some((statement, n)) = loops.first() {
            let Statement::For {
                source,
                body,
                binding: ForBinding::Destructuring { names, .. },
                ..
            } = n.payload()
            else {
                return Err(failure("birth destructuring For", n));
            };
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("birth actual K/V syntax", names));
            };
            need(
                if case.shape == Shape::Direct {
                    *source == birth
                } else {
                    sources.slice(span(parsed, *source)) == Ok("source")
                        && matches!(
                            parsed.ast().expressions().get(*source).unwrap().payload(),
                            Expression::Name
                        )
                },
                "birth For uses actual result or original local",
                source,
            )?;
            let read = call(sources, parsed, n.span(), "read")?;
            let Expression::Call { arguments, .. } =
                parsed.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            let [a, b] = arguments.as_slice() else {
                return Err(failure("birth original reader arguments", arguments));
            };
            Some(Site {
                statement: *statement,
                span: n.span(),
                source: *source,
                body: *body,
                bindings: [*key, *value],
                read,
                arguments: [a.value, b.value],
            })
        } else {
            None
        };
        let size = if case.shape == Shape::Control {
            Some(one(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Expression::Member { name_span, .. }
                            if inside(entry, n.span()) && sources.slice(*name_span) == Ok("size") =>
                        {
                            Some(id)
                        }
                        _ => None,
                    }),
                "birth original no-For size",
            )?)
        } else {
            None
        };
        let after = one(
            parsed.ast().expressions().iter().filter_map(|(_, n)| {
                (inside(entry, n.span()) && sources.slice(n.span()) == Ok("println(\"after\")"))
                    .then_some(n.span())
            }),
            "birth original after marker",
        )?;
        Ok(Input {
            birth_span: span(parsed, birth),
            entry,
            reader,
            birth,
            root,
            construction,
            initial,
            put,
            lambda,
            callee,
            factory,
            produce,
            argument,
            site,
            size,
            after,
        })
    }
}
pub(in crate::ssa) mod facts;
pub(super) mod graph;
mod sources {
    //! Frozen actual attribution inputs; Single FunctionValue stays a separate 0336 goal.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Kind {
        New,
        Consume,
        FunctionValue,
        Updated,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Shape {
        Control,
        Named,
        Direct,
    }
    pub(crate) struct Case {
        pub label: &'static str,
        pub source: &'static str,
        pub golden: &'static str,
        pub kind: Kind,
        pub shape: Shape,
        pub mutable: bool,
    }
    pub(crate) fn sources() -> Vec<Case> {
        vec![
            Case {
                label: "function-value-Map-no-for",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-no-for.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-no-for.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Control,
                mutable: false,
            },
            Case {
                label: "function-value-Map-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-named.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Named,
                mutable: false,
            },
            Case {
                label: "function-value-Map-direct",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-direct.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-direct.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Direct,
                mutable: false,
            },
            Case {
                label: "function-value-MutableMap-no-for",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-no-for.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-no-for.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Control,
                mutable: true,
            },
            Case {
                label: "function-value-MutableMap-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-named.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Named,
                mutable: true,
            },
            Case {
                label: "function-value-MutableMap-direct",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-direct.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-direct.stdout"
                ),
                kind: Kind::FunctionValue,
                shape: Shape::Direct,
                mutable: true,
            },
            Case {
                label: "intrinsic-empty-map-no-for",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-no-for.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-no-for.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Control,
                mutable: false,
            },
            Case {
                label: "intrinsic-empty-map-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-named.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Named,
                mutable: false,
            },
            Case {
                label: "intrinsic-empty-map-direct",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-direct.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-direct.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Direct,
                mutable: false,
            },
            Case {
                label: "intrinsic-mutable-map-no-for",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-no-for.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-no-for.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Control,
                mutable: true,
            },
            Case {
                label: "intrinsic-mutable-map-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-named.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Named,
                mutable: true,
            },
            Case {
                label: "intrinsic-mutable-map-direct",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-direct.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-direct.stdout"
                ),
                kind: Kind::New,
                shape: Shape::Direct,
                mutable: true,
            },
            Case {
                label: "intrinsic-consumed-map-no-for",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-no-for.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-no-for.stdout"
                ),
                kind: Kind::Consume,
                shape: Shape::Control,
                mutable: false,
            },
            Case {
                label: "intrinsic-consumed-map-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-named.stdout"
                ),
                kind: Kind::Consume,
                shape: Shape::Named,
                mutable: false,
            },
            Case {
                label: "intrinsic-consumed-map-direct",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-direct.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-direct.stdout"
                ),
                kind: Kind::Consume,
                shape: Shape::Direct,
                mutable: false,
            },
            Case {
                label: "intrinsic-mutable-map-put-current-named",
                source: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-put-current-named.ko"
                ),
                golden: include_str!(
                    "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-put-current-named.stdout"
                ),
                kind: Kind::Updated,
                shape: Shape::Named,
                mutable: true,
            },
        ]
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, Kind, Shape, sources};
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "BIRTH INPUT label={} rooted={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(case, &sources, &parsed)?;
    let mut report = Report::default();
    let mut proof = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            let checked = facts::unit((case, &sources, &parsed, &input), n, t, o);
            r.query(
                "birth same-analysis qualified actual creation/consume/invoke and owner",
                &checked,
            );
            proof = Some(checked);
        },
    );
    diagnostic!(
        "BIRTH LOWER label={} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit temporary Entry keeps outer source and actual producer/owner transport",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    let proof = proof.ok_or_else(|| failure("birth actual analysis observer", "not called"))??;
    report.query(
        "birth actual SSA producer/current owner/full incoming/protected uses/Drop",
        &graph::check(case, &sources, &parsed, &input, &proof, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "BIRTH FINAL label={} rooted={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("birth independent FE/SSA proof", report))
    }
}
#[test]
fn original_intrinsic_and_unit_function_value_births_keep_exact_owner_identity() {
    let cases = sources();
    assert_eq!(cases.len(), 16);
    // These six FunctionValue sources retain their original rooted-only cells.
    // Do not add Memory cells or recast the old Single prerequisite as a rejection oracle.
    for c in cases.iter().filter(|c| c.kind == Kind::FunctionValue) {
        diagnostic!(
            "BIRTH SINGLE FUNCTION PREREQUISITE label={} source={:?} golden={:?}",
            c.label,
            c.source,
            c.golden
        );
    }
    let mut outcomes = Vec::new();
    for case in cases {
        for unit in if case.kind == Kind::FunctionValue {
            vec![true]
        } else {
            vec![false, true]
        } {
            let result = observe(&case, unit);
            diagnostic!(
                "BIRTH CASE label={} rooted={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 26);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
