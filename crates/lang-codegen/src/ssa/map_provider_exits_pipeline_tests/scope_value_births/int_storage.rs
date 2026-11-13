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
                    } if sources.slice(*name) == Ok("readInt") => {
                        Some((n.span(), *name, parameters))
                    }
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
                matches!(n.payload(),Expression::Return{value:Some(v),..}if *v==consume)
                    .then_some(id)
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
            let Expression::Call { arguments, .. } =
                p.ast().expressions().get(read).unwrap().payload()
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
mod facts {
    //! Original concrete Int facts and exact current owner/drop facts from one analysis.
    mod unit {
        //! Source-qualified concrete Int facts; current owner identity is closed by the SSA query.
        use super::*;
        fn root(n: &ValidatedCompilationUnitNames, span: Span) -> Result<UnitSymbolId, Failure> {
            let (unit, symbol) = one(
                n.names().source_units().iter().flat_map(|u| {
                    u.resolution()
                        .symbols()
                        .iter()
                        .filter(move |s| s.span() == span && !s.is_synthetic())
                        .map(move |s| (u.source_unit(), s.id()))
                }),
                "Int qualified actual original symbol",
            )?;
            one(
                n.names()
                    .references()
                    .iter()
                    .filter_map(|r| match r.target() {
                        UnitReferenceTarget::Symbol(id)
                            if id.source_unit() == unit && id.symbol() == symbol =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter(),
                "Int actual qualified local reference identity",
            )
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::int_storage) fn check(
            input: Input<'_>,
            n: &ValidatedCompilationUnitNames,
            t: &CompilationUnitTypes,
            o: &CompilationUnitOwnership,
        ) -> Result<Proof, Failure> {
            let (c, _, p, i) = input;
            let unit = one(n.names().source_units().iter(), "Int original unit")?.source_unit();
            let e = |id| UnitExpressionId::new(unit, id);
            let decl = |marker| {
                one(
                    n.names()
                        .index()
                        .declarations()
                        .iter()
                        .filter(|d| d.source_unit() == unit && d.name_span() == marker)
                        .map(|d| d.id()),
                    "Int actual original qualified declaration",
                )
            };
            let initial = root(n, i.initial)?;
            let source = root(n, i.source)?;
            let constructed = t
                .map_construction(e(i.construction))
                .ok_or_else(|| failure("Int qualified actual construction", i.construction))?;
            let put = t
                .map_put(e(i.put))
                .ok_or_else(|| failure("Int qualified actual put", i.put))?;
            let consumed = t
                .map_consume(e(i.consume))
                .ok_or_else(|| failure("Int qualified actual consume", i.consume))?;
            let int = constructed.key_type();
            let mutable = constructed.map_type();
            let map = consumed.result_type();
            need(
                constructed.expression() == e(i.construction)
                    && constructed.callable() == IntrinsicCallable::MutableMapOf
                    && constructed.value_type() == int
                    && t.types().get(int) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    && t.copyability(int) == Copyability::Copyable
                    && t.symbol_type(initial) == Some(mutable)
                    && o.formed_owned_locals().contains(&initial)
                    && put.expression() == e(i.put)
                    && put.receiver() == e(receiver(p, i.put))
                    && put.key() == e(i.payload[0])
                    && put.value() == e(i.payload[1])
                    && put.key_type() == int
                    && put.value_type() == int
                    && i.payload
                        .iter()
                        .all(|id| t.expression_type(e(*id)) == Some(int))
                    && consumed.expression() == e(i.consume)
                    && consumed.receiver() == e(receiver(p, i.consume))
                    && consumed.source() == Some((initial, mutable))
                    && consumed.receiver_type() == mutable
                    && consumed.key_type() == int
                    && consumed.value_type() == int
                    && t.expression_type(e(i.consume)) == Some(map)
                    && t.types().get(map)
                        == Some(&UnitTypeKind::Intrinsic {
                            constructor: IntrinsicTypeConstructor::Map,
                            arguments: vec![int, int],
                        }),
                "Int qualified exact Copy construction/put(1,2)/consume storage",
                (constructed, put, consumed),
            )?;
            for id in [put.receiver(), consumed.receiver()] {
                let r = one(
                    n.names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::span(p, id.expression())),
                    "Int qualified factory receiver",
                )?;
                need(
                    r.target() == &UnitReferenceTarget::Symbol(initial),
                    "Int qualified put/consume current same m root",
                    r,
                )?;
            }
            need(
                !o.drops().iter().any(|d| {
                    d.target() == UnitDropTarget::Named(initial)
                        || d.target() == UnitDropTarget::Temporary(e(i.consume))
                }),
                "Int qualified transferred initial/return not duplicated cleanup",
                o.drops(),
            )?;
            let call = t
                .call(e(i.birth))
                .ok_or_else(|| failure("Int qualified actual makeInt Call", i.birth))?;
            need(
                call.target() == UnitCallTarget::Declaration(decl(i.make_name)?)
                    && call.arguments().is_empty()
                    && call.return_type() == map
                    && call.result_source() == CallableResultSource::Owned
                    && call.borrow_return().is_none()
                    && t.symbol_type(source) == Some(map)
                    && o.formed_owned_locals().contains(&source)
                    && !o
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Temporary(e(i.birth))),
                "Int qualified exact concrete named source birth",
                call,
            )?;
            let reader = t
                .signatures()
                .declaration(decl(i.reader_name)?)
                .and_then(|d| d.callable())
                .ok_or_else(|| failure("Int qualified actual reader signature", i.reader_name))?;
            need(
                reader.parameters().len() == 2
                    && reader
                        .parameters()
                        .iter()
                        .all(|a| a.mode() == ParameterMode::Borrow && a.ty() == int)
                    && reader.parameters()[0].symbol() == Some(root(n, i.reader_params[0])?)
                    && reader.parameters()[1].symbol() == Some(root(n, i.reader_params[1])?),
                "Int qualified actual declared reader parameter ABI/Copy types",
                reader,
            )?;
            for index in 0..2 {
                let r = one(
                    n.names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::span(p, i.compared[index])),
                    "Int qualified actual compared parameter",
                )?;
                need(
                    r.target() == &UnitReferenceTarget::Symbol(root(n, i.reader_params[index])?)
                        && t.expression_type(e(i.compared[index])) == Some(int)
                        && t.expression_type(e(i.literals[index])) == Some(int)
                        && t.expression_type(e(i.compares[index])).is_some_and(|ty| {
                            t.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                        }),
                    "Int qualified original key/value scalar comparisons",
                    r,
                )?;
            }
            let scanner = t
                .call(e(i.scanner_call))
                .ok_or_else(|| failure("Int qualified scanner entry", i.scanner_call))?;
            need(
                scanner.target() == UnitCallTarget::Declaration(decl(i.scan_name)?)
                    && scanner.arguments().is_empty(),
                "Int qualified actual scanner selected",
                scanner,
            )?;
            let drop = one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(source)),
                "Int qualified unique current source Drop",
            )?;
            need(
                drop.value_origin() == i.source
                    && drop.point()
                        == if let Some(site) = &i.site {
                            UnitDropPoint::LoopExit(UnitStatementId::new(unit, site.statement))
                        } else {
                            UnitDropPoint::CallReturn(e(i.size.unwrap()))
                        },
                "Int qualified current source cleanup identity/endpoint",
                drop,
            )?;
            need(
                t.map_iterations().len() == usize::from(c.provider)
                    && o.map_iterations().len() == usize::from(c.provider),
                "Int qualified exact provider/control set",
                o.map_iterations(),
            )?;
            if let Some(site) = &i.site {
                let statement = UnitStatementId::new(unit, site.statement);
                let plan = o
                    .map_iteration(statement)
                    .ok_or_else(|| failure("Int qualified actual For plan", site))?;
                let d = plan.descriptor();
                let r = one(
                    n.names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::span(p, site.source)),
                    "Int qualified actual For source Name",
                )?;
                need(
                    t.map_iteration(statement) == Some(d)
                        && d.source() == e(site.source)
                        && d.source_type() == map
                        && d.key_type() == int
                        && d.value_type() == int
                        && r.target() == &UnitReferenceTarget::Symbol(source)
                        && plan.source_access() == UnitIterationSourceAccess::Owned
                        && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==source),
                    "Int qualified original owned source Int storage",
                    plan,
                )?;
                let read = t
                    .call(e(site.read))
                    .ok_or_else(|| failure("Int qualified actual read Call", site.read))?;
                need(
                    read.target() == UnitCallTarget::Declaration(decl(i.reader_name)?)
                        && read.arguments().len() == 2,
                    "Int qualified actual read target",
                    read,
                )?;
                for index in 0..2 {
                    let binding = root(n, site.bindings[index])?;
                    let component = &plan.loans()[index];
                    let loan = one(
                        o.loans().iter().filter(|l| {
                            l.call() == e(site.read) && l.argument() == e(site.arguments[index])
                        }),
                        "Int qualified actual component call loan",
                    )?;
                    need(
                        d.bindings()[index] == binding
                            && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                            && component.binding() == binding
                            && component.statement() == statement
                            && component.source() == e(site.source)
                            && Some(component.origin()) == plan.source()
                            && component.kind() == LoanKind::Shared
                            && read.arguments()[index].argument_index() == index
                            && read.arguments()[index].parameter_index() == index
                            && read.arguments()[index].mode() == ParameterMode::Borrow
                            && read.arguments()[index].parameter_type() == int
                            && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                            && loan.kind() == LoanKind::Shared
                            && loan.end_span() == ast::span(p, site.read),
                        "Int qualified exact projection/reader delivery/loan bounds",
                        (component, loan),
                    )?;
                }
                for (kind, point, mut actions) in [
                    (
                        UnitIterationExitKind::Fallthrough,
                        UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                        vec![
                            UnitIterationCleanupAction::EndBinding {
                                statement,
                                symbol: d.value_binding(),
                            },
                            UnitIterationCleanupAction::EndBinding {
                                statement,
                                symbol: d.key_binding(),
                            },
                        ],
                    ),
                    (
                        UnitIterationExitKind::Exhaustion,
                        UnitDropPoint::LoopExit(statement),
                        vec![
                            UnitIterationCleanupAction::FinishProvider(statement),
                            UnitIterationCleanupAction::EndSource(statement),
                        ],
                    ),
                ] {
                    actions.extend(
                        o.drops()
                            .iter()
                            .filter(|d| d.point() == point)
                            .copied()
                            .map(UnitIterationCleanupAction::Drop),
                    );
                    let exit = one(
                        plan.exits().iter().filter(|e| e.kind() == kind),
                        "Int qualified actual normal exit",
                    )?;
                    need(
                        plan.exits().len() == 2
                            && exit.point() == point
                            && exit.actions() == actions
                            && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                        "Int qualified complete ordered provider cleanup",
                        exit,
                    )?;
                }
            } else {
                let size = t
                    .map_size(e(i.size.unwrap()))
                    .ok_or_else(|| failure("Int qualified real MapSize", i.size))?;
                need(
                    size.map_type() == map && t.expression_type(size.receiver()) == Some(map),
                    "Int qualified control uses actual named Map size",
                    size,
                )?;
            }
            Ok(Proof {
                drop: drop.value_origin(),
            })
        }
    }
    use super::super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, ast};
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
        ownership_checking::*,
        parser::{Expression, ParsedFile},
        source::{SourceMap, Span},
        type_checking::*,
    };
    #[derive(Debug)]
    pub(super) struct Proof {
        pub drop: Span,
    }
    type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);

    fn receiver(
        p: &ParsedFile,
        id: lang_frontend::ast::ExpressionId,
    ) -> lang_frontend::ast::ExpressionId {
        let Expression::Call { callee, .. } = p.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            p.ast().expressions().get(*callee).unwrap().payload()
        else {
            unreachable!()
        };
        *receiver
    }
    pub(super) use unit::check as unit;
}
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
                source: include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/map-provider-scope-0331/P06-for.ko"
                )),
                golden: include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/map-provider-scope-0331/P06-for.stdout"
                )),
            },
            Case {
                label: "P06-control",
                provider: false,
                source: include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/map-provider-scope-0331/P06-control.ko"
                )),
                golden: include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/map-provider-scope-0331/P06-control.stdout"
                )),
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
