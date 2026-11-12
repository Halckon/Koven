//! 同次公开 FE facts 的独立身份桥；不调用生产 qualification/proof。
#[path = "query_cleanup.rs"]
mod query_cleanup;
mod query_guards {
    //! 两个独立 Resource local 共用真实 Guard 类型与析构；形成不冒充当前 owner。
    use super::super::ast::{Anchors, Input};
    use super::{Failure, failure, need, one, unit_bridge};
    use lang_frontend::{
        name_resolution::{
            Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::CompilationUnitOwnership,
        type_checking::{
            CompilationUnitTypes, ExpressionCategory, NominalKind, ParameterMode,
            UnitConstructionTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        unit: SourceUnitId,
    ) -> Result<[UnitSymbolId; 2], Failure> {
        let mut locals = Vec::new();
        let mut classes = Vec::new();
        let mut types = Vec::new();
        for guard in &a.guards {
            // 普通 Guard 无 Name 读取；只从真实 formed 表取 qualified ID，再回核 names marker。
            let local = one(
                owned.formed_owned_locals().iter().copied().filter(|id| {
                    id.source_unit() == unit
                        && unit_bridge::binding_at(names, *id, guard.marker, SymbolKind::Variable)
                            .is_ok()
                }),
                "temporary exits FE unique actual qualified Guard formation",
            )?;
            unit_bridge::binding_at(names, local, guard.marker, SymbolKind::Variable)?;
            let class = one(
                names.names().index().declarations().iter().filter(|d| {
                    d.source_unit() == unit
                        && d.root() == guard.class_item
                        && d.name_span() == guard.class_marker
                        && d.kind() == SymbolKind::Classifier
                        && d.namespace() == Namespace::Type
                }),
                "temporary exits FE unique actual qualified Guard declaration",
            )?
            .id();
            let nominal = one(
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter(|d| d.declaration() == class)
                    .filter_map(|d| d.nominal()),
                "temporary exits FE unique qualified Guard nominal",
            )?;
            let expression = UnitExpressionId::new(unit, guard.initializer);
            let construction = typed.construction(expression).ok_or_else(|| {
                failure(
                    "temporary exits FE actual qualified Guard construction",
                    guard,
                )
            })?;
            let deinit = nominal
                .deinit()
                .ok_or_else(|| failure("temporary exits FE actual qualified Guard deinit", nominal))?;
            need(
                nominal.declaration() == class
                    && nominal.kind() == NominalKind::Class
                    && nominal.fields().is_empty()
                    && nominal.type_parameters().is_empty()
                    && deinit.owner() == class
                    && deinit.item() == UnitItemId::new(unit, guard.deinit_item)
                    && deinit.body() == UnitStatementId::new(unit, guard.deinit_body)
                    && deinit.receiver_mode() == ParameterMode::Borrow
                    && deinit.receiver_type() == construction.result_type()
                    && construction.expression() == expression
                    && construction.arguments().is_empty()
                    && construction.instance().type_arguments().is_empty()
                    && construction.target() == UnitConstructionTarget::Nominal(class)
                    && matches!(typed.types().get(construction.result_type()),
                    Some(UnitTypeKind::Nominal { declaration, arguments }) if *declaration == class && arguments.is_empty())
                    && typed.expression_type(expression) == Some(construction.result_type())
                    && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                    && typed.symbol_type(local) == Some(construction.result_type())
                    && typed.is_resource_type(construction.result_type()) == Some(true),
                "temporary exits FE qualified Guard Resource formation, nominal and real deinit",
                (guard, local, nominal, construction, deinit),
            )?;
            unit_bridge::callee(f, names, guard.initializer, class, Namespace::Type)?;
            locals.push(local);
            classes.push(class);
            types.push(construction.result_type());
        }
        need(
            locals.len() == 2
                && locals[0] != locals[1]
                && classes[0] == classes[1]
                && types[0] == types[1],
            "temporary exits FE qualified Guard locals distinct and same class/type",
            (&locals, classes, types),
        )?;
        locals
            .try_into()
            .map_err(|v| failure("temporary exits FE two actual qualified Guard locals", v))
    }
}
mod unit_bridge {
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
            IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
            UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
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
            "temporary exits FE unique actual qualified function declaration",
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
            "temporary exits FE actual symbol source unit",
        )?;
        let symbol = one(
            source
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.id() == id.symbol()),
            "temporary exits FE actual qualified local symbol",
        )?;
        need(
            symbol.span() == span
                && symbol.kind() == kind
                && !symbol.is_synthetic()
                && symbol.namespace() == Namespace::Value,
            "temporary exits FE qualified symbol matches exact marker and kind",
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
            "temporary exits FE actual unit resolution",
        )?;
        let actual = one(
            source
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.span() == span && !s.is_synthetic()),
            "temporary exits FE actual source marker symbol",
        )?;
        need(
            actual.kind() == kind && actual.namespace() == Namespace::Value,
            "temporary exits FE actual unit symbol kind",
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
            "temporary exits FE unique actual referenced qualified symbol",
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
            "temporary exits FE direct qualified Name operand",
            value,
        )?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span),
            "temporary exits FE unique actual qualified reference",
        )?;
        let UnitReferenceTarget::Symbol(id) = reference.target() else {
            return Err(failure(
                "temporary exits FE actual qualified Name target",
                reference,
            ));
        };
        let source = one(
            names.names().source_units().iter(),
            "temporary exits FE reference source unit",
        )?;
        need(
            reference.namespace() == Some(Namespace::Value)
                && reference.source_unit() == source.source_unit()
                && id.source_unit() == source.source_unit(),
            "temporary exits FE ordinary qualified reference source and Value namespace",
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
            "temporary exits FE actual qualified callee reference",
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
            "temporary exits FE callee source unit",
        )?;
        need(
            present
                && reference.namespace() == Some(namespace)
                && reference.source_unit() == source.source_unit(),
            "temporary exits FE selected declaration belongs to actual callee reference",
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
            "temporary exits FE actual sole source unit",
        )?;
        let unit = source_names.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        need(
            source_names.resolution().source_id() == f.parsed.source_id()
                && typed.map_iterations().len() == 2
                && owned.map_iterations().len() == 2,
            "temporary exits FE qualified source and exact two plans",
            (unit, typed.map_iterations(), owned.map_iterations()),
        )?;
        signatures(a, names, typed, owned, unit)?;
        let mut bindings = Vec::new();
        let mut component_loans = Vec::new();
        for site in &a.sites {
            let statement = UnitStatementId::new(unit, site.statement);
            let source = expression(site.source);
            let d = typed
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary exits FE actual typed For", site))?;
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary exits FE actual owned For", site))?;
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
                    && typed.sequential_iteration(statement).is_none()
                    && owned.iteration(statement).is_none()
                    && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                    && [d.key_type(), d.value_type()].iter().all(|ty| {
                        typed.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    })
                    && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                    && p.source_access() == UnitIterationSourceAccess::Temporary
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                    && p.source() == Some(&UnitLoanTarget::Temporary(source)),
                "temporary exits FE exact owned Map/Mutable String pair and temporary Call source",
                (site, d, p),
            )?;
            let factory = anchor(a, site.factory)?;
            let target = declaration(names, unit, factory)?;
            let call = typed
                .call(source)
                .ok_or_else(|| failure("temporary exits FE actual zeroarg factory Call", site))?;
            need(
                call.expression() == source
                    && call.target() == UnitCallTarget::Declaration(target)
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && call.return_type() == d.source_type()
                    && !owned.loans().iter().any(|loan| loan.call() == source)
                    && owned.call_argument_contracts_for(source).next().is_none()
                    && !owned
                        .value_deliveries()
                        .iter()
                        .any(|delivery| delivery.call() == source),
                "temporary exits FE factory actual declaration Call and Owned result",
                call,
            )?;
            callee(f, names, site.source, target, Namespace::Value)?;
            for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let binding = d.bindings()[i];
                let loan = p.loan(component);
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
                        && loan.origin() == &UnitLoanTarget::Temporary(source)
                        && !owned
                            .drops()
                            .iter()
                            .any(|drop| drop.target() == UnitDropTarget::Named(binding)),
                    "temporary exits FE exact component marker Shared type and source lineage",
                    (site, binding, loan),
                )?;
                bindings.push(binding);
                component_loans.push(loan);
            }
            let consumer = Call {
                expression: site.read,
                span: site.read_span,
                target: site.consumer,
                arguments: site.arguments.to_vec(),
            };
            borrow_call(f, a, &consumer, names, (typed, owned), &d.bindings())?;
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
                    .all(|(i, l)| !component_loans[..i].contains(l)),
            "temporary exits FE all four component identities stay independent",
            (bindings, component_loans),
        )?;
        if let Some(read) = &a.operand {
            let roots = read
                .binding_markers
                .iter()
                .map(|marker| symbol(names, unit, *marker, SymbolKind::ValueParameter))
                .collect::<Result<Vec<_>, _>>()?;
            borrow_call(f, a, &read.call, names, (typed, owned), &roots)?;
        }
        callers(f, a, names, typed, owned, unit)?;
        let guards = query_guards::unit(f, a, names, typed, owned, unit)?;
        let drops = query_cleanup::unit(f, a, guards, owned, unit)?;
        spans(f, a, drops, unit)
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
            .ok_or_else(|| failure("temporary exits FE nonempty qualified call roots", roots))?;
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
            "temporary exits FE actual qualified callee signature",
        )?;
        let call = typed
            .call(call_id)
            .ok_or_else(|| failure("temporary exits FE actual qualified typed call", site))?;
        need(
            call.expression() == call_id
                && call.target() == UnitCallTarget::Declaration(target)
                && call.receiver().is_none()
                && call.arguments().len() == roots.len()
                && site.arguments.len() == roots.len()
                && signature.parameters().len() == roots.len()
                && call.return_type() == signature.return_type()
                && typed.expression_type(call_id) == Some(call.return_type())
                && call.result_source() == CallableResultSource::Owned
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(expected_result(site.target))),
            "temporary exits FE actual qualified read/scan target signature and arity",
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
            "temporary exits FE qualified exact SharedLoan contracts and same-table end accessor",
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
                "temporary exits FE qualified argument parameter index and Borrow root exact",
                (site, index, actual, parameter, root),
            )?;
            let contract = one(
                contracts
                    .iter()
                    .copied()
                    .filter(|c| c.argument() == expression(*argument)),
                "temporary exits FE unique actual qualified argument contract",
            )?;
            let loan = one(
                loans
                    .iter()
                    .copied()
                    .filter(|l| l.argument() == expression(*argument)),
                "temporary exits FE unique actual qualified argument loan",
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
                "temporary exits FE qualified exact loan root contract and synchronous call spans",
                (contract, loan),
            )?;
        }
        Ok(())
    }

    fn signatures(
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
                "temporary exits FE actual unique qualified function signature",
            )?;
            need(
                signature.name_span() == function.name_span
                    && signature.target() == UnitCallableTarget::Declaration(id)
                    && signature.receiver().is_none()
                    && signature.type_parameters().is_empty()
                    && signature.has_body()
                    && signature.result_source() == CallableResultSource::Owned
                    && signature.parameters().len() == function.parameters.len(),
                "temporary exits FE actual qualified function declaration and signature shape",
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
                        && typed.types().get(parameter.ty())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
                    "temporary exits FE actual ordered qualified Borrow parameter",
                    (function, i, parameter, id),
                )?;
            }
            need(
                if a.sites.iter().any(|site| site.factory == function.name) {
                    Some(signature.return_type()) == source_type
                } else {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(expected_result(function.name)))
                },
                "temporary exits FE actual qualified function return contract",
                (function, signature),
            )?;
        }
        Ok(())
    }

    fn callers(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        unit: SourceUnitId,
    ) -> Result<(), Failure> {
        let expression = |id| UnitExpressionId::new(unit, id);
        let expected = if f.exit == super::super::ast::Exit::Return {
            2
        } else {
            0
        };
        need(
            a.scans.len() == expected && a.return_checks.len() == expected,
            "temporary exits FE caller coverage before indexing",
            (&a.scans, &a.return_checks),
        )?;
        let mut locals = Vec::new();
        for (index, (scan, check)) in a.scans.iter().zip(&a.return_checks).enumerate() {
            let target = declaration(names, unit, anchor(a, scan.target)?)?;
            let call = typed
                .call(expression(scan.expression))
                .ok_or_else(|| failure("temporary exits FE actual scanner call", scan))?;
            need(
                scan.arguments.is_empty()
                    && call.expression() == expression(scan.expression)
                    && call.target() == UnitCallTarget::Declaration(target)
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    && typed.expression_type(expression(scan.expression)) == Some(call.return_type())
                    && !owned
                        .loans()
                        .iter()
                        .any(|loan| loan.call() == expression(scan.expression))
                    && owned
                        .call_argument_contracts_for(expression(scan.expression))
                        .next()
                        .is_none()
                    && !owned
                        .value_deliveries()
                        .iter()
                        .any(|delivery| delivery.call() == expression(scan.expression)),
                "temporary exits FE zeroarg scanner exact Int Call result",
                (scan, call),
            )?;
            callee(f, names, scan.expression, target, Namespace::Value)?;
            let local = symbol(names, unit, check.marker, SymbolKind::Variable)?;
            need(
                check.expected == if index == 0 { "7" } else { "0" }
                    && check.initializer == scan.expression
                    && reference(f, names, check.reference)? == local
                    && owned.formed_owned_locals().contains(&local)
                    && typed.symbol_type(local) == Some(call.return_type())
                    && typed.expression_type(expression(check.reference)) == Some(call.return_type()),
                "temporary exits FE caller formed marker and exact scan-return reference",
                (check, local, call),
            )?;
            locals.push(local);
        }
        need(
            locals
                .iter()
                .enumerate()
                .all(|(i, v)| !locals[..i].contains(v)),
            "temporary exits FE caller local identities distinct",
            locals,
        )
    }
}

