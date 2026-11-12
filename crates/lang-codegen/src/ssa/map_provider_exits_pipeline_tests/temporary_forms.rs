//! Existing represented payloads and Int keys retain a real temporary owner per For.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Original same-scope source Calls, component markers and following output statements.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use lang_frontend::{
        ast::{ExpressionId, ItemId, StatementId},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
        },
        source::{SourceMap, Span},
    };

    #[derive(Debug)]
    pub(super) struct Function {
        pub item: ItemId,
        pub name: Span,
        pub span: Span,
        pub body: StatementId,
    }

    #[derive(Debug)]
    pub(super) struct Site {
        pub statement: StatementId,
        pub span: Span,
        pub body: StatementId,
        pub source: ExpressionId,
        pub call: Span,
        pub factory: Function,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub arguments: [ExpressionId; 2],
        pub after: Span,
        pub marker: &'static str,
    }

    pub(super) struct Input {
        pub entry: Function,
        pub reader: Function,
        pub sites: [Site; 2],
    }

    pub(super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }

    pub(super) fn function(
        sources: &SourceMap,
        parsed: &ParsedFile,
        wanted: &str,
    ) -> Result<Function, Failure> {
        one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                let Item::Function {
                    name: NameMarker::Present(name),
                    form,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                if sources.slice(*name) != Ok(wanted) {
                    return None;
                }
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return None,
                };
                Some(Function {
                    item: *id,
                    name: *name,
                    span: node.span(),
                    body,
                })
            }),
            "temporary forms original function",
        )
    }

    fn elements(parsed: &ParsedFile, block: StatementId) -> Result<&[StatementId], Failure> {
        match parsed.ast().statements().get(block).unwrap().payload() {
            Statement::Block { elements } => Ok(elements),
            other => Err(failure("temporary forms original block", other)),
        }
    }

    fn statement_expression(
        parsed: &ParsedFile,
        statement: StatementId,
    ) -> Result<ExpressionId, Failure> {
        match parsed.ast().statements().get(statement).unwrap().payload() {
            Statement::Expression { expression } => Ok(*expression),
            other => Err(failure(
                "temporary forms original expression statement",
                other,
            )),
        }
    }

    fn call(
        sources: &SourceMap,
        parsed: &ParsedFile,
        id: ExpressionId,
        name: &str,
        arity: usize,
    ) -> Result<Vec<ExpressionId>, Failure> {
        let node = parsed.ast().expressions().get(id).unwrap();
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = node.payload()
        else {
            return Err(failure("temporary forms original direct Call", node));
        };
        need(
            matches!(
                parsed.ast().expressions().get(*callee).unwrap().payload(),
                Expression::Name
            ) && sources.slice(expression(parsed, *callee)) == Ok(name)
                && type_arguments.is_empty()
                && arguments.len() == arity
                && arguments
                    .iter()
                    .all(|a| a.named_prefix.is_none() && a.mode_marker.is_none()),
            "temporary forms exact source Call and operand order",
            node,
        )?;
        Ok(arguments.iter().map(|argument| argument.value).collect())
    }

    fn marker(
        sources: &SourceMap,
        parsed: &ParsedFile,
        id: StatementId,
        expected: &str,
    ) -> Result<Span, Failure> {
        let id = statement_expression(parsed, id)?;
        let argument = call(sources, parsed, id, "println", 1)?[0];
        need(
            sources.slice(expression(parsed, argument)) == Ok(format!("\"{expected}\"").as_str()),
            "temporary forms actual output marker",
            argument,
        )?;
        Ok(expression(parsed, id))
    }

    fn site(
        sources: &SourceMap,
        parsed: &ParsedFile,
        statement: StatementId,
        factory: &str,
        after: Span,
        marker: &'static str,
    ) -> Result<Site, Failure> {
        let node = parsed.ast().statements().get(statement).unwrap();
        let Statement::For {
            binding: ForBinding::Destructuring { names, .. },
            source,
            body,
            ..
        } = node.payload()
        else {
            return Err(failure("temporary forms original destructuring For", node));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("temporary forms actual K/V markers", names));
        };
        need(
            sources.slice(*key) == Ok("key") && sources.slice(*value) == Ok("value"),
            "temporary forms ordered original component names",
            names,
        )?;
        call(sources, parsed, *source, factory, 0)?;
        let [read] = elements(parsed, *body)? else {
            return Err(failure("temporary forms sole body reader", body));
        };
        let read = statement_expression(parsed, *read)?;
        let arguments = call(sources, parsed, read, "read", 2)?;
        let arguments = [arguments[0], arguments[1]];
        for (argument, name) in arguments.into_iter().zip(["key", "value"]) {
            need(
                matches!(
                    parsed.ast().expressions().get(argument).unwrap().payload(),
                    Expression::Name
                ) && sources.slice(expression(parsed, argument)) == Ok(name),
                "temporary forms actual read Name operand",
                argument,
            )?;
        }
        Ok(Site {
            statement,
            span: node.span(),
            body: *body,
            source: *source,
            call: expression(parsed, *source),
            factory: function(sources, parsed, factory)?,
            bindings: [*key, *value],
            read,
            arguments,
            after,
            marker,
        })
    }

    pub(super) fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
        let entry = function(sources, parsed, "entry")?;
        let reader = function(sources, parsed, "read")?;
        let [nonempty, first, after_first, empty, second, after_second] = elements(parsed, entry.body)?
        else {
            return Err(failure(
                "temporary forms six same-scope entry statements",
                entry,
            ));
        };
        marker(sources, parsed, *nonempty, "nonempty")?;
        marker(sources, parsed, *empty, "empty")?;
        let first_after = marker(sources, parsed, *after_first, "after-nonempty")?;
        let second_after = marker(sources, parsed, *after_second, "after-empty")?;
        let sites = [
            site(
                sources,
                parsed,
                *first,
                "produce",
                first_after,
                "after-nonempty",
            )?,
            site(
                sources,
                parsed,
                *second,
                "produceEmpty",
                second_after,
                "after-empty",
            )?,
        ];
        need(
            parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
                .count()
                == 2
                && sites[0].span.end() < sites[1].span.start()
                && sites[0].source != sites[1].source
                && sites[0]
                    .bindings
                    .iter()
                    .all(|binding| !sites[1].bindings.contains(binding)),
            "temporary forms exactly two distinct same-scope providers",
            &sites,
        )?;
        Ok(Input {
            entry,
            reader,
            sites,
        })
    }

    pub(super) fn nominal(
        sources: &SourceMap,
        parsed: &ParsedFile,
        name: &str,
    ) -> Result<(Span, Span), Failure> {
        one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                let Item::Classifier(class) = node.payload() else {
                    return None;
                };
                matches!(class.name, NameMarker::Present(marker) if sources.slice(marker) == Ok(name))
                    .then(|| {
                        let NameMarker::Present(marker) = class.name else {
                            unreachable!()
                        };
                        (node.span(), marker)
                    })
            }),
            "temporary forms original nominal declaration",
        )
    }
}
mod facts {
    //! Same-analysis source selection, represented component types and independent public cleanup.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
        query_temporary_cleanup as cleanup,
    };
    use super::{Case, ast};
    use lang_frontend::{
        name_resolution::{
            NameResolution, SymbolId, UnitReferenceTarget, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
            OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::ParsedFile,
        source::{SourceMap, Span},
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
            UnitExpressionId, UnitStatementId, UnitTypeId, UnitTypeKind,
        },
    };

    fn single_symbol(names: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
        Ok(one(
            names
                .symbols()
                .iter()
                .filter(|symbol| symbol.span() == marker && !symbol.is_synthetic()),
            "temporary forms actual source symbol",
        )?
        .id())
    }

    fn unit_value(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        typed: &CompilationUnitTypes,
        names: &ValidatedCompilationUnitNames,
        key: UnitTypeId,
        value: UnitTypeId,
    ) -> Result<(), Failure> {
        let expected = if matches!(case.representation, 2 | 4) {
            BuiltinType::Int
        } else {
            BuiltinType::String
        };
        need(
            typed.types().get(key) == Some(&UnitTypeKind::Builtin(expected)),
            "temporary forms exact qualified key",
            key,
        )?;
        if case.representation == 4 {
            return need(
                typed.types().get(value) == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
                "temporary forms qualified Pure String",
                value,
            );
        }
        let value = if case.representation == 3 {
            let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(value) else {
                return Err(failure("temporary forms actual qualified nullable", value));
            };
            *inner
        } else {
            value
        };
        let wanted = ast::nominal(
            sources,
            parsed,
            if case.representation == 1 {
                "Payload"
            } else {
                "Resource"
            },
        )?
        .1;
        need(
            matches!(typed.types().get(value), Some(UnitTypeKind::Nominal {declaration, arguments})
            if arguments.is_empty() && typed.signatures().declaration(*declaration).and_then(|d| d.nominal()).is_some_and(|nominal|
                names.names().source_units().iter().any(|unit| unit.source_unit() == nominal.symbol().source_unit()
                    && unit.resolution().symbols().get(nominal.symbol().symbol().index()).is_some_and(|symbol| symbol.span() == wanted)))),
            "temporary forms original qualified represented nominal",
            value,
        )
    }

    pub(super) fn unit(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let input = ast::input(sources, parsed)?;
        let resolution = one(
            names.names().source_units().iter(),
            "temporary forms original source unit",
        )?;
        let unit = resolution.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        need(
            typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
            "temporary forms two actual qualified plans",
            typed.map_iterations(),
        )?;
        let target = |function: &ast::Function| {
            one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| {
                        d.source_unit() == unit
                            && d.root() == function.item
                            && d.name_span() == function.name
                    })
                    .map(|d| d.id()),
                "temporary forms original qualified function declaration",
            )
        };
        let mut bindings = Vec::new();
        for site in &input.sites {
            let statement = UnitStatementId::new(unit, site.statement);
            let source = expression(site.source);
            let d = typed
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary forms actual qualified typed For", site))?;
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary forms actual qualified owned For", site))?;
            let factory = typed
                .call(source)
                .ok_or_else(|| failure("temporary forms actual qualified factory Call", source))?;
            let constructor = if case.mutable {
                IntrinsicTypeConstructor::MutableMap
            } else {
                IntrinsicTypeConstructor::Map
            };
            need(
                p.descriptor() == d
                    && d.source() == source
                    && d.kind()
                        == if case.mutable {
                            MapIterationKind::MutableMap
                        } else {
                            MapIterationKind::Map
                        }
                    && typed.expression_type(source) == Some(d.source_type())
                    && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                    && typed.is_resource_type(d.source_type()) == Some(case.representation != 4)
                    && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic {constructor:c, arguments}) if *c == constructor && arguments.as_slice() == [d.key_type(), d.value_type()])
                    && p.source_access() == UnitIterationSourceAccess::Temporary
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source),
                "temporary forms original qualified temporary and represented classification",
                p,
            )?;
            unit_value(
                case,
                sources,
                parsed,
                typed,
                names,
                d.key_type(),
                d.value_type(),
            )?;
            need(
                factory.target() == UnitCallTarget::Declaration(target(&site.factory)?)
                    && factory.expression() == source
                    && factory.arguments().is_empty()
                    && factory.receiver().is_none()
                    && factory.return_type() == d.source_type()
                    && factory.result_source() == CallableResultSource::Owned,
                "temporary forms exact qualified zeroarg factory Owned result",
                factory,
            )?;
            let read = typed
                .call(expression(site.read))
                .ok_or_else(|| failure("temporary forms actual qualified reader", site.read))?;
            need(
                read.target() == UnitCallTarget::Declaration(target(&input.reader)?)
                    && read.arguments().len() == 2
                    && read.receiver().is_none(),
                "temporary forms actual qualified reader target/arity",
                read,
            )?;
            for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let binding = d.bindings()[index];
                let loan = p.loan(component);
                let argument = read.arguments()[index];
                let reference = one(
                    names
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::expression(parsed, site.arguments[index])),
                    "temporary forms qualified actual reader root",
                )?;
                need(
                    binding.source_unit() == unit
                        && binding.symbol()
                            == single_symbol(resolution.resolution(), site.bindings[index])?
                        && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && p.bindings()[index].symbol() == binding
                        && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                        && loan.statement() == statement
                        && loan.source() == source
                        && loan.binding() == binding
                        && loan.component() == component
                        && loan.kind() == LoanKind::Shared
                        && loan.origin() == &UnitLoanTarget::Temporary(source)
                        && argument.argument_index() == index
                        && argument.parameter_index() == index
                        && argument.mode() == ParameterMode::Borrow
                        && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                        && matches!(reference.target(), UnitReferenceTarget::Symbol(root) if *root == binding),
                    "temporary forms qualified ordered Shared component and Borrow argument",
                    (loan, argument, reference),
                )?;
                bindings.push(binding);
            }
            // Unit publishes exact source-qualified Drop/exit actions. The paired
            // graph query proves the retired Call owner, all directed incoming
            // roles, and disjoint complete capabilities instead of old FE owner IDs.
            cleanup::unit(
                UnitStatementId::new(unit, site.body),
                site.call,
                statement,
                source,
                owned,
            )?;
        }
        need(
            bindings
                .iter()
                .enumerate()
                .all(|(index, value)| !bindings[..index].contains(value)),
            "temporary forms four qualified component identities stay separate",
            bindings,
        )
    }
}
pub(in crate::ssa) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub representation: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for representation in 0..5 {
        for mutable in [false, true] {
            let tag = [
                "field",
                "structural",
                "int-resource",
                "nullable-field",
                "int-string",
            ][representation];
            let kind = if mutable { "MutableMap" } else { "Map" };
            let integer = matches!(representation, 2 | 4);
            let key = if integer { "Int" } else { "String" };
            let payload = match representation {
                1 => "Payload",
                3 => "Resource?",
                4 => "String",
                _ => "Resource",
            };
            let resource = if representation == 0 || representation == 3 {
                "class Resource(val tag:String){deinit(){println(this.tag)}}"
            } else if representation == 4 {
                ""
            } else {
                "class Resource{deinit(){println(\"resource-drop\")}}"
            };
            let structural = if representation == 1 {
                "value class Payload(val resource:Resource)"
            } else {
                ""
            };
            let construct = match representation {
                0 | 3 => "Resource(\"field-drop\".clone())",
                1 => "Payload(Resource())",
                4 => "\"value\".clone()",
                _ => "Resource()",
            };
            let first = if integer { "1" } else { "\"z\".clone()" };
            let second = if integer { "2" } else { "\"a\".clone()" };
            let middle = if representation == 3 {
                "source.put(\"n\".clone(),null);"
            } else {
                ""
            };
            let read_key = if integer {
                "val actualKey:Int=key;if(actualKey==1){println(\"int-one\")}else{if(actualKey==2){println(\"int-two\")}else{error(\"bad-int-key\")}}"
            } else {
                "println(key)"
            };
            let read_value = match representation {
                3 => "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}",
                4 => "println(value)",
                _ => "println(\"value\")",
            };
            let conversion = if mutable {
                "return source"
            } else {
                "val result=source.consume();return result"
            };
            let empty = if mutable { "mutableMapOf" } else { "mapOf" };
            let source = format!(
                r#"{resource}
{structural}
fun read(key:{key},value:{payload}):Unit{{{read_key};{read_value}}}
fun produce():{kind}<{key},{payload}>{{println("factory-start");var source=mutableMapOf<{key},{payload}>();source.put({first},{construct});{middle}source.put({second},{construct});println("factory-ready");{conversion}}}
fun produceEmpty():{kind}<{key},{payload}>{{println("empty-start");val result={empty}<{key},{payload}>();println("empty-ready");return result}}
fun entry():Unit{{println("nonempty");for((key,value) in produce()){{read(key,value)}};println("after-nonempty");println("empty");for((key,value) in produceEmpty()){{read(key,value)}};println("after-empty")}}
"#
            );
            let mut golden = String::from("nonempty\nfactory-start\nfactory-ready\n");
            let keys = if integer {
                vec!["int-one", "int-two"]
            } else if representation == 3 {
                vec!["z", "n", "a"]
            } else {
                vec!["z", "a"]
            };
            for (index, key) in keys.into_iter().enumerate() {
                golden.push_str(key);
                golden.push('\n');
                golden.push_str(if representation == 3 {
                    if index == 1 { "none\n" } else { "some\n" }
                } else {
                    "value\n"
                });
            }
            if representation != 4 {
                let drop = if representation == 0 || representation == 3 {
                    "field-drop\n"
                } else {
                    "resource-drop\n"
                };
                golden.push_str(drop);
                golden.push_str(drop);
            }
            golden.push_str("after-nonempty\nempty\nempty-start\nempty-ready\nafter-empty\n");
            cases.push(Case {
                label: format!("temporary-{tag}-{kind}-normal"),
                source,
                golden,
                mutable,
                representation,
            });
        }
    }
    assert_eq!(cases.len(), 10);
    cases
}

fn observe(case: &Case, rooted: bool) -> Result<(), Failure> {
    diagnostic!(
        "TEMPORARY FORMS INPUT case={} rooted={rooted} source={:?} golden={:?}",
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
    let mut report = Report::default();
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        rooted,
        |names, typed, owned, report| {
            source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
            report.query(
                "represented temporary original qualified FE identities",
                &facts::unit(case, &sources, &parsed, names, typed, owned),
            );
        },
    );
    diagnostic!(
        "TEMPORARY FORMS LOWER {} rooted={rooted} report={report:#?} outcome={:?}",
        case.label,
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    report.query(
        "Unit original temporary entry and actual canonical owner acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "represented temporary actual SSA identities and storage",
        &graph::check(case, &sources, &parsed, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE/SSA observations", report));
    }
    Ok(())
}

#[test]
fn existing_temporary_representations_reach_both_verified_pipelines() {
    let mut results = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            let result = observe(&case, rooted);
            diagnostic!(
                "TEMPORARY FORMS CASE {} rooted={rooted} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), rooted, result));
        }
    }
    assert_eq!(results.len(), 20);
    assert_diagnostic!(
        results.iter().all(|(_, _, result)| result.is_ok()),
        "{results:#?}"
    );
}
