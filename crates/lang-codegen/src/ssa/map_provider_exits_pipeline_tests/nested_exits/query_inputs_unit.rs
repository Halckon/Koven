//! unit 的 source-qualified 公共事实；不手造 SymbolId、owner 或 loan-end 表。
use super::super::ast::{Anchors, Call, Function, Input};
use super::{
    Failure, Inputs, anchor, callee_span, expected_result, failure, need, one, query_cleanup,
    query_guards, spans,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicCallable, IntrinsicTypeConstructor, MapIterationKind, ParameterMode,
        UnitCallTarget, UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn declaration(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    function: &Function,
) -> Result<DeclarationId, Failure> {
    Ok(one(
        names.names().index().declarations().iter().filter(|d| {
            d.source_unit() == unit
                && d.root() == function.item
                && d.name_span() == function.name_span
                && d.kind() == SymbolKind::Function
                && d.namespace() == Namespace::Value
                && d.name() == function.name
        }),
        "nested exits FE unique actual qualified function declaration",
    )?
    .id())
}

pub(super) fn binding_at(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> Result<(), Failure> {
    let source = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == id.source_unit()),
        "nested exits FE actual symbol source unit",
    )?;
    let symbol = one(
        source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.id() == id.symbol()),
        "nested exits FE actual qualified local symbol",
    )?;
    need(
        symbol.span() == span
            && symbol.kind() == kind
            && !symbol.is_synthetic()
            && symbol.namespace() == Namespace::Value,
        "nested exits FE qualified symbol matches exact marker and kind",
        (id, symbol),
    )
}

pub(super) fn symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    span: Span,
    kind: SymbolKind,
) -> Result<UnitSymbolId, Failure> {
    let source = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == unit),
        "nested exits FE actual unit resolution",
    )?;
    let actual = one(
        source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.span() == span && !s.is_synthetic()),
        "nested exits FE actual source marker symbol",
    )?;
    need(
        actual.kind() == kind && actual.namespace() == Namespace::Value,
        "nested exits FE actual unit symbol kind",
        actual,
    )?;
    // qualified ID 仅取本次真实 reference；不调用 frontend 私有构造器。
    let mut found = Vec::new();
    for reference in names.names().references() {
        if let UnitReferenceTarget::Symbol(id) = reference.target()
            && id.source_unit() == unit
            && id.symbol() == actual.id()
            && !found.contains(id)
        {
            found.push(*id);
        }
    }
    one(
        found.into_iter(),
        "nested exits FE unique actual referenced qualified symbol",
    )
}

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let (value, span) = f.expression(expression)?;
    need(
        matches!(value, Expression::Name),
        "nested exits FE direct qualified Name operand",
        value,
    )?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span),
        "nested exits FE unique actual qualified reference",
    )?;
    let UnitReferenceTarget::Symbol(id) = reference.target() else {
        return Err(failure(
            "nested exits FE actual qualified Name target",
            reference,
        ));
    };
    let source = one(
        names.names().source_units().iter(),
        "nested exits FE reference source unit",
    )?;
    need(
        reference.namespace() == Some(Namespace::Value)
            && reference.source_unit() == source.source_unit()
            && id.source_unit() == source.source_unit(),
        "nested exits FE ordinary qualified reference source and Value namespace",
        reference,
    )?;
    Ok(*id)
}

pub(super) fn callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    selected: DeclarationId,
    namespace: Namespace,
) -> Result<(), Failure> {
    let span = callee_span(f, expression)?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span),
        "nested exits FE actual qualified callee reference",
    )?;
    let present = match reference.target() {
        UnitReferenceTarget::Declaration(id) => *id == selected,
        UnitReferenceTarget::OverloadSet(ids) => {
            namespace == Namespace::Value && ids.contains(&selected)
        }
        _ => false,
    };
    let source = one(
        names.names().source_units().iter(),
        "nested exits FE callee source unit",
    )?;
    need(
        present
            && reference.namespace() == Some(namespace)
            && reference.source_unit() == source.source_unit(),
        "nested exits FE selected declaration belongs to actual callee reference",
        reference,
    )
}