use super::{
    super::{
        fixture::{Failure, contains, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Exit, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::CompilationUnitOwnership,
    parser::{BinaryOperator, Expression},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypes},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct GuardInput {
    pub marker: Span,
    pub construction: Span,
    pub deinit: Span,
    pub class_marker: Span,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct SiteInput {
    pub function: Span,
    pub span: Span,
    pub source: Span,
    pub factory: Span,
    pub body: Span,
    pub consumer: Span,
    pub read_call: Span,
    pub arguments: [Span; 2],
    pub transfer: Span,
    pub drop: Span,
    pub guard: GuardInput,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct OperandInput {
    pub function: Span,
    pub call: Span,
    pub arguments: [Span; 2],
}
#[derive(Clone, Copy, Debug)]
pub(super) struct CallerInput {
    pub function: Span,
    pub call: Span,
    pub returned: Span,
    pub expected: Span,
}
#[derive(Debug)]
pub(super) struct Inputs {
    pub unit: SourceUnitId,
    pub entry: Span,
    pub read: Span,
    pub sites: [SiteInput; 2],
    pub operand: Option<OperandInput>,
    pub callers: Vec<CallerInput>,
}

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    unit_bridge::collect(f, a, names, typed, owned)
}
fn anchor<'a>(a: &'a Anchors, name: &str) -> Result<&'a Function, Failure> {
    one(
        a.functions.iter().filter(|v| v.name == name),
        "temporary exits FE unique actual function AST",
    )
}
fn expected_result(name: &str) -> BuiltinType {
    if matches!(name, "operand" | "scanNonempty" | "scanEmpty") {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}
fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("temporary exits FE actual Call", value));
    };
    let (callee, span) = f.expression(*callee)?;
    need(
        matches!(callee, Expression::Name),
        "temporary exits FE actual direct Name callee",
        callee,
    )?;
    Ok(span)
}

