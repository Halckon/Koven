//! Existing ordinary parameter delivery is shared by no-For controls and temporary sources.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Real source-order argument Calls, named mapping and the selected declaration.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, Delivery};
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker,
            ParameterModeMarker, ParsedFile, Statement,
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
        pub function: Function,
    }
    #[derive(Debug)]
    pub(super) struct Site {
        pub statement: StatementId,
        pub span: Span,
        pub body: StatementId,
        pub source: ExpressionId,
        pub call: Span,
        pub bindings: [Span; 2],
        pub read: ExpressionId,
        pub arguments: [ExpressionId; 2],
    }
    pub(super) struct Input {
        pub entry: Function,
        pub factory: Function,
        pub reader: Function,
        pub call: ExpressionId,
        pub arguments: Vec<Argument>,
        pub site: Option<Site>,
        pub after: Span,
        pub control_root: Option<Span>,
        pub size: Option<ExpressionId>,
    }

    pub(super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn inside(inner: Span, outer: Span) -> bool {
        inner.source_id() == outer.source_id()
            && inner.start() > outer.start()
            && inner.end() < outer.end()
    }

    pub(super) fn function(
        sources: &SourceMap,
        parsed: &ParsedFile,
        wanted: &str,
    ) -> Result<Function, Failure> {
        let node=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok())
            .filter(|n|matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted))),"delivery original function declaration")?;
        let Item::Function {
            name: NameMarker::Present(name),
            form,
            parameters,
            ..
        } = node.payload()
        else {
            unreachable!()
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            other => return Err(failure("delivery original callable block", other)),
        };
        let parameters = parameters
            .iter()
            .map(|p| {
                let NameMarker::Present(name) = p.name else {
                    return Err(failure("delivery actual parameter marker", p));
                };
                let mode = match p.mode_marker {
                    None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
                    Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
                    other => return Err(failure("delivery actual parameter mode", other)),
                };
                Ok(Parameter {
                    name,
                    span: p.span,
                    mode,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        Ok(Function {
            span: node.span(),
            name: *name,
            body,
            parameters,
        })
    }

    fn call(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
    ) -> Result<ExpressionId, Failure> {
        one(
            parsed.ast().expressions().iter().filter_map(|(id, n)| {
                let Expression::Call { callee, .. } = n.payload() else {
                    return None;
                };
                (inside(n.span(), scope)
                    && sources.slice(expression(parsed, *callee)) == Ok(wanted))
                .then_some(id)
            }),
            "delivery actual source Call",
        )
    }

    pub(super) fn input(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        control: bool,
    ) -> Result<Input, Failure> {
        let entry = function(sources, parsed, "entry")?;
        let factory = function(sources, parsed, "produce")?;
        let reader = function(sources, parsed, "read")?;
        let call = call(sources, parsed, entry.span, "produce")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        let expected = match case.delivery {
            Delivery::BorrowAndOwnedResource => vec![("borrowedArgument", 0), ("ownedArgument", 1)],
            Delivery::SameMapBorrow => vec![("borrowedArgument", 0)],
            Delivery::ReorderedMixed => vec![
                ("rightArgument", 1),
                ("ownedArgument", 2),
                ("leftArgument", 0),
            ],
        };
        need(
            arguments.len() == expected.len() && factory.parameters.len() == arguments.len(),
            "delivery actual arity matches fixed fixture",
            arguments,
        )?;
        let arguments = arguments
            .iter()
            .zip(expected)
            .enumerate()
            .map(|(index, (argument, (wanted, expected_slot)))| {
                need(
                    argument.mode_marker.is_none(),
                    "delivery ordinary Value/Borrow has no Inout marker",
                    argument,
                )?;
                let parameter = if let Some(prefix) = argument.named_prefix {
                    one(
                        factory
                            .parameters
                            .iter()
                            .enumerate()
                            .filter_map(|(slot, p)| {
                                (sources.slice(p.name) == sources.slice(prefix.name_span))
                                    .then_some(slot)
                            }),
                        "delivery original named parameter mapping",
                    )?
                } else {
                    index
                };
                let node = parsed.ast().expressions().get(argument.value).unwrap();
                let Expression::Call {
                    callee, arguments, ..
                } = node.payload()
                else {
                    return Err(failure("delivery actual argument evaluator Call", node));
                };
                need(
                    parameter == expected_slot
                        && arguments.is_empty()
                        && sources.slice(expression(parsed, *callee)) == Ok(wanted)
                        && matches!(
                            parsed.ast().expressions().get(*callee).unwrap().payload(),
                            Expression::Name
                        ),
                    "delivery source-order evaluator and actual named slot",
                    (index, parameter, node),
                )?;
                need(
                    factory.parameters[parameter].mode
                        == if wanted == "ownedArgument" {
                            ParameterMode::Value
                        } else {
                            ParameterMode::Borrow
                        },
                    "delivery actual Value/Borrow parameter syntax",
                    &factory.parameters[parameter],
                )?;
                let function = function(sources, parsed, wanted)?;
                need(
                    function.parameters.is_empty(),
                    "delivery evaluator has no arguments",
                    &function,
                )?;
                Ok(Argument {
                    span: argument.span,
                    expression: argument.value,
                    parameter,
                    function,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let loops = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, n)| matches!(n.payload(), Statement::For { .. }).then_some((id, n)))
            .collect::<Vec<_>>();
        need(
            loops.len() == usize::from(!control),
            "delivery original provider/control distinction",
            &loops,
        )?;
        let site = if control {
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
                return Err(failure("delivery original For shape", node));
            };
            let [NameMarker::Present(key), NameMarker::Present(val)] = names.as_slice() else {
                return Err(failure("delivery actual component markers", names));
            };
            let read = self::call(sources, parsed, node.span(), "read")?;
            let Expression::Call { arguments, .. } =
                parsed.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            need(
                *source == call && arguments.len() == 2,
                "delivery actual factory Call is provider source",
                node,
            )?;
            Some(Site {
                statement,
                span: node.span(),
                body: *body,
                source: *source,
                call: expression(parsed, *source),
                bindings: [*key, *val],
                read,
                arguments: [arguments[0].value, arguments[1].value],
            })
        };
        let (control_root, size) = if control {
            let root = one(
                parsed.ast().items().iter().filter_map(|(_, n)| {
                    let Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } = n.payload()
                    else {
                        return None;
                    };
                    (inside(n.span(), entry.span) && *initializer == call).then_some(*name)
                }),
                "delivery actual control result binding",
            )?;
            let size=one(parsed.ast().expressions().iter().filter_map(|(id,n)|{
                (inside(n.span(),entry.span)&&matches!(n.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok("size"))).then_some(id)
            }),"delivery actual control size")?;
            (Some(root), Some(size))
        } else {
            (None, None)
        };
        let after=one(parsed.ast().expressions().iter().filter_map(|(_,n)|{
            let Expression::Call {callee,arguments,..}=n.payload() else{return None;};
            (inside(n.span(),entry.span)&&sources.slice(expression(parsed,*callee))==Ok("println")
                &&matches!(arguments.as_slice(),[arg] if sources.slice(expression(parsed,arg.value))==Ok("\"after\""))).then_some(n.span())
        }),"delivery actual after output")?;
        Ok(Input {
            entry,
            factory,
            reader,
            call,
            arguments,
            site,
            after,
            control_root,
            size,
        })
    }
}
mod facts {
    //! Source-order descriptors and ownership deliveries from the same actual FE products.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
        query_temporary_cleanup as cleanup,
    };
    use super::{Case, Delivery, ast};
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, LoanKind, MapIterationComponent, OwnershipBindingKind,
            UnitCallArgumentOwnershipKind, UnitDropPoint, UnitDropTarget,
            UnitIterationSourceAccess, UnitLoanTarget, UnitValueDeliveryKind,
            UnitValueDeliverySource,
        },
        parser::{Expression, ParsedFile},
        source::SourceMap,
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId,
            UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        control: bool,
    ) -> Result<(), Failure> {
        let input = ast::input(case, sources, parsed, control)?;
        let resolution = one(
            names.names().source_units().iter(),
            "delivery original source unit",
        )?;
        let unit = resolution.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        let declaration = |name| {
            one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == unit && d.name_span() == name)
                    .map(|d| d.id()),
                "delivery actual qualified callable",
            )
        };
        let target = declaration(input.factory.name)?;
        let signature = typed
            .signatures()
            .declaration(target)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("delivery actual qualified signature", target))?;
        let call = typed
            .call(expression(input.call))
            .ok_or_else(|| failure("delivery qualified actual Call", input.call))?;
        need(
            call.target() == UnitCallTarget::Declaration(target)
                && call.receiver().is_none()
                && call.arguments().len() == input.arguments.len()
                && signature.parameters().len() == input.arguments.len()
                && call.return_type() == signature.return_type()
                && call.result_source() == CallableResultSource::Owned,
            "delivery qualified selected signature and complete arity",
            (call, signature),
        )?;
        let resource = !matches!(case.delivery, Delivery::SameMapBorrow);
        let kind = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            typed.is_resource_type(call.return_type()) == Some(resource)
                && matches!(typed.types().get(call.return_type()),Some(UnitTypeKind::Intrinsic {constructor,arguments})
                if *constructor==kind&&arguments.len()==2&&typed.types().get(arguments[0])==Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    &&(resource||typed.types().get(arguments[1])==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
            "delivery qualified exact result kind/classification",
            call.return_type(),
        )?;
        for (index, argument) in input.arguments.iter().enumerate() {
            let descriptor = call.arguments()[index];
            let parameter = &input.factory.parameters[argument.parameter];
            let expected = &signature.parameters()[argument.parameter];
            let value = typed
                .call(expression(argument.expression))
                .ok_or_else(|| failure("delivery qualified evaluator Call", argument.expression))?;
            let parameter_symbol = expected
                .symbol()
                .ok_or_else(|| failure("delivery original qualified parameter symbol", expected))?;
            need(
                value.target() == UnitCallTarget::Declaration(declaration(argument.function.name)?)
                    && value.arguments().is_empty()
                    && value.receiver().is_none()
                    && value.result_source() == CallableResultSource::Owned
                    && value.return_type() == expected.ty()
                    && descriptor.argument_index() == index
                    && descriptor.parameter_index() == argument.parameter
                    && descriptor.mode() == parameter.mode
                    && expected.mode() == parameter.mode
                    && descriptor.parameter_type() == value.return_type()
                    && descriptor.category() == ExpressionCategory::Temporary
                    && typed.expression_category(expression(argument.expression))
                        == Some(ExpressionCategory::Temporary)
                    && typed.expression_type(expression(argument.expression))
                        == Some(value.return_type())
                    && parameter_symbol.source_unit() == unit
                    && resolution
                        .resolution()
                        .symbols()
                        .get(parameter_symbol.symbol().index())
                        .is_some_and(|s| s.span() == parameter.name)
                    && owned.binding_kind(parameter_symbol)
                        == Some(if parameter.mode == ParameterMode::Value {
                            OwnershipBindingKind::Owned
                        } else {
                            OwnershipBindingKind::Shared
                        }),
                "delivery qualified actual source-order ABI mapping/category/capability",
                (index, descriptor, value),
            )?;
            let contract = one(
                owned
                    .call_argument_contracts_for(expression(input.call))
                    .filter(|c| c.argument() == expression(argument.expression)),
                "delivery original argument ownership contract",
            )?;
            need(
                contract.argument_span() == ast::expression(parsed, argument.expression)
                    && contract.call_span() == ast::expression(parsed, input.call)
                    && contract.parameter_span() == Some(parameter.span)
                    && contract.parameter_index() == argument.parameter
                    && contract.parameter_type() == expected.ty()
                    && contract.category() == ExpressionCategory::Temporary
                    && contract.kind()
                        == if parameter.mode == ParameterMode::Value {
                            UnitCallArgumentOwnershipKind::Value
                        } else {
                            UnitCallArgumentOwnershipKind::SharedLoan
                        },
                "delivery exact qualified ownership contract",
                contract,
            )?;
            let loans = owned
                .loans()
                .iter()
                .filter(|l| {
                    l.call() == expression(input.call)
                        && l.argument() == expression(argument.expression)
                })
                .collect::<Vec<_>>();
            let drops = owned
                .drops()
                .iter()
                .filter(|d| {
                    d.target() == UnitDropTarget::Temporary(expression(argument.expression))
                })
                .collect::<Vec<_>>();
            let deliveries = owned
                .value_deliveries()
                .iter()
                .filter(|d| {
                    d.call() == expression(input.call)
                        && d.argument() == expression(argument.expression)
                })
                .collect::<Vec<_>>();
            if parameter.mode == ParameterMode::Borrow {
                need(
                    matches!(loans.as_slice(),[loan] if loan.kind()==LoanKind::Shared&&loan.target()==&UnitLoanTarget::Temporary(expression(argument.expression))
                    &&loan.begin_span()==ast::expression(parsed,argument.expression)&&loan.end_span()==ast::expression(parsed,input.call))
                        && matches!(drops.as_slice(),[drop] if drop.point()==UnitDropPoint::CallReturn(expression(input.call))&&drop.value_origin()==ast::expression(parsed,argument.expression))
                        && deliveries.is_empty(),
                    "delivery qualified temporary loan/CallReturn Drop/no Value delivery",
                    (loans, drops, deliveries),
                )?;
            } else {
                need(
                    loans.is_empty()
                        && drops.is_empty()
                        && matches!(deliveries.as_slice(),[delivery]
                    if delivery.kind()==UnitValueDeliveryKind::Temporary&&delivery.source()==&UnitValueDeliverySource::Temporary(expression(argument.expression))&&delivery.parameter_span()==Some(parameter.span)),
                    "delivery qualified real temporary Value transfer, no caller cleanup",
                    (loans, drops, deliveries),
                )?;
            }
        }
        need(
            typed.map_iterations().len() == usize::from(!control)
                && owned.map_iterations().len() == usize::from(!control),
            "delivery qualified actual plan count",
            typed.map_iterations(),
        )?;
        if let Some(site) = &input.site {
            let statement = UnitStatementId::new(unit, site.statement);
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("delivery qualified actual temporary plan", site))?;
            let d = p.descriptor();
            need(
                typed.map_iteration(statement) == Some(d)
                    && d.source() == expression(input.call)
                    && d.source_type() == call.return_type()
                    && p.source_access() == UnitIterationSourceAccess::Temporary
                    && p.source() == Some(&UnitLoanTarget::Temporary(expression(input.call))),
                "delivery qualified independent output hidden owner",
                p,
            )?;
            let read = typed
                .call(expression(site.read))
                .ok_or_else(|| failure("delivery qualified reader Call", site.read))?;
            need(
                read.target() == UnitCallTarget::Declaration(declaration(input.reader.name)?)
                    && read.arguments().len() == 2,
                "delivery qualified actual reader",
                read,
            )?;
            for (index, loan) in p.loans().iter().enumerate() {
                let reference = one(
                    names
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::expression(parsed, site.arguments[index])),
                    "delivery qualified component reference",
                )?;
                let binding = d.bindings()[index];
                need(
                    binding.source_unit() == unit
                        && resolution
                            .resolution()
                            .symbols()
                            .get(binding.symbol().index())
                            .is_some_and(|s| s.span() == site.bindings[index])
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && loan.source() == expression(input.call)
                        && loan.component()
                            == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                        && loan.statement() == statement
                        && loan.binding() == binding
                        && loan.origin() == &UnitLoanTarget::Temporary(expression(input.call))
                        && loan.kind() == LoanKind::Shared
                        && matches!(reference.target(),UnitReferenceTarget::Symbol(root) if *root==binding)
                        && read.arguments()[index].argument_index() == index
                        && read.arguments()[index].parameter_index() == index
                        && read.arguments()[index].mode() == ParameterMode::Borrow
                        && read.arguments()[index].parameter_type()
                            == [d.key_type(), d.value_type()][index],
                    "delivery qualified real Shared K/V and ordered reader roots",
                    (loan, reference),
                )?;
            }
            // Old FE owner/condition/phi observations are carried by the graph's
            // exact current roles, all incoming edges, complete inventory and
            // argument/output retirement; this getter proves only Unit Drop actions.
            cleanup::unit(
                UnitStatementId::new(unit, site.body),
                site.call,
                statement,
                expression(input.call),
                owned,
            )?;
        } else {
            let root = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter_map(|r| match r.target() {
                        UnitReferenceTarget::Symbol(root)
                            if root.source_unit() == unit
                                && resolution
                                    .resolution()
                                    .symbols()
                                    .get(root.symbol().index())
                                    .is_some_and(|s| Some(s.span()) == input.control_root) =>
                        {
                            Some(*root)
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter(),
                "delivery qualified actual control root",
            )?;
            let size_id = input.size.unwrap();
            let size = typed.map_size(expression(size_id)).ok_or_else(|| {
                failure("delivery qualified actual control size descriptor", size_id)
            })?;
            let Expression::Member { receiver, .. } =
                parsed.ast().expressions().get(size_id).unwrap().payload()
            else {
                return Err(failure("delivery qualified actual size member", size_id));
            };
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::expression(parsed, *receiver)),
                "delivery qualified control size original root reference",
            )?;
            need(
                size.expression() == expression(size_id)
                    && size.receiver() == expression(*receiver)
                    && size.map_type() == call.return_type()
                    && matches!(reference.target(),UnitReferenceTarget::Symbol(actual) if *actual==root),
                "delivery qualified size canonical expression/type/actual root",
                (size, reference),
            )?;
            let drop = one(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(root)),
                "delivery qualified output control Drop",
            )?;
            need(
                drop.value_origin() == input.control_root.unwrap()
                    && drop.point()
                        == if resource {
                            UnitDropPoint::AfterStatement(UnitStatementId::new(
                                unit,
                                input.entry.body,
                            ))
                        } else {
                            UnitDropPoint::CallReturn(size.expression())
                        },
                "delivery qualified lexical Resource/Pure-size output cleanup",
                drop,
            )?;
        }
        Ok(())
    }
}
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Delivery {
    BorrowAndOwnedResource,
    SameMapBorrow,
    ReorderedMixed,
}

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub control: String,
    pub golden: String,
    pub control_golden: String,
    pub delivery: Delivery,
    pub mutable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for delivery in [
        Delivery::BorrowAndOwnedResource,
        Delivery::SameMapBorrow,
        Delivery::ReorderedMixed,
    ] {
        for mutable in [false, true] {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let conversion = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let resource =
                "class Resource(val tag:String){deinit(){println(\"deinit\");println(this.tag)}}";
            let (tag, declarations, parameters, before, value_type, value, call, prefix, read) =
                match delivery {
                    Delivery::BorrowAndOwnedResource => (
                        "borrow-owned-resource",
                        format!("{resource}\nfun borrowedArgument():Resource{{println(\"borrow-argument\");return Resource(\"borrow-drop\".clone())}}\nfun ownedArgument():Resource{{println(\"owned-argument\");return Resource(\"owned-drop\".clone())}}"),
                        "input:Resource,own payload:Resource",
                        "println(input.tag)",
                        "Resource",
                        "payload",
                        "produce(borrowedArgument(),ownedArgument())",
                        "borrow-argument\nowned-argument\nfactory\nborrow-drop\ndeinit\nborrow-drop\n",
                        "println(value.tag)",
                    ),
                    Delivery::SameMapBorrow => (
                        "same-map-borrow",
                        "fun borrowedArgument():Map<String,String>{println(\"borrow-argument\");var source=mutableMapOf<String,String>();source.put(\"input\".clone(),\"input-value\".clone());val output=source.consume();return output}".to_owned(),
                        "input:Map<String,String>",
                        "if(input.size!=1){error(\"bad-input-size\")};println(\"input-size\")",
                        "String",
                        "\"value\".clone()",
                        "produce(borrowedArgument())",
                        "borrow-argument\nfactory\ninput-size\n",
                        "println(value)",
                    ),
                    Delivery::ReorderedMixed => (
                        "reordered-mixed",
                        format!("{resource}\nfun leftArgument():String{{println(\"left-argument\");return \"left\".clone()}}\nfun rightArgument():String{{println(\"right-argument\");return \"right\".clone()}}\nfun ownedArgument():Resource{{println(\"owned-argument\");return Resource(\"owned-drop\".clone())}}"),
                        "left:String,right:String,own payload:Resource",
                        "println(left);println(right)",
                        "Resource",
                        "payload",
                        "produce(right=rightArgument(),payload=ownedArgument(),left=leftArgument())",
                        "right-argument\nowned-argument\nleft-argument\nfactory\nleft\nright\n",
                        "println(value.tag)",
                    ),
                };
            let common = format!(
                "{declarations}\nfun produce({parameters}):{kind}<String,{value_type}>{{println(\"factory\");{before};var result=mutableMapOf<String,{value_type}>();result.put(\"key\".clone(),{value});{conversion}}}\nfun read(key:String,value:{value_type}):Unit{{println(key);{read}}}\n"
            );
            let source = format!(
                "{common}fun entry():Unit{{println(\"begin\");for((key,value) in {call}){{read(key,value)}};println(\"after\")}}\n"
            );
            let control = format!(
                "{common}fun entry():Unit{{println(\"begin\");val source={call};if(source.size==1){{println(\"control\")}}else{{error(\"bad-output-size\")}};println(\"after\")}}\n"
            );
            let (body, drop) = match delivery {
                Delivery::SameMapBorrow => ("key\nvalue\n", ""),
                _ => ("key\nowned-drop\n", "deinit\nowned-drop\n"),
            };
            cases.push(Case {
                label: format!("temporary-delivery-{tag}-{kind}"),
                source,
                control,
                golden: format!("begin\n{prefix}{body}{drop}after\n"),
                control_golden: format!("begin\n{prefix}control\nafter\n{drop}"),
                delivery,
                mutable,
            });
        }
    }
    assert_eq!(cases.len(), 6);
    cases
}