pub(super) fn collect(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let source_names = one(
        names.names().source_units().iter(),
        "nested exits FE actual sole source unit",
    )?;
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2
            && typed.map_sizes().len() == a.sizes.len(),
        "nested exits FE qualified source and exact plan/size coverage",
        (
            unit,
            typed.map_iterations(),
            owned.map_iterations(),
            typed.map_sizes(),
        ),
    )?;
    signatures(f, a, names, typed, owned, unit)?;
    let parameter = a
        .parameter()
        .ok_or_else(|| failure("nested exits FE unit scan parameter", a))?;
    let root = symbol(names, unit, parameter, SymbolKind::ValueParameter)?;
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("nested exits FE actual qualified typed For", statement))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("nested exits FE actual qualified owned For", statement))?;
        let constructor = if f.map {
            IntrinsicTypeConstructor::Map
        } else {
            IntrinsicTypeConstructor::MutableMap
        };
        let kind = if f.map {
            MapIterationKind::Map
        } else {
            MapIterationKind::MutableMap
        };
        need(
            d.statement() == statement
                && d.source() == source
                && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.symbol_type(root) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none()
                && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic {
                    constructor: actual, arguments
                }) if *actual == constructor && d.kind() == kind
                    && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|ty| {
                    typed.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                }),
            "nested exits FE exact qualified descriptor and String K/V intrinsic",
            (site, d, p),
        )?;
        need(
            reference(f, names, site.source)? == root
                && root.source_unit() == unit
                && owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
                && typed.expression_category(source) == Some(ExpressionCategory::Place)
                && p.source_access() == UnitIterationSourceAccess::Shared
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Place(place))
                    if place.is_root() && place.root() == root)
                && matches!(p.source(), Some(UnitLoanTarget::Place(place))
                    if place.is_root() && place.root() == root),
            "nested exits FE both qualified sources are same actual Borrow root",
            (site, root, p),
        )?;
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            binding_at(names, binding, site.bindings[i], SymbolKind::ForBinding)?;
            need(
                binding.source_unit() == unit
                    && binding == symbol(names, unit, site.bindings[i], SymbolKind::ForBinding)?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[i].symbol() == binding
                    && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[i]
                    && loan.statement() == statement
                    && loan.source() == source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                "nested exits FE qualified site-specific marker component and loan lineage",
                (site, binding, loan),
            )?;
            bindings.push(binding);
            component_loans.push(loan);
        }
    }
    need(
        bindings.len() == 4
            && bindings
                .iter()
                .enumerate()
                .all(|(i, b)| !bindings[..i].contains(b))
            && component_loans.len() == 4
            && component_loans
                .iter()
                .enumerate()
                .all(|(i, l)| !component_loans[..i].contains(l))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(root)),
        "nested exits FE qualified component identities distinct and Borrow root has no Drop",
        (bindings, component_loans, root),
    )?;
    let guards = query_guards::unit(f, a, names, typed, owned, unit)?;
    query_cleanup::unit(f, a, guards, owned, unit)?;
    for read in &a.reads {
        let roots = read
            .binding_markers
            .iter()
            .map(|marker| {
                let kind = if a.sites.iter().any(|site| site.bindings.contains(marker)) {
                    SymbolKind::ForBinding
                } else {
                    SymbolKind::ValueParameter
                };
                symbol(names, unit, *marker, kind)
            })
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, &read.call, names, (typed, owned), &roots)?;
    }
    if let Some(call) = &a.operand {
        let roots = a
            .sites
            .iter()
            .flat_map(|site| site.bindings)
            .map(|marker| symbol(names, unit, marker, SymbolKind::ForBinding))
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, call, names, (typed, owned), &roots)?;
    }
    let mut locals = Vec::new();
    let mut drops = Vec::new();
    for (index, call) in a.scans.iter().enumerate() {
        let (marker, initializer) = a.locals[index];
        let local = symbol(names, unit, marker, SymbolKind::Variable)?;
        let initializer = expression(initializer);
        let ty = typed
            .expression_type(initializer)
            .ok_or_else(|| failure("nested exits FE qualified initializer type", initializer))?;
        // formed 仅证明形成历史，真实调用/size/Drop 与 SSA 才证明当前活性。
        need(
            local.source_unit() == unit
                && owned.formed_owned_locals().contains(&local)
                && typed.symbol_type(local) == Some(ty)
                && Some(ty) == typed.expression_type(expression(a.sites[0].source))
                && typed.expression_category(initializer) == Some(ExpressionCategory::Temporary),
            "nested exits FE qualified caller owned formation and source type",
            (local, initializer, ty),
        )?;
        borrow_call(f, a, call, names, (typed, owned), &[local])?;
        let size = one(
            a.sizes.iter().filter(|s| s.root_marker == marker),
            "nested exits FE qualified caller actual unique size",
        )?;
        drops.push(query_cleanup::unit_caller(
            local,
            marker,
            expression(size.expression),
            owned,
        )?);
        locals.push(local);
    }
    need(
        locals.len() == 2 && locals[0] != locals[1],
        "nested exits FE two caller qualified local identities distinct",
        locals,
    )?;
    factory(f, a, names, typed, unit)?;
    for size in &a.sizes {
        let kind = if size.root_marker == parameter {
            SymbolKind::ValueParameter
        } else {
            SymbolKind::Variable
        };
        let expected_root = symbol(names, unit, size.root_marker, kind)?;
        let descriptor = typed
            .map_size(expression(size.expression))
            .ok_or_else(|| failure("nested exits FE qualified size descriptor", size))?;
        need(
            size.span.source_id() == f.parsed.source_id()
                && reference(f, names, size.receiver)? == expected_root
                && descriptor.expression() == expression(size.expression)
                && descriptor.receiver() == expression(size.receiver)
                && Some(descriptor.map_type()) == typed.expression_type(expression(size.receiver))
                && typed.symbol_type(expected_root) == Some(descriptor.map_type())
                && Some(descriptor.map_type())
                    == typed.expression_type(expression(a.sites[0].source))
                && typed
                    .expression_type(expression(size.expression))
                    .is_some_and(|ty| {
                        typed.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    }),
            "nested exits FE each qualified size retains exact original root",
            (size, expected_root, descriptor),
        )?;
    }
    let drops: [Span; 2] = drops
        .try_into()
        .map_err(|v| failure("nested exits FE two qualified caller Drops", v))?;
    return_checks(f, a, names, typed, unit)?;
    spans(f, a, drops, true)
}