fn spans(f: &Input, a: &Anchors, drops: [Span; 2], unit: SourceUnitId) -> Result<Inputs, Failure> {
    let entry = anchor(a, "entry")?;
    let read = anchor(a, "read")?;
    let returning = f.exit == Exit::Return;
    need(
        a.scans.len() == if returning { 2 } else { 0 }
            && a.return_checks.len() == a.scans.len()
            && a.operand.is_some() == returning
            && a.sites[0].statement != a.sites[1].statement
            && a.sites[0].source != a.sites[1].source
            && a.sites[0].transfer != a.sites[1].transfer
            && a.sites[0].span.end() <= a.sites[1].span.start(),
        "temporary exits FE independent two sites and exact Return-only caller coverage",
        a,
    )?;
    let mut sites = Vec::new();
    for (index, site) in a.sites.iter().enumerate() {
        let function = anchor(
            a,
            if returning {
                if index == 0 {
                    "scanNonempty"
                } else {
                    "scanEmpty"
                }
            } else {
                "entry"
            },
        )?;
        let guard = &a.guards[index];
        let body = f
            .parsed
            .ast()
            .statements()
            .get(site.body)
            .map_err(|e| failure("temporary exits FE actual body span", e))?
            .span();
        let transfer = f.expression(site.transfer)?.1;
        let arguments = [
            f.expression(site.arguments[0])?.1,
            f.expression(site.arguments[1])?.1,
        ];
        need(
            contains(function.span, site.span)
                && contains(site.span, body)
                && contains(body, site.read_span)
                && contains(body, transfer)
                && contains(body, guard.span)
                && guard.marker.end() <= guard.span.start()
                && guard.span.end() <= site.read_span.start()
                && site.consumer == if returning { "operand" } else { "read" }
                && drops[index] == site.source_span
                && site.span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko")
                && function.parameters.is_empty(),
            "temporary exits FE real enclosing function, construction, consumer and transfer spans",
            (site, guard, function),
        )?;
        sites.push(SiteInput {
            function: function.span,
            span: site.span,
            source: site.source_span,
            factory: anchor(a, site.factory)?.span,
            body,
            consumer: anchor(a, site.consumer)?.span,
            read_call: site.read_span,
            arguments,
            transfer,
            drop: drops[index],
            guard: GuardInput {
                marker: guard.marker,
                construction: guard.span,
                deinit: guard.deinit_span,
                class_marker: guard.class_marker,
            },
        });
    }
    let operand = if let Some(value) = &a.operand {
        let [key, value_arg] = value.call.arguments.as_slice() else {
            return Err(failure(
                "temporary exits FE actual operand read argument pair",
                value,
            ));
        };
        let function = anchor(a, "operand")?;
        need(
            value.binding_markers.as_slice() == function.parameters.as_slice()
                && value.call.target == "read"
                && contains(function.span, value.call.span),
            "temporary exits FE operand read keeps actual parameter roots",
            value,
        )?;
        Some(OperandInput {
            function: function.span,
            call: value.call.span,
            arguments: [f.expression(*key)?.1, f.expression(*value_arg)?.1],
        })
    } else {
        None
    };
    let mut callers = Vec::new();
    for (index, (scan, check)) in a.scans.iter().zip(&a.return_checks).enumerate() {
        let expected_text = if index == 0 { "7" } else { "0" };
        let expected = one(
            f.parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, node)| match node.payload() {
                    Expression::Binary {
                        left,
                        operator: BinaryOperator::Equal,
                        right,
                        ..
                    } if *left == check.reference => Some(*right),
                    _ => None,
                }),
            "temporary exits FE unique actual caller equality RHS",
        )?;
        let (literal, expected) = f.expression(expected)?;
        let returned = f.expression(check.reference)?.1;
        need(
            matches!(literal, Expression::Literal(_))
                && f.sources.slice(expected) == Ok(expected_text)
                && check.expected == expected_text
                && check.initializer == scan.expression
                && scan.arguments.is_empty()
                && scan.target
                    == if index == 0 {
                        "scanNonempty"
                    } else {
                        "scanEmpty"
                    }
                && contains(entry.span, scan.span)
                && contains(entry.span, returned)
                && scan.span.end() <= returned.start()
                && returned.end() <= expected.start(),
            "temporary exits FE actual zeroarg scanner and exact caller 7/0 comparison",
            (scan, check, expected),
        )?;
        callers.push(CallerInput {
            function: anchor(a, scan.target)?.span,
            call: scan.span,
            returned,
            expected,
        });
    }
    Ok(Inputs {
        unit,
        entry: entry.span,
        read: read.span,
        sites: sites
            .try_into()
            .map_err(|v| failure("temporary exits FE two source sites", v))?,
        operand,
        callers,
    })
}
