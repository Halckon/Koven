//! A real selected factory takes an evaluated argument through the ordinary call ABI.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(super) mod ast {
    //! The original Int overload and nested argument Call, independent of arena positions.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
        source::{SourceMap, Span},
    };

    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Function {
        pub span: Span,
        pub name: Span,
        pub parameter: Option<Span>,
    }

    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Loop {
        pub statement: StatementId,
        pub span: Span,
        pub body: StatementId,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub arguments: [ExpressionId; 2],
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Input {
        pub entry: Function,
        pub argument: Function,
        pub factory: Function,
        pub rejected: Function,
        pub reader: Function,
        pub produce: ExpressionId,
        pub value: ExpressionId,
        pub loop_site: Option<Loop>,
        pub size: Option<Span>,
        pub after: Span,
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn span(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn inside(inner: Span, outer: Span) -> bool {
        inner.source_id() == outer.source_id()
            && inner.start() > outer.start()
            && inner.end() < outer.end()
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn function(
        sources: &SourceMap,
        parsed: &ParsedFile,
        wanted: &str,
        parameter: Option<&str>,
    ) -> Result<Function, Failure> {
        one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                let Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                if sources.slice(*name) != Ok(wanted) {
                    return None;
                }
                let marker = match (parameter, parameters.as_slice()) {
                    (None, []) => None,
                    (Some(wanted), [p])
                        if p.mode_marker.is_none()
                            && sources.slice(parsed.ast().type_refs().get(p.type_ref).ok()?.span())
                                == Ok(wanted) =>
                    {
                        let NameMarker::Present(name) = p.name else {
                            return None;
                        };
                        Some(name)
                    }
                    _ => return None,
                };
                Some(Function {
                    span: node.span(),
                    name: *name,
                    parameter: marker,
                })
            }),
            "argument factory original function and parameter syntax",
        )
    }

    fn calls(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        name: &str,
    ) -> Result<ExpressionId, Failure> {
        one(
            parsed.ast().expressions().iter().filter_map(|(id, node)| {
                let Expression::Call { callee, .. } = node.payload() else {
                    return None;
                };
                (inside(node.span(), scope) && sources.slice(span(parsed, *callee)) == Ok(name))
                    .then_some(id)
            }),
            "argument factory actual unique source Call",
        )
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn input(
        sources: &SourceMap,
        parsed: &ParsedFile,
        control: bool,
    ) -> Result<Input, Failure> {
        let entry = function(sources, parsed, "entry", None)?;
        let argument = function(sources, parsed, "argument", None)?;
        let factory = function(sources, parsed, "produce", Some("Int"))?;
        let rejected = function(sources, parsed, "produce", Some("Boolean"))?;
        let reader = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                matches!(node.payload(), Item::Function {name:NameMarker::Present(name), parameters, ..}
                if sources.slice(*name) == Ok("read") && parameters.len() == 2)
                .then(|| {
                    let Item::Function {
                        name: NameMarker::Present(name),
                        ..
                    } = node.payload()
                    else {
                        unreachable!()
                    };
                    Function {
                        span: node.span(),
                        name: *name,
                        parameter: None,
                    }
                })
            }),
            "argument factory actual component reader",
        )?;
        let produce = calls(sources, parsed, entry.span, "produce")?;
        let value = calls(sources, parsed, entry.span, "argument")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(produce).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            matches!(arguments.as_slice(), [argument] if argument.value == value && argument.mode_marker.is_none() && argument.named_prefix.is_none())
                && matches!(parsed.ast().expressions().get(value).unwrap().payload(), Expression::Call {arguments, ..} if arguments.is_empty()),
            "argument factory source evaluates actual nested zeroarg Call",
            produce,
        )?;
        let loops = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Statement::For { .. }).then_some((id, node))
            })
            .collect::<Vec<_>>();
        need(
            loops.len() == usize::from(!control),
            "argument factory actual provider/control distinction",
            &loops,
        )?;
        let loop_site = if control {
            None
        } else {
            let (statement, node) = loops[0];
            let Statement::For {
                binding: ForBinding::Destructuring { names, .. },
                source,
                body,
                ..
            } = node.payload()
            else {
                return Err(failure("argument factory original destructuring For", node));
            };
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("argument factory K/V markers", names));
            };
            let read = calls(sources, parsed, node.span(), "read")?;
            let Expression::Call { arguments, .. } =
                parsed.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            need(
                *source == produce && arguments.len() == 2,
                "argument factory For source is the selected nested Call",
                node,
            )?;
            Some(Loop {
                statement,
                span: node.span(),
                body: *body,
                bindings: [*key, *value],
                read,
                arguments: [arguments[0].value, arguments[1].value],
            })
        };
        let size = if control {
            Some(one(
                parsed.ast().expressions().iter().filter_map(|(_, node)| {
                    (inside(node.span(), entry.span)
                        && matches!(node.payload(), Expression::Member {name_span, ..}
                if sources.slice(*name_span) == Ok("size")))
                    .then_some(node.span())
                }),
                "argument factory control original size",
            )?)
        } else {
            None
        };
        let after = one(parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::Call {callee, arguments, ..} = node.payload() else { return None; };
            (inside(node.span(), entry.span) && sources.slice(span(parsed, *callee)) == Ok("println")
                && matches!(arguments.as_slice(), [argument] if sources.slice(span(parsed, argument.value)) == Ok("\"after\"")))
                .then_some(node.span())
        }), "argument factory original after marker")?;
        Ok(Input {
            entry,
            argument,
            factory,
            rejected,
            reader,
            produce,
            value,
            loop_site,
            size,
            after,
        })
    }
}
pub(super) mod facts {
    //! Actual selected overload, Borrow argument delivery and the provider's independent owner facts.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, ast};
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, LoanKind, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
            UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
            UnitLoanTarget,
        },
        parser::ParsedFile,
        source::{SourceMap, Span},
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
            UnitTypeKind,
        },
    };

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn unit(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        control: bool,
    ) -> Result<(), Failure> {
        let input = ast::input(sources, parsed, control)?;
        let resolution = one(
            names.names().source_units().iter(),
            "argument original source unit",
        )?;
        let unit = resolution.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        let declaration = |name: Span| {
            one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == unit && d.name_span() == name)
                    .map(|d| d.id()),
                "argument actual qualified declaration",
            )
        };
        let factory = declaration(input.factory.name)?;
        let signature = typed
            .signatures()
            .declaration(factory)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("argument actual qualified signature", factory))?;
        let call = typed
            .call(expression(input.produce))
            .ok_or_else(|| failure("argument actual qualified factory Call", input.produce))?;
        let value = typed
            .call(expression(input.value))
            .ok_or_else(|| failure("argument actual qualified argument Call", input.value))?;
        need(
            call.target() == UnitCallTarget::Declaration(factory)
                && factory != declaration(input.rejected.name)?
                && signature.parameters().len() == 1
                && signature.parameters()[0].mode() == ParameterMode::Borrow
                && typed.types().get(signature.parameters()[0].ty())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && signature.parameters()[0].symbol().is_some_and(|p| {
                    p.source_unit() == unit
                        && resolution
                            .resolution()
                            .symbols()
                            .get(p.symbol().index())
                            .is_some_and(|s| s.span() == input.factory.parameter.unwrap())
                })
                && value.target() == UnitCallTarget::Declaration(declaration(input.argument.name)?)
                && value.arguments().is_empty()
                && value.receiver().is_none()
                && typed.types().get(value.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && typed.expression_type(expression(input.value)) == Some(value.return_type())
                && typed.expression_category(expression(input.value))
                    == Some(ExpressionCategory::Temporary)
                && matches!(call.arguments(), [argument] if argument.argument_index() == 0 && argument.parameter_index() == 0
                && argument.mode() == ParameterMode::Borrow && argument.category() == ExpressionCategory::Temporary
                && argument.parameter_type() == value.return_type())
                && call.return_type() == signature.return_type()
                && call.result_source() == CallableResultSource::Owned
                && call.receiver().is_none()
                && typed.expression_type(expression(input.produce)) == Some(call.return_type())
                && typed.expression_category(expression(input.produce))
                    == Some(ExpressionCategory::Temporary),
            "argument actual qualified Int overload/signature/Borrow mapping",
            (call, value, signature),
        )?;
        let constructor = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            matches!(typed.types().get(call.return_type()), Some(UnitTypeKind::Intrinsic {constructor:c, arguments})
            if *c == constructor && arguments.len() == 2 && arguments.iter().all(|ty| typed.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
            "argument qualified exact represented factory return",
            call,
        )?;
        let loan = one(
            owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == expression(input.produce)),
            "argument qualified actual argument loan",
        )?;
        need(
            loan.argument() == expression(input.value)
                && loan.kind() == LoanKind::Shared
                && loan.target() == &UnitLoanTarget::Temporary(expression(input.value))
                && loan.begin_span() == ast::span(parsed, input.value)
                && loan.end_span() == ast::span(parsed, input.produce)
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|delivery| delivery.call() == expression(input.produce)),
            "argument qualified actual temporary Borrow and no Value delivery",
            loan,
        )?;
        need(
            typed.map_iterations().len() == usize::from(!control)
                && owned.map_iterations().len() == usize::from(!control),
            "argument qualified actual provider/control plans",
            typed.map_iterations(),
        )?;
        let Some(site) = &input.loop_site else {
            return Ok(());
        };
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("argument qualified actual temporary plan", site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == expression(input.produce)
                && d.source_type() == call.return_type()
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && p.source() == Some(&UnitLoanTarget::Temporary(expression(input.produce))),
            "argument qualified factory Call is this provider's hidden owner",
            p,
        )?;
        let read = typed
            .call(expression(site.read))
            .ok_or_else(|| failure("argument actual qualified reader", site.read))?;
        need(
            read.target() == UnitCallTarget::Declaration(declaration(input.reader.name)?)
                && read.arguments().len() == 2
                && read.receiver().is_none(),
            "argument qualified actual reader selection",
            read,
        )?;
        for (index, component) in p.loans().iter().enumerate() {
            let binding = d.bindings()[index];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[index])),
                "argument qualified component reference",
            )?;
            need(
                binding.source_unit() == unit
                    && resolution
                        .resolution()
                        .symbols()
                        .get(binding.symbol().index())
                        .is_some_and(|s| s.span() == site.bindings[index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && component.statement() == statement
                    && component.source() == expression(input.produce)
                    && component.binding() == binding
                    && component.origin() == &UnitLoanTarget::Temporary(expression(input.produce))
                    && component.kind() == LoanKind::Shared
                    && matches!(reference.target(), UnitReferenceTarget::Symbol(root) if *root == binding)
                    && read.arguments()[index].mode() == ParameterMode::Borrow
                    && read.arguments()[index].argument_index() == index
                    && read.arguments()[index].parameter_index() == index
                    && read.arguments()[index].parameter_type()
                        == [d.key_type(), d.value_type()][index],
                "argument qualified K/V original roots and Borrow mapping",
                (component, reference),
            )?;
        }
        // Unit has no old cleanup-owner/Always/closure-phi table. The paired graph
        // query proves the original Call birth, every directed current-role edge,
        // exact retirement and complete owner/lease/place definition inventory.
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(expression(input.produce))),
            "argument qualified unique hidden owner Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::LoopExit(statement)
                && drop.value_origin() == ast::span(parsed, input.produce)
                && p.exits().len() == 2,
            "argument qualified Call-origin owner normal Drop",
            drop,
        )?;
        for (kind, point, actions) in [
            (
                UE::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UE::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UA::FinishProvider(statement),
                    UA::EndSource(statement),
                    UA::Drop(*drop),
                ],
            ),
        ] {
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "argument qualified actual normal exit",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "argument qualified complete same-point normal cleanup",
                exit,
            )?;
        }
        Ok(())
    }
}
pub(super) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub control: String,
    pub golden: String,
    pub control_golden: String,
    pub mutable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    [false, true]
        .into_iter()
        .map(|mutable| {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let factory = if mutable { "mutableMapOf" } else { "mapOf" };
            let conversion = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let common = format!(
                r#"fun argument():Int{{println("argument");return 1}}
fun produce(input:Int):{kind}<String,String>{{if(input!=1){{error("bad-input")}};println("factory");var result=mutableMapOf<String,String>();result.put("key".clone(),"value".clone());{conversion}}}
fun produce(input:Boolean):{kind}<String,String>{{println("wrong-overload");return {factory}<String,String>()}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
"#
            );
            let source = format!(
                "{common}fun entry():Unit{{println(\"begin\");for((key,value) in produce(argument())){{read(key,value)}};println(\"after\")}}\n"
            );
            let control = format!(
                "{common}fun entry():Unit{{println(\"begin\");val source=produce(argument());if(source.size==1){{println(\"control\")}}else{{error(\"bad-control-size\")}};println(\"after\")}}\n"
            );
            Case {
                label: format!("temporary-argument-{kind}-borrow-int-overload"),
                source,
                control,
                golden: "begin\nargument\nfactory\nkey\nvalue\nafter\n".to_owned(),
                control_golden: "begin\nargument\nfactory\ncontrol\nafter\n".to_owned(),
                mutable,
            }
        })
        .collect()
}