fn borrow_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &ValidatedCompilationUnitNames,
    products: (&CompilationUnitTypes, &CompilationUnitOwnership),
    roots: &[UnitSymbolId],
) -> Result<(), Failure> {
    let (typed, owned) = products;
    let first = roots
        .first()
        .ok_or_else(|| failure("nested exits FE nonempty qualified call roots", roots))?;
    let unit = first.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let call_id = expression(site.expression);
    let target = declaration(names, unit, anchor(a, site.target)?)?;
    let signature = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|s| s.declaration() == target)
            .filter_map(|s| s.callable()),
        "nested exits FE actual qualified callee signature",
    )?;
    let call = typed
        .call(call_id)
        .ok_or_else(|| failure("nested exits FE actual qualified typed call", site))?;
    need(
        call.expression() == call_id
            && call.target() == UnitCallTarget::Declaration(target)
            && call.receiver().is_none()
            && call.arguments().len() == roots.len()
            && site.arguments.len() == roots.len()
            && signature.parameters().len() == roots.len()
            && call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && typed.types().get(call.return_type())
                == Some(&UnitTypeKind::Builtin(expected_result(f, site.target))),
        "nested exits FE actual qualified read/scan target signature and arity",
        (site, call, signature),
    )?;
    callee(f, names, site.expression, target, Namespace::Value)?;
    let loans = owned
        .loans()
        .iter()
        .filter(|l| l.call() == call_id)
        .collect::<Vec<_>>();
    let ending = owned.loans_ending_at(call_id).collect::<Vec<_>>();
    let contracts = owned
        .call_argument_contracts_for(call_id)
        .collect::<Vec<_>>();
    // unit getter 只按 call 筛同表，明确只用于一致性；终止遵循 UnitLoanFact 同步合同与 SSA BorrowEnd。
    need(
        loans.len() == roots.len()
            && ending == loans
            && contracts.len() == roots.len()
            && !owned.value_deliveries().iter().any(|d| d.call() == call_id),
        "nested exits FE qualified exact SharedLoan contracts and same-table end accessor",
        (&loans, &ending, &contracts),
    )?;
    for (index, (argument, root)) in site.arguments.iter().zip(roots).enumerate() {
        let actual = &call.arguments()[index];
        let parameter = &signature.parameters()[index];
        let argument_span = f.expression(*argument)?.1;
        need(
            root.source_unit() == unit
                && actual.argument_index() == index
                && actual.parameter_index() == index
                && actual.mode() == ParameterMode::Borrow
                && parameter.mode() == ParameterMode::Borrow
                && actual.parameter_type() == parameter.ty()
                && typed.expression_type(expression(*argument)) == Some(actual.parameter_type())
                && typed.expression_category(expression(*argument))
                    == Some(ExpressionCategory::Place)
                && typed.symbol_type(*root) == Some(actual.parameter_type())
                && reference(f, names, *argument)? == *root,
            "nested exits FE qualified argument parameter index and Borrow root exact",
            (site, index, actual, parameter, root),
        )?;
        let contract = one(
            contracts
                .iter()
                .copied()
                .filter(|c| c.argument() == expression(*argument)),
            "nested exits FE unique actual qualified argument contract",
        )?;
        let loan = one(
            loans
                .iter()
                .copied()
                .filter(|l| l.argument() == expression(*argument)),
            "nested exits FE unique actual qualified argument loan",
        )?;
        need(
            contract.call() == call_id
                && contract.parameter_index() == index
                && contract.parameter_type() == actual.parameter_type()
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.category() == ExpressionCategory::Place
                && !contract.crosses_thread()
                && contract.argument_span() == argument_span
                && contract.call_span() == site.span
                && contract.parameter_span() == Some(parameter.span())
                && contract.loan_begin_span() == argument_span
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == *root)
                && loan.begin_span() == argument_span
                && loan.end_span() == site.span
                && loan.parameter_span() == contract.parameter_span(),
            "nested exits FE qualified exact loan root contract and synchronous call spans",
            (contract, loan),
        )?;
    }
    Ok(())
}

