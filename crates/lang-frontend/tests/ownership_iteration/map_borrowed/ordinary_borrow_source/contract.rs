//! 从真实 AST/name/call 合同定位普通借用来源及 L0164 的精确 primary/label。
use super::super::{
    Report,
    fixture::{Blocked, Fixture, blocked, diagnostics},
    type_facts::{single_reference, symbol_at, unit_reference},
};
use lang_frontend::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameResolution, SymbolKind, ValidatedCompilationUnitNames},
    parser::{
        BorrowReturnSource, Expression, ForBinding, FunctionBody, FunctionForm,
        FunctionResultSource, Item, NameMarker, Statement,
    },
    source::Span,
    type_checking::{
        BorrowReturnContract, BorrowReturnOrigin, BuiltinType, CallableResultSource,
        CallableTarget, CompilationUnitTypes, IntrinsicTypeConstructor, ParameterMode, TypeId,
        TypeKind, TypedFile, UnitCallTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

#[derive(Debug)]
pub(super) struct Sites {
    pub call: ExpressionId,
    pub call_span: Span,
    pub argument: ExpressionId,
    pub argument_span: Span,
    pub view: Span,
    pub view_parameter: Span,
    pub scan_parameter: Span,
    pub returned: ExpressionId,
    pub returned_span: Span,
    pub marker: Span,
    pub from_source: Span,
}

pub(super) fn sites(fixture: &Fixture) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let functions = ast
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                parameters,
                form,
                ..
            } => Some((*name, parameters, form)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(view, parameters, form), (scan, scan_parameters, _)] = functions.as_slice() else {
        return Err(blocked("AST functions", "exactly view then scan required"));
    };
    if fixture.sources.slice(*view) != Ok("view") || fixture.sources.slice(*scan) != Ok("scan") {
        return Err(blocked("AST function names", (*view, *scan)));
    }
    let [parameter] = parameters.as_slice() else {
        return Err(blocked("AST view parameter", parameters));
    };
    let [scan_parameter] = scan_parameters.as_slice() else {
        return Err(blocked("AST scan parameter", scan_parameters));
    };
    let (NameMarker::Present(view_parameter), NameMarker::Present(scan_parameter)) =
        (parameter.name, scan_parameter.name)
    else {
        return Err(blocked(
            "AST parameters",
            "real source parameter names required",
        ));
    };
    let FunctionForm::Explicit {
        result_source: Some(FunctionResultSource::Borrow(syntax)),
        body: FunctionBody::Expression {
            expression: returned,
            ..
        },
        ..
    } = form
    else {
        return Err(blocked(
            "AST borrow result",
            "ordinary borrow expression body required",
        ));
    };
    let BorrowReturnSource::Parameter(NameMarker::Present(from_source)) = syntax.source else {
        return Err(blocked("AST from source", "real source marker required"));
    };
    let returned_node = ast
        .expressions()
        .get(*returned)
        .map_err(|error| blocked("AST returned source", error))?;
    if !matches!(returned_node.payload(), Expression::Name) {
        return Err(blocked(
            "AST returned source",
            "stable parameter Name required",
        ));
    }
    let loops = ast
        .statements()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Statement::For {
                binding,
                source,
                body,
                ..
            } => Some((binding, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(ForBinding::Destructuring { names, .. }, call, body)] = loops.as_slice() else {
        return Err(blocked("AST iteration", "one destructuring for required"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(blocked("AST iteration", "two real bindings required"));
    };
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST body", error))?;
    if !matches!(body.payload(), Statement::Block { elements } if elements.is_empty()) {
        return Err(blocked("AST body", "empty loop body required"));
    }
    let call_node = ast
        .expressions()
        .get(*call)
        .map_err(|error| blocked("AST call", error))?;
    let Expression::Call {
        callee, arguments, ..
    } = call_node.payload()
    else {
        return Err(blocked("AST call", "direct ordinary source call required"));
    };
    let [argument] = arguments.as_slice() else {
        return Err(blocked("AST argument", "exactly one argument required"));
    };
    let callee = ast
        .expressions()
        .get(*callee)
        .map_err(|error| blocked("AST callee", error))?;
    let argument_node = ast
        .expressions()
        .get(argument.value)
        .map_err(|error| blocked("AST argument", error))?;
    if !matches!(callee.payload(), Expression::Name)
        || !matches!(argument_node.payload(), Expression::Name)
    {
        return Err(blocked("AST call operands", "real ordinary names required"));
    }
    for (span, text) in [
        (view_parameter, "source"),
        (scan_parameter, "source"),
        (from_source, "source"),
        (returned_node.span(), "source"),
        (syntax.borrow_span, "borrow"),
        (callee.span(), "view"),
        (argument_node.span(), "source"),
        (call_node.span(), "view(source)"),
        (*key, "key"),
        (*value, "value"),
    ] {
        if fixture.sources.slice(span) != Ok(text) {
            return Err(blocked("AST spelling", (span, text)));
        }
    }
    Ok(Sites {
        call: *call,
        call_span: call_node.span(),
        argument: argument.value,
        argument_span: argument_node.span(),
        view: *view,
        view_parameter,
        scan_parameter,
        returned: *returned,
        returned_span: returned_node.span(),
        marker: syntax.borrow_span,
        from_source,
    })
}

fn contract_matches(contract: BorrowReturnContract, sites: &Sites) -> bool {
    contract.origin() == BorrowReturnOrigin::Parameter(0)
        && contract.marker_span() == sites.marker
        && contract.source_span() == sites.from_source
}

fn single_map(raw: &TypedFile, ty: Option<TypeId>) -> bool {
    matches!(ty.and_then(|ty| raw.types().get(ty)), Some(TypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::Map, arguments })
        if arguments.len() == 2 && arguments.iter().all(|ty| raw.types().get(*ty) == Some(&TypeKind::Builtin(BuiltinType::Int))))
}

fn unit_map(raw: &CompilationUnitTypes, ty: Option<UnitTypeId>) -> bool {
    matches!(ty.and_then(|ty| raw.types().get(ty)), Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::Map, arguments })
        if arguments.len() == 2 && arguments.iter().all(|ty| raw.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))))
}