fn observe(case: &Case, rooted: bool, control: bool) -> Result<(), Failure> {
    let (text, golden) = if control {
        (&case.control, &case.control_golden)
    } else {
        (&case.source, &case.golden)
    };
    diagnostic!(
        "DELIVERY INPUT case={} rooted={rooted} control={control} source={text:?} golden={golden:?}",
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
                "delivery qualified selected arguments/cleanup",
                &facts::unit(case, &sources, &parsed, n, t, o, control),
            );
        },
    );
    diagnostic!(
        "DELIVERY LOWER {} rooted={rooted} control={control} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit original temporary entry and actual canonical owner acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    let deep = graph::check(case, &sources, &parsed, &program, control);
    report.query("delivery actual SSA owner/argument graph", &deep);
    diagnostic!(
        "DELIVERY GRAPH {} rooted={rooted} control={control} deep={deep:?}",
        case.label
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE/SSA queries", report));
    }
    Ok(())
}

fn check(control: bool) {
    let mut outcomes = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            let result = observe(&case, rooted, control);
            diagnostic!(
                "DELIVERY CASE {} rooted={rooted} control={control} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), rooted, result));
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

#[test]
fn ordinary_borrow_and_owned_delivery_reaches_both_pipelines() {
    check(true);
}

#[test]
fn temporary_source_uses_ordinary_borrow_and_owned_delivery() {
    check(false);
}