fn factory(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let id = declaration(names, unit, anchor(a, a.factory.target)?)?;
    let call = typed
        .call(expression(a.factory.expression))
        .ok_or_else(|| failure("nested exits FE actual qualified factory Call", &a.factory))?;
    need(
        a.locals[0].1 == a.factory.expression
            && call.expression() == expression(a.factory.expression)
            && call.target() == UnitCallTarget::Declaration(id)
            && call.receiver().is_none()
            && call.arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && Some(call.return_type()) == typed.expression_type(expression(a.sites[0].source)),
        "nested exits FE first qualified owner is actual zero-argument factory result",
        call,
    )?;
    callee(f, names, a.factory.expression, id, Namespace::Value)?;
    let empty = typed
        .map_construction(expression(a.locals[1].1))
        .ok_or_else(|| {
            failure(
                "nested exits FE actual qualified empty initializer",
                a.locals[1],
            )
        })?;
    let (value, _) = f.expression(a.locals[1].1)?;
    need(
        matches!(value, Expression::Call { arguments, .. } if arguments.is_empty())
            && empty.expression() == expression(a.locals[1].1)
            && empty.callable()
                == if f.map {
                    IntrinsicCallable::MapOf
                } else {
                    IntrinsicCallable::MutableMapOf
                }
            && Some(empty.map_type()) == typed.expression_type(expression(a.sites[0].source)),
        "nested exits FE second qualified owner is actual empty intrinsic construction",
        empty,
    )
}

fn signatures(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    for function in &a.functions {
        let id = declaration(names, unit, function)?;
        let signature = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter(|s| s.declaration() == id)
                .filter_map(|s| s.callable()),
            "nested exits FE actual unique qualified function signature",
        )?;
        need(
            signature.name_span() == function.name_span
                && signature.target() == UnitCallableTarget::Declaration(id)
                && signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && signature.parameters().len() == function.parameters.len(),
            "nested exits FE actual qualified function declaration and signature shape",
            (function, signature),
        )?;
        let source_type = typed.expression_type(UnitExpressionId::new(unit, a.sites[0].source));
        for (i, parameter) in signature.parameters().iter().enumerate() {
            let id = symbol(
                names,
                unit,
                function.parameters[i],
                SymbolKind::ValueParameter,
            )?;
            need(
                parameter.mode() == ParameterMode::Borrow
                    && parameter.symbol() == Some(id)
                    && typed.symbol_type(id) == Some(parameter.ty())
                    && owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                    && if function.name == "scan" {
                        Some(parameter.ty()) == source_type
                    } else {
                        typed.types().get(parameter.ty())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    },
                "nested exits FE actual ordered qualified Borrow parameter",
                (function, i, parameter, id),
            )?;
        }
        need(
            if function.name == a.factory.target {
                Some(signature.return_type()) == source_type
            } else {
                typed.types().get(signature.return_type())
                    == Some(&UnitTypeKind::Builtin(expected_result(f, function.name)))
            },
            "nested exits FE actual qualified function return contract",
            (function, signature),
        )?;
    }
    Ok(())
}

fn return_checks(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        a.return_checks.len()
            == if f.exit == super::super::ast::Exit::Return {
                2
            } else {
                0
            },
        "nested exits FE exact result-reference coverage before indexing",
        &a.return_checks,
    )?;
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, unit, check.marker, SymbolKind::Variable)?;
        need(
            check.expected == if index == 0 { "7" } else { "0" }
                && check.initializer == a.scans[index].expression
                && reference(f, names, check.reference)? == root
                && typed.symbol_type(root) == typed.expression_type(expression(check.initializer))
                && typed.expression_type(expression(check.reference))
                    == typed.expression_type(expression(check.initializer))
                && typed
                    .expression_type(expression(check.initializer))
                    .is_some_and(|ty| {
                        typed.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    }),
            "nested exits FE qualified result initializer/reference Int identity",
            (check, root),
        )?;
    }
    Ok(())
}