pub(super) fn single(
    fixture: &Fixture,
    sites: &Sites,
    names: &NameResolution,
    raw: &TypedFile,
    report: &mut Report,
) {
    let view = symbol_at(names, sites.view);
    let source = symbol_at(names, sites.view_parameter);
    let caller = symbol_at(names, sites.scan_parameter);
    let returned = single_reference(names, sites.returned_span);
    let argument = single_reference(names, sites.argument_span);
    report.record(
        "helper RHS real parameter identity",
        source.is_some_and(|symbol| {
            symbol.kind() == SymbolKind::ValueParameter
                && returned == Some(symbol.id())
                && raw.parameter_mode(symbol.id()) == Some(ParameterMode::Borrow)
                && raw.symbol_type(symbol.id()) == raw.expression_type(sites.returned)
        }),
        (source, returned),
    );
    report.record(
        "caller argument real distinct parameter",
        caller.zip(source).is_some_and(|(caller, source)| {
            caller.kind() == SymbolKind::ValueParameter
                && caller.id() != source.id()
                && argument == Some(caller.id())
                && raw.parameter_mode(caller.id()) == Some(ParameterMode::Borrow)
        }),
        (caller, argument),
    );
    let call = raw.call(sites.call);
    report.record(
        "ordinary local view target",
        view.is_some_and(|view| {
            view.kind() == SymbolKind::Function
                && call.is_some_and(|call| call.target() == CallableTarget::Source(view.id()))
        }),
        (view, call.map(|call| call.target())),
    );
    report.record("ordinary Borrow result marker/origin", call.is_some_and(|call| {
        matches!(call.result_source(), CallableResultSource::Borrow(contract) if contract_matches(contract, sites))
            && raw.call_borrow_return(sites.call) == call.borrow_return()
    }), call.map(|call| call.result_source()));
    report.record(
        "source and returned intrinsic Map<Int,Int>",
        single_map(raw, raw.expression_type(sites.call))
            && single_map(raw, raw.expression_type(sites.returned)),
        (
            raw.expression_type(sites.call)
                .and_then(|ty| raw.types().get(ty)),
            raw.expression_type(sites.returned)
                .and_then(|ty| raw.types().get(ty)),
        ),
    );
    report.record("actual Borrow argument mapping", call.is_some_and(|call| matches!(call.arguments(), [argument]
        if argument.argument_index() == 0 && argument.parameter_index() == 0 && argument.mode() == ParameterMode::Borrow
            && single_map(raw, Some(argument.parameter_type()))
            && raw.expression_type(sites.argument) == Some(argument.parameter_type()))), call);
    report.note(
        "real marker/call source slices",
        (
            fixture.sources.slice(sites.marker),
            fixture.sources.slice(sites.call_span),
        ),
    );
}

