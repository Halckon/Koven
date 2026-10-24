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
        UnitCallArgumentOwnershipKind, UnitDropPoint, UnitDropTarget, UnitIterationSourceAccess,
        UnitLoanTarget, UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind,
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
            .filter(|d| d.target() == UnitDropTarget::Temporary(expression(argument.expression)))
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
        let size = typed
            .map_size(expression(size_id))
            .ok_or_else(|| failure("delivery qualified actual control size descriptor", size_id))?;
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
                        UnitDropPoint::AfterStatement(UnitStatementId::new(unit, input.entry.body))
                    } else {
                        UnitDropPoint::CallReturn(size.expression())
                    },
            "delivery qualified lexical Resource/Pure-size output cleanup",
            drop,
        )?;
    }
    Ok(())
}
