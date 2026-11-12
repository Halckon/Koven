//! Argument-bearing ordinary factories form the actual named provider owner.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod ast {
    //! Select the actual Call initializer, lexical scope, parameter syntax and For use independently.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker,
            ParsedFile, Statement,
        },
        source::{SourceMap, Span},
        type_checking::ParameterMode,
    };
    #[derive(Debug)]
    pub(super) struct Parameter {
        pub name: Span,
        pub span: Span,
        pub mode: ParameterMode,
    }
    #[derive(Debug)]
    pub(super) struct Function {
        pub span: Span,
        pub name: Span,
        pub body: StatementId,
        pub parameters: Vec<Parameter>,
    }
    #[derive(Debug)]
    pub(super) struct Argument {
        pub span: Span,
        pub expression: ExpressionId,
        pub parameter: usize,
    }
    #[derive(Debug)]
    pub(super) struct Input {
        pub scope: Function,
        pub factory: Function,
        pub reader: Function,
        pub birth_span: Span,
        pub call: ExpressionId,
        pub arguments: Vec<Argument>,
        pub root: Option<Span>,
        pub statement: StatementId,
        pub span: Span,
        pub source: ExpressionId,
        pub body: StatementId,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub read_arguments: [ExpressionId; 2],
        pub after: Option<Span>,
    }
    pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn within(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    fn function(
        parsed: &ParsedFile,
        node: &lang_frontend::ast::AstNode<Item>,
    ) -> Result<Function, Failure> {
        let Item::Function {
            name: NameMarker::Present(name),
            form,
            parameters,
            ..
        } = node.payload()
        else {
            return Err(failure("local argument original function", node));
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return Err(failure("local argument actual block", form)),
        };
        let parameters = parameters
            .iter()
            .map(|p| {
                let NameMarker::Present(name) = p.name else {
                    return Err(failure("local argument original parameter", p));
                };
                let mode = match p.mode_marker {
                    None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
                    Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
                    _ => return Err(failure("local argument parameter mode", p)),
                };
                Ok(Parameter {
                    name,
                    span: p.span,
                    mode,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let _ = parsed;
        Ok(Function {
            span: node.span(),
            name: *name,
            body,
            parameters,
        })
    }
    pub(super) fn input(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<Input, Failure> {
        let (statement, node) = one(
            parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, n)| matches!(n.payload(), Statement::For { .. })),
            "local argument sole original For",
        )?;
        let Statement::For {
            source,
            body,
            binding: ForBinding::Destructuring { names, .. },
            ..
        } = node.payload()
        else {
            return Err(failure("local argument actual destructuring", node));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("local argument actual K/V", names));
        };
        let scope_node = one(
            parsed
                .roots()
                .iter()
                .filter_map(|id| parsed.ast().items().get(*id).ok())
                .filter(|n| {
                    matches!(n.payload(), Item::Function { .. }) && within(n.span(), node.span())
                }),
            "local argument actual enclosing callable",
        )?;
        let scope = function(parsed, scope_node)?;
        let (root, call) = if case.direct {
            (None, *source)
        } else {
            need(
                matches!(
                    parsed.ast().expressions().get(*source).unwrap().payload(),
                    Expression::Name
                ) && sources.slice(span(parsed, *source)) == Ok("source"),
                "local argument For names actual local",
                source,
            )?;
            let declaration = one(
                parsed
                    .ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if within(scope.span, n.span()) && sources.slice(*name) == Ok("source") => {
                            Some((*name, *initializer))
                        }
                        _ => None,
                    }),
                "local argument actual current local initializer",
            )?;
            need(
                span(parsed, declaration.1).end() < node.span().start(),
                "local argument birth precedes For",
                declaration,
            )?;
            (Some(declaration.0), declaration.1)
        };
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(call).unwrap().payload()
        else {
            return Err(failure("local argument actual birth Call", call));
        };
        let wanted = if case.route { "other" } else { "produce" };
        need(
            sources
                .slice(span(parsed, *callee))
                .is_ok_and(|s| s.ends_with(wanted)),
            "local argument original callable spelling",
            callee,
        )?;
        let class = if case.route {
            Some(one(parsed.roots().iter().filter_map(|id| parsed.ast().items().get(*id).ok()).filter(|n| matches!(n.payload(), Item::Classifier(c) if matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Factory")))), "local argument actual Factory")?.span())
        } else {
            None
        };
        let factory_node = one(
            parsed.ast().items().iter().filter_map(|(_, n)| {
                let Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (sources.slice(*name) == Ok(wanted)
                    && parameters.len() == arguments.len()
                    && class.is_none_or(|scope| within(scope, n.span()))
                    && !parameters.iter().any(|p| {
                        sources.slice(parsed.ast().type_refs().get(p.type_ref).unwrap().span())
                            == Ok("Boolean")
                    }))
                .then_some(n)
            }),
            "local argument exact selected source implementation",
        )?;
        let factory = function(parsed, factory_node)?;
        let arguments = arguments
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let slot = if let Some(prefix) = a.named_prefix {
                    one(
                        factory.parameters.iter().enumerate().filter_map(|(i, p)| {
                            (sources.slice(p.name) == sources.slice(prefix.name_span)).then_some(i)
                        }),
                        "local argument original named slot",
                    )?
                } else {
                    i
                };
                need(
                    a.mode_marker.is_none(),
                    "local argument ordinary argument syntax",
                    a,
                )?;
                Ok(Argument {
                    span: a.span,
                    expression: a.value,
                    parameter: slot,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let reader_node = one(parsed.roots().iter().filter_map(|id| parsed.ast().items().get(*id).ok()).filter(|n| matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("read"))), "local argument actual reader declaration")?;
        let reader = function(parsed, reader_node)?;
        let read = one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { callee, .. }
                        if within(node.span(), n.span())
                            && sources.slice(span(parsed, *callee)) == Ok("read") =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "local argument actual reader Call",
        )?;
        let Expression::Call {
            arguments: read_args,
            ..
        } = parsed.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            read_args.len() == 2,
            "local argument real reader arity",
            read_args,
        )?;
        let after = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, n)| {
                (within(scope.span, n.span()) && sources.slice(n.span()) == Ok("println(\"after\")"))
                    .then_some(n.span())
            })
            .next();
        Ok(Input {
            birth_span: span(parsed, call),
            scope,
            factory,
            reader,
            call,
            arguments,
            root,
            statement,
            span: node.span(),
            source: *source,
            body: *body,
            bindings: [*key, *value],
            read,
            read_arguments: [read_args[0].value, read_args[1].value],
            after,
        })
    }
}
mod facts;
mod generic;
mod graph;
mod sources {
    //! New source derivations leave every already accepted temporary/ordinary input untouched.
    use super::super::{temporary_argument_cfg_runtime, temporary_delivery};

    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
        pub mutable: bool,
        pub resource: bool,
        pub unit_only: bool,
        pub direct: bool,
        pub route: bool,
        pub delegated: bool,
    }
    fn named(text: &str) -> String {
        let prefix = "for((key,value) in ";
        let start = text.find(prefix).unwrap() + prefix.len();
        let end = text[start..]
            .find("){read(key,value)}")
            .map(|i| start + i)
            .unwrap();
        let call = &text[start..end];
        text.replacen(
            &format!("{prefix}{call})"),
            &format!("val source={call};for((key,value) in source)"),
            1,
        )
    }

    pub(crate) fn sources() -> Vec<Case> {
        let mut out = super::temporary_arguments::sources()
            .into_iter()
            .map(|case| {
                // These are the exact two first-red source/golden bytes.
                let old = "for((key,value) in produce(argument()))";
                assert_eq!(case.source.matches(old).count(), 1);
                Case {
                    label: case
                        .label
                        .replacen("temporary-argument", "local-argument", 1),
                    source: case.source.replace(
                        old,
                        "val source=produce(argument());for((key,value) in source)",
                    ),
                    golden: case.golden,
                    mutable: case.mutable,
                    resource: false,
                    unit_only: false,
                    direct: false,
                    route: false,
                    delegated: false,
                }
            })
            .collect::<Vec<_>>();
        for case in temporary_delivery::sources() {
            let resource = !matches!(case.delivery, temporary_delivery::Delivery::SameMapBorrow);
            let golden = if resource {
                let old = "deinit\nowned-drop\nafter\n";
                assert!(case.golden.ends_with(old));
                case.golden.strip_suffix(old).unwrap().to_owned() + "after\ndeinit\nowned-drop\n"
            } else {
                case.golden
            };
            out.push(Case {
                label: case
                    .label
                    .replacen("temporary-delivery", "local-delivery", 1),
                source: named(&case.source),
                golden,
                mutable: case.mutable,
                resource,
                unit_only: false,
                direct: false,
                route: false,
                delegated: false,
            });
        }
        for case in temporary_argument_cfg_runtime::sources()
            .into_iter()
            .filter(|c| c.label.ends_with("control-false"))
        {
            let mutable = case.label.contains("MutableMap");
            out.push(Case {
                label: case
                    .label
                    .replacen("temporary-argument", "local-argument", 1),
                source: named(&case.source),
                golden: case.golden,
                mutable,
                resource: false,
                unit_only: false,
                direct: false,
                route: false,
                delegated: false,
            });
        }
        // Use the original ordinary-route declarations; the selected nonzero-arg
        // callable itself supplies the current owner, without an intervening make().
        for (label, source, _) in super::super::local_ordinary_route_sources()
            .into_iter()
            .filter(|(l, _, _)| l.ends_with("previous-epoch"))
        {
            let prefix = source.split("fun entry():Unit").next().unwrap();
            let route = label
                .strip_prefix("ordinary-")
                .unwrap()
                .strip_suffix("-previous-epoch")
                .unwrap();
            let callee = if route == "delegation" {
                "Host(Factory()).other"
            } else {
                "Factory().other"
            };
            for direct in [false, true] {
                let provider = if direct {
                    format!("for((key,value) in {callee}(1))")
                } else {
                    format!("val source={callee}(1);for((key,value) in source)")
                };
                let drop = "other-drop\n";
                let (before, after) = if direct { (drop, "") } else { ("", drop) };
                out.push(Case {
                    label: format!("local-argument-route-{route}-{}", if direct { "direct-temporary" } else { "current-owner" }),
                    source: format!("{prefix}fun entry():Unit{{println(\"begin\");{provider}{{read(key,value)}};println(\"after\")}}\n"),
                    golden: format!("begin\n{route}-call\nother-start\nother-ready\nother-key\nresource-borrow\n{before}after\n{after}"),
                    mutable: false, resource: true, unit_only: true, direct, route: true, delegated: route == "delegation",
                });
            }
        }
        assert_eq!(out.len(), 18);
        out
    }
}
use super::fixture;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms, temporary_arguments,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) use generic::{
    native_sources as generic_sources, prerequisite_sources as generic_prerequisite_sources,
};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "LOCAL ARG INPUT label={} unit={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(case, &sources, &parsed)?;
    let mut report = Report::default();
    let mut proof = None;
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
                let checked = facts::unit(case, &sources, &parsed, &input, n, t, o);
                r.query(
                    "local argument exact qualified Call, delivery, birth and provider owner",
                    &checked,
                );
                if case.label.starts_with("generic-wrapper-") {
                    r.query(
                        "generic named wrapper qualified forwarding",
                        &generic::wrapper::unit(&sources, &parsed, n, t, o),
                    );
                }
                proof = Some(checked);
            },
        )
    } else {
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            let checked = facts::unit(case, &sources, &parsed, &input, n, t, o);
            r.query(
                "local argument exact qualified Call, delivery, birth and provider owner",
                &checked,
            );
            if case.label.starts_with("generic-wrapper-") {
                r.query(
                    "generic named wrapper qualified forwarding",
                    &generic::wrapper::unit(&sources, &parsed, n, t, o),
                );
            }
            proof = Some(checked);
        })
    };
    diagnostic!(
        "LOCAL ARG LOWER label={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    let proof = proof.ok_or_else(|| failure("local argument observer", "not called"))??;
    report.query(
        "local argument actual SSA current owner, complete incoming and protected uses",
        &graph::check(case, &sources, &parsed, &input, &proof, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("local argument independent queries", report));
    }
    Ok(())
}

#[test]
fn selected_argument_factory_forms_named_owner_in_both_pipelines() {
    let cases = sources();
    assert_eq!(cases.len(), 18);
    let mut outcomes = Vec::new();
    for case in cases {
        for unit in if case.unit_only {
            vec![true]
        } else {
            vec![false, true]
        } {
            let result = observe(&case, unit);
            diagnostic!(
                "LOCAL ARG CASE label={} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(outcomes.len(), 30);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