pub(super) fn unit(
    fixture: &Fixture,
    sites: &Sites,
    names: &ValidatedCompilationUnitNames,
    raw: &CompilationUnitTypes,
    report: &mut Report,
) {
    let [source_names] = names.names().source_units() else {
        report.record(
            "unit source identity",
            false,
            names.names().source_units().len(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let view = symbol_at(source_names.resolution(), sites.view);
    let source = symbol_at(source_names.resolution(), sites.view_parameter);
    let caller = symbol_at(source_names.resolution(), sites.scan_parameter);
    let returned = unit_reference(names, sites.returned_span);
    let argument = unit_reference(names, sites.argument_span);
    report.record(
        "helper RHS real unit parameter identity",
        source.zip(returned).is_some_and(|(local, symbol)| {
            local.kind() == SymbolKind::ValueParameter
                && symbol.source_unit() == unit
                && symbol.symbol() == local.id()
                && raw.symbol_type(symbol) == raw.expression_type(expression(sites.returned))
        }),
        (source, returned),
    );
    report.record(
        "caller argument real distinct unit parameter",
        caller
            .zip(source)
            .zip(argument)
            .is_some_and(|((caller, source), argument)| {
                caller.kind() == SymbolKind::ValueParameter
                    && caller.id() != source.id()
                    && argument.source_unit() == unit
                    && argument.symbol() == caller.id()
            }),
        (caller, argument),
    );
    let call = raw.call(expression(sites.call));
    let target = call.and_then(|call| match call.target() {
        UnitCallTarget::Declaration(declaration) => names.names().declaration_symbol(declaration),
        _ => None,
    });
    report.record(
        "ordinary local unit view target",
        view.zip(target).is_some_and(|(view, target)| {
            view.kind() == SymbolKind::Function
                && target.source_unit() == unit
                && target.symbol() == view.id()
        }),
        (view, call.map(|call| call.target()), target),
    );
    report.record("ordinary unit Borrow result marker/origin", call.is_some_and(|call| {
        matches!(call.result_source(), CallableResultSource::Borrow(contract) if contract_matches(contract, sites))
            && raw.call_borrow_return(expression(sites.call)) == call.borrow_return()
    }), call.map(|call| call.result_source()));
    report.record(
        "source and returned unit intrinsic Map<Int,Int>",
        unit_map(raw, raw.expression_type(expression(sites.call)))
            && unit_map(raw, raw.expression_type(expression(sites.returned))),
        (
            raw.expression_type(expression(sites.call))
                .and_then(|ty| raw.types().get(ty)),
            raw.expression_type(expression(sites.returned))
                .and_then(|ty| raw.types().get(ty)),
        ),
    );
    report.record("actual unit Borrow argument mapping", call.is_some_and(|call| matches!(call.arguments(), [argument]
        if argument.argument_index() == 0 && argument.parameter_index() == 0 && argument.mode() == ParameterMode::Borrow
            && unit_map(raw, Some(argument.parameter_type()))
            && raw.expression_type(expression(sites.argument)) == Some(argument.parameter_type()))), call);
    report.note(
        "real marker/call source slices",
        (
            fixture.sources.slice(sites.marker),
            fixture.sources.slice(sites.call_span),
        ),
    );
}

pub(super) fn rejection(
    fixture: &Fixture,
    sites: &Sites,
    items: &[Diagnostic],
    report: &mut Report,
) {
    report.record(
        "ownership actual L0164",
        items.len() == 1 && items[0].code().to_string() == "L0164",
        diagnostics(&fixture.sources, "ownership", items),
    );
    let exact = items
        .first()
        .filter(|item| item.code().to_string() == "L0164");
    report.record(
        "L0164 exact source call primary",
        exact.is_some_and(|item| item.primary_span() == sites.call_span),
        exact.map(|item| item.primary_span()),
    );
    report.record(
        "L0164 exact declaration borrow label",
        exact.is_some_and(|item| {
            item.details().iter().any(|detail| {
        matches!(detail, DiagnosticDetail::Label(label) if label.span() == sites.marker)
    })
        }),
        items
            .iter()
            .flat_map(|item| item.details())
            .map(|detail| match detail {
                DiagnosticDetail::Label(label) => format!(
                    "label {:?} slice={:?} message={:?}",
                    label.span(),
                    fixture.sources.slice(label.span()),
                    label.message()
                ),
                other => format!("{other:?}"),
            })
            .collect::<Vec<_>>(),
    );
}