fn observe(case: &Case, rooted: bool, control: bool) -> Result<(), Failure> {
    let text = if control { &case.control } else { &case.source };
    let golden = if control {
        &case.control_golden
    } else {
        &case.golden
    };
    diagnostic!(
        "TEMPORARY ARGUMENT INPUT case={} rooted={rooted} control={control} source={text:?} golden={golden:?}",
        case.label
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        rooted,
        |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            r.query(
                "argument original qualified overload and Borrow facts",
                &facts::unit(case, &sources, &parsed, n, t, o, control),
            );
        },
    );
    diagnostic!(
        "TEMPORARY ARGUMENT LOWER {} rooted={rooted} control={control} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit original temporary entry and actual canonical owner acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "argument independent selected Call and current owner graph",
        &graph::check(case, &sources, &parsed, &program, control),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE queries", report));
    }
    Ok(())
}

fn check(control: bool) {
    let mut outcomes = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            let result = observe(&case, rooted, control);
            diagnostic!(
                "TEMPORARY ARGUMENT CASE {} rooted={rooted} control={control} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), rooted, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

#[test]
fn selected_argument_factory_reaches_both_provider_pipelines() {
    check(false);
}

#[test]
fn same_selected_argument_call_without_for_reaches_both_pipelines() {
    check(true);
}
