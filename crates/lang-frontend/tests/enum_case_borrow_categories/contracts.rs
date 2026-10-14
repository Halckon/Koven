//! 只读产物断言；不调用生产分类器，不构造或改写 facts/IR。
use super::*;

pub(super) fn assert_case_descriptor(
    checked: &Evidence,
    text: &str,
    operands: usize,
    generic_int: bool,
) {
    assert_case_descriptor_at(checked, checked.expression(text), operands, generic_int);
}

pub(super) fn assert_case_descriptor_at(
    checked: &Evidence,
    id: ExpressionId,
    operands: usize,
    generic_int: bool,
) {
    let descriptor = checked
        .raw
        .construction(id)
        .expect("selected source construction");
    assert_eq!(descriptor.expression(), id);
    assert_eq!(descriptor.arguments().len(), operands);
    assert_eq!(
        checked.raw.expression_type(id),
        Some(descriptor.result_type())
    );
    let ConstructionTarget::EnumCase(case_id) = descriptor.target() else {
        panic!("EnumCase target");
    };
    let case = checked
        .raw
        .enum_cases()
        .iter()
        .find(|case| case.id() == case_id)
        .expect("same analysis enum identity");
    let token = case_token_span(&checked.file, id);
    let reference = checked
        .names
        .references()
        .iter()
        .find(|reference| reference.span() == token && reference.namespace() == Namespace::Value)
        .expect("real source case value reference");
    assert_eq!(
        reference.target(),
        &ReferenceTarget::Symbol(case.value_symbol()),
        "same root type must not substitute another enum case"
    );
    let Some(TypeKind::Nominal { nominal, arguments }) =
        checked.raw.types().get(descriptor.result_type())
    else {
        panic!("case result must be complete root enum, never case refinement");
    };
    assert_eq!(*nominal, case.root());
    assert_eq!(arguments, descriptor.instance().type_arguments());
    assert_eq!(arguments.len(), usize::from(generic_int));
    if generic_int {
        assert_eq!(
            checked.raw.types().get(arguments[0]),
            Some(&TypeKind::Builtin(BuiltinType::Int))
        );
    }
    for (index, argument) in descriptor.arguments().iter().enumerate() {
        assert_eq!(argument.mode(), ParameterMode::Value);
        assert_eq!(
            (argument.parameter_index(), argument.evaluation_index()),
            (index, index)
        );
    }
    let unit_id = UnitExpressionId::new(
        source_unit(&checked.unit.names, checked.file.source_id()),
        id,
    );
    assert_unit_case_descriptor(&checked.unit, &checked.file, unit_id, operands, generic_int);
}

pub(super) fn assert_unit_case_descriptor(
    checked: &UnitEvidence,
    file: &ParsedFile,
    id: UnitExpressionId,
    operands: usize,
    generic_int: bool,
) {
    let raw = &checked.raw;
    let descriptor = raw
        .construction(id)
        .expect("selected source-qualified construction");
    assert_eq!(descriptor.expression(), id);
    assert_eq!(descriptor.arguments().len(), operands);
    assert_eq!(raw.expression_type(id), Some(descriptor.result_type()));
    let UnitConstructionTarget::EnumCase(case_id) = descriptor.target() else {
        panic!("unit EnumCase target");
    };
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = raw.types().get(descriptor.result_type())
    else {
        panic!("unit case result must be complete root enum");
    };
    let nominal = raw
        .signatures()
        .declaration(*declaration)
        .and_then(|declaration| declaration.nominal())
        .expect("real root declaration");
    let case = nominal
        .enum_cases()
        .iter()
        .find(|case| case.value_symbol() == case_id)
        .expect("case belongs to the actual root declaration");
    let token = case_token_span(file, id.expression());
    let reference = checked
        .names
        .names()
        .references()
        .iter()
        .find(|reference| {
            reference.source_unit() == id.source_unit()
                && reference.span() == token
                && reference.namespace() == Some(Namespace::Value)
        })
        .expect("real source-qualified case value reference");
    assert_eq!(
        reference.target(),
        &UnitReferenceTarget::Symbol(case.value_symbol())
    );
    assert_eq!(arguments, descriptor.instance().type_arguments());
    assert_eq!(arguments.len(), usize::from(generic_int));
    if generic_int {
        assert_eq!(
            raw.types().get(arguments[0]),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    for (index, argument) in descriptor.arguments().iter().enumerate() {
        assert_eq!(argument.mode(), ParameterMode::Value);
        assert_eq!(
            (argument.parameter_index(), argument.evaluation_index()),
            (index, index)
        );
    }
}

pub(super) fn assert_argument_category(
    checked: &Evidence,
    call_text: &str,
    operand_text: &str,
    parameter_index: usize,
    mode: ParameterMode,
    category: ExpressionCategory,
) {
    let call_id = checked.expression(call_text);
    let operand = checked.expression(operand_text);
    assert_argument_category_at(checked, call_id, operand, parameter_index, mode, category);
}

pub(super) fn assert_argument_category_at(
    checked: &Evidence,
    call_id: ExpressionId,
    operand: ExpressionId,
    parameter_index: usize,
    mode: ParameterMode,
    category: ExpressionCategory,
) {
    let Expression::Call { arguments, .. } = checked
        .file
        .ast()
        .expressions()
        .get(call_id)
        .unwrap()
        .payload()
    else {
        panic!("real outer call AST");
    };
    let argument_index = arguments
        .iter()
        .position(|argument| argument.value == operand)
        .expect("operand belongs to this call");
    let call = checked.raw.call(call_id).expect("selected single callable");
    assert_eq!(call.expression(), call_id);
    let argument = call
        .arguments()
        .iter()
        .find(|argument| argument.argument_index() == argument_index)
        .expect("single source argument mapping");
    assert_eq!(argument.parameter_index(), parameter_index);
    assert_eq!((argument.mode(), argument.category()), (mode, category));
    assert_eq!(checked.raw.expression_category(operand), Some(category));
    assert_eq!(
        Some(argument.parameter_type()),
        checked.raw.expression_type(operand)
    );
    let source = source_unit(&checked.unit.names, checked.file.source_id());
    let unit_call_id = UnitExpressionId::new(source, call_id);
    let unit_operand = UnitExpressionId::new(source, operand);
    let call = checked
        .unit
        .raw
        .call(unit_call_id)
        .expect("selected unit callable");
    assert_eq!(call.expression(), unit_call_id);
    let argument = call
        .arguments()
        .iter()
        .find(|argument| argument.argument_index() == argument_index)
        .expect("unit source argument mapping");
    assert_eq!(argument.parameter_index(), parameter_index);
    assert_eq!((argument.mode(), argument.category()), (mode, category));
    assert_eq!(
        checked.unit.raw.expression_category(unit_operand),
        Some(category)
    );
    assert_eq!(
        Some(argument.parameter_type()),
        checked.unit.raw.expression_type(unit_operand)
    );
}

pub(super) fn assert_unit_loan<'a>(
    sources: &SourceMap,
    file: &ParsedFile,
    checked: &'a UnitEvidence,
    call: UnitExpressionId,
    operand: UnitExpressionId,
    kind: LoanKind,
    temporary: bool,
) -> &'a lang_frontend::ownership_checking::UnitLoanFact {
    let owned = checked.owned.accepted();
    let matching = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call && loan.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1, "one real loan for the source operand");
    let loan = matching[0];
    assert_eq!(loan.kind(), kind);
    assert_eq!(loan.end_span(), span(file, call.expression()));
    let contracts = owned
        .call_argument_contracts_for(call)
        .filter(|contract| contract.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(contracts.len(), 1);
    let contract = contracts[0];
    assert_eq!(contract.call(), call);
    assert_eq!(contract.argument_span(), span(file, operand.expression()));
    assert_eq!(contract.call_span(), span(file, call.expression()));
    assert_eq!(contract.loan_begin_span(), loan.begin_span());
    assert_eq!(contract.parameter_span(), loan.parameter_span());
    assert!(contract.parameter_span().is_some());
    assert_eq!(
        contract.kind(),
        if kind == LoanKind::Shared {
            UnitCallArgumentOwnershipKind::SharedLoan
        } else {
            UnitCallArgumentOwnershipKind::ExclusiveLoan
        }
    );
    let expected_category = if temporary {
        ExpressionCategory::Temporary
    } else {
        ExpressionCategory::Place
    };
    assert_eq!(contract.category(), expected_category);
    let typed_argument = checked
        .raw
        .call(call)
        .unwrap()
        .arguments()
        .iter()
        .find(|argument| argument.parameter_index() == contract.parameter_index())
        .unwrap();
    assert_eq!(contract.parameter_type(), typed_argument.parameter_type());
    if kind == LoanKind::Shared {
        assert_eq!(loan.begin_span(), span(file, operand.expression()));
    } else {
        assert_eq!(sources.slice(loan.begin_span()), Ok("&"));
    }
    if temporary {
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(operand));
    }
    assert_eq!(
        owned
            .loans_ending_at(call)
            .filter(|loan| loan.argument() == operand)
            .count(),
        1
    );
    loan
}

pub(super) fn assert_temporary_loan(
    checked: &Evidence,
    call_text: &str,
    operand_text: &str,
    construction_text: &str,
    move_only: bool,
    payload_kind: Option<ConstructionDeliveryKind>,
    generic_int: bool,
) {
    let call = checked.expression(call_text);
    let operand = checked.expression(operand_text);
    let construction = checked.expression(construction_text);
    let mut inner = operand;
    let mut groups = Vec::new();
    while let Expression::Group { expression } = checked
        .file
        .ast()
        .expressions()
        .get(inner)
        .unwrap()
        .payload()
    {
        groups.push(inner);
        inner = *expression;
    }
    assert_eq!(
        inner, construction,
        "group chain reaches the actual construction"
    );
    if operand_text != construction_text {
        assert_ne!(operand, construction);
        assert!(!groups.is_empty());
    }
    let descriptor = checked
        .raw
        .construction(construction)
        .expect("real construction descriptor");
    assert_case_descriptor(
        checked,
        construction_text,
        usize::from(payload_kind.is_some()),
        generic_int,
    );
    let owned = checked.accepted_single();
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call && loan.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(
        loans.len(),
        1,
        "single must publish a loan, not merely return Ok"
    );
    let loan = loans[0];
    assert_eq!(
        loan.target(),
        &LoanTarget::Temporary(operand),
        "a case symbol is not a binding Place"
    );
    assert_eq!(loan.kind(), LoanKind::Shared);
    assert_eq!(loan.begin_span(), span(&checked.file, operand));
    assert_eq!(loan.end_span(), span(&checked.file, call));
    let ends = owned
        .loan_ends()
        .iter()
        .filter(|end| end.call() == call && end.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].point(), LoanEndPoint::CallReturn(call));
    assert_eq!(
        owned
            .loans_ending_at(call)
            .filter(|loan| loan.argument() == operand)
            .count(),
        1
    );
    let unit_call = checked.unit_expression(call_text);
    let unit_operand = checked.unit_expression(operand_text);
    let unit_construction = checked.unit_expression(construction_text);
    assert_unit_loan(
        &checked.sources,
        &checked.file,
        &checked.unit,
        unit_call,
        unit_operand,
        LoanKind::Shared,
        true,
    );
    assert_argument_category(
        checked,
        call_text,
        operand_text,
        0,
        ParameterMode::Borrow,
        ExpressionCategory::Temporary,
    );

    let plan = owned
        .construction_plan(construction)
        .expect("single construction plan");
    assert_eq!(plan.target(), descriptor.target());
    assert!(plan.terminating_operand().is_none());
    let unit_owned = checked.unit.owned.accepted();
    let unit_plan = unit_owned
        .construction_plans()
        .iter()
        .find(|plan| plan.construction() == unit_construction)
        .expect("unit construction plan");
    let unit_descriptor = checked.unit.raw.construction(unit_construction).unwrap();
    for group in groups {
        let unit_group = UnitExpressionId::new(unit_operand.source_unit(), group);
        assert!(checked.raw.construction(group).is_none());
        assert!(checked.unit.raw.construction(unit_group).is_none());
        assert!(owned.construction_plan(group).is_none());
        assert!(
            !unit_owned
                .construction_plans()
                .iter()
                .any(|plan| plan.construction() == unit_group)
        );
        assert_eq!(
            checked.raw.expression_type(group),
            Some(descriptor.result_type())
        );
        assert_eq!(
            checked.unit.raw.expression_type(unit_group),
            Some(unit_descriptor.result_type())
        );
    }
    assert_eq!(unit_plan.target(), unit_descriptor.target());
    assert!(unit_plan.terminating_operand().is_none());
    assert_eq!(plan.deliveries().len(), descriptor.arguments().len());
    assert_eq!(
        unit_plan.deliveries().len(),
        unit_descriptor.arguments().len()
    );
    let copyability = if move_only {
        Copyability::MoveOnly
    } else {
        Copyability::Copyable
    };
    assert_eq!(
        checked.raw.copyability(descriptor.result_type()),
        Some(copyability)
    );
    assert_eq!(
        checked.unit.raw.copyability(unit_descriptor.result_type()),
        copyability
    );
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Temporary(operand))
        .collect::<Vec<_>>();
    let unit_drops = unit_owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Temporary(unit_operand))
        .collect::<Vec<_>>();
    if move_only {
        let root = plan
            .root_obligation()
            .expect("MoveOnly root including empty case");
        assert_eq!(
            (root.construction(), root.result_type(), root.kind()),
            (
                construction,
                descriptor.result_type(),
                ConstructionRootKind::Inline
            )
        );
        let root = unit_plan.root_obligation().expect("unit MoveOnly root");
        assert_eq!(
            (root.construction(), root.result_type(), root.kind()),
            (
                unit_construction,
                unit_descriptor.result_type(),
                ConstructionRootKind::Inline
            )
        );
        assert_eq!(drops.len(), 1, "unique owner consumption");
        assert_eq!(drops[0].point(), DropPoint::CallReturn(call));
        assert_eq!(drops[0].value_origin(), span(&checked.file, operand));
        assert_eq!(unit_drops.len(), 1, "unique unit owner consumption");
        assert_eq!(unit_drops[0].point(), UnitDropPoint::CallReturn(unit_call));
        assert_eq!(unit_drops[0].value_origin(), span(&checked.file, operand));
    } else {
        assert!(plan.root_obligation().is_none());
        assert!(unit_plan.root_obligation().is_none());
        assert!(drops.is_empty(), "Copyable has no unique drop");
        assert!(unit_drops.is_empty(), "unit Copyable has no unique drop");
    }
    if operand != construction {
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Temporary(construction)),
            "inner construction cannot create a second owner consumption"
        );
        assert!(
            !unit_owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Temporary(unit_construction))
        );
    }
    for (argument, delivery) in descriptor.arguments().iter().zip(plan.deliveries()) {
        let expected = payload_kind.expect("source fixture declares a payload");
        let category = if expected == ConstructionDeliveryKind::DeliverTemporary {
            ExpressionCategory::Temporary
        } else {
            ExpressionCategory::Place
        };
        assert_eq!(argument.category(), category);
        assert_eq!(
            (delivery.argument(), delivery.kind()),
            (argument.argument(), expected)
        );
        assert_eq!(
            (delivery.parameter_index(), delivery.evaluation_index()),
            (argument.parameter_index(), argument.evaluation_index())
        );
        assert_eq!(delivery.parameter_symbol(), argument.parameter_symbol());
        if expected == ConstructionDeliveryKind::Move {
            let origin = span(&checked.file, argument.argument());
            let reference = checked
                .names
                .references()
                .iter()
                .find(|reference| {
                    reference.span() == origin && reference.namespace() == Namespace::Value
                })
                .unwrap();
            let ReferenceTarget::Symbol(symbol) = reference.target() else {
                panic!("owned payload binding");
            };
            assert!(
                !owned
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == DropTarget::Named(*symbol)),
                "moved payload binding must not also be destroyed by the callee"
            );
        }
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Temporary(argument.argument())),
            "Value payload has transferred to its root, never independently double-dropped"
        );
    }
    for (argument, delivery) in unit_descriptor
        .arguments()
        .iter()
        .zip(unit_plan.deliveries())
    {
        let expected = payload_kind.expect("unit source fixture declares a payload");
        let category = if expected == ConstructionDeliveryKind::DeliverTemporary {
            ExpressionCategory::Temporary
        } else {
            ExpressionCategory::Place
        };
        assert_eq!(argument.category(), category);
        assert_eq!(
            (delivery.argument(), delivery.kind()),
            (argument.argument(), expected)
        );
        assert_eq!(
            (delivery.parameter_index(), delivery.evaluation_index()),
            (argument.parameter_index(), argument.evaluation_index())
        );
        assert_eq!(delivery.parameter_symbol(), argument.parameter_symbol());
        if expected == ConstructionDeliveryKind::Move {
            let origin = span(&checked.file, argument.argument().expression());
            let reference = checked
                .unit
                .names
                .names()
                .references()
                .iter()
                .find(|reference| {
                    reference.span() == origin && reference.namespace() == Some(Namespace::Value)
                })
                .unwrap();
            let UnitReferenceTarget::Symbol(symbol) = reference.target() else {
                panic!("unit owned payload binding");
            };
            assert!(
                !unit_owned
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(*symbol))
            );
        }
        assert!(
            !unit_owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Temporary(argument.argument()))
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn assert_place_loan(
    checked: &Evidence,
    call_text: &str,
    operand_text: &str,
    root_name: &str,
    field_name: Option<&str>,
    element: bool,
    kind: LoanKind,
) {
    let call = checked.expression(call_text);
    let operand = checked.expression(operand_text);
    let (root_span, field_expressions) = place_source_path(&checked.file, operand);
    let root_reference = checked
        .names
        .references()
        .iter()
        .find(|reference| {
            reference.span() == root_span && reference.namespace() == Namespace::Value
        })
        .expect("actual source binding reference");
    let ReferenceTarget::Symbol(expected_root) = root_reference.target() else {
        panic!("source place root binding");
    };
    let expected_fields = field_expressions
        .iter()
        .map(|expression| {
            checked
                .raw
                .aggregate_projection(*expression)
                .expect("selected source field")
                .field()
        })
        .collect::<Vec<_>>();
    let unit_source = source_unit(&checked.unit.names, checked.file.source_id());
    let unit_root_reference = checked
        .unit
        .names
        .names()
        .references()
        .iter()
        .find(|reference| {
            reference.source_unit() == unit_source
                && reference.span() == root_span
                && reference.namespace() == Some(Namespace::Value)
        })
        .expect("actual source-qualified binding reference");
    let UnitReferenceTarget::Symbol(expected_unit_root) = unit_root_reference.target() else {
        panic!("unit place root binding");
    };
    let expected_unit_fields = field_expressions
        .iter()
        .map(|expression| {
            checked
                .unit
                .raw
                .aggregate_projection(UnitExpressionId::new(unit_source, *expression))
                .expect("selected source-qualified field")
                .field()
        })
        .collect::<Vec<_>>();
    let owned = checked.accepted_single();
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call && loan.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(loans.len(), 1);
    let LoanTarget::Place(place) = loans[0].target() else {
        panic!("real binding/field/element remains Place");
    };
    assert_eq!(place.root(), *expected_root);
    assert_eq!(place.fields(), expected_fields.as_slice());
    assert!(expected_fields.iter().all(|id| {
        checked
            .names
            .symbols()
            .iter()
            .any(|symbol| symbol.id() == *id && symbol.kind() == SymbolKind::Field)
    }));
    let symbol = checked
        .names
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == place.root())
        .unwrap();
    assert_eq!(symbol.name(), root_name);
    assert!(matches!(
        symbol.kind(),
        SymbolKind::Variable | SymbolKind::ValueParameter
    ));
    let fields = place
        .fields()
        .iter()
        .map(|id| {
            checked
                .names
                .symbols()
                .iter()
                .find(|symbol| symbol.id() == *id)
                .unwrap()
                .name()
        })
        .collect::<Vec<_>>();
    assert_eq!(fields, field_name.into_iter().collect::<Vec<_>>());
    assert_eq!(place.element().is_some(), element);
    assert_eq!(loans[0].kind(), kind);
    assert_eq!(loans[0].end_span(), span(&checked.file, call));
    if kind == LoanKind::Exclusive {
        assert_eq!(checked.sources.slice(loans[0].begin_span()), Ok("&"));
    }
    let unit_loan = assert_unit_loan(
        &checked.sources,
        &checked.file,
        &checked.unit,
        checked.unit_expression(call_text),
        checked.unit_expression(operand_text),
        kind,
        false,
    );
    let UnitLoanTarget::Place(place) = unit_loan.target() else {
        panic!("unit real place target");
    };
    assert_eq!(place.root(), *expected_unit_root);
    assert_eq!(place.root().source_unit(), unit_source);
    assert_eq!(place.fields(), expected_unit_fields.as_slice());
    for field in &expected_unit_fields {
        assert_eq!(field.source_unit(), unit_source);
        let resolution =
            checked.unit.names.names().source_units()[field.source_unit().index()].resolution();
        assert!(
            resolution
                .symbols()
                .iter()
                .any(|symbol| symbol.id() == field.symbol() && symbol.kind() == SymbolKind::Field)
        );
    }
    let resolution =
        checked.unit.names.names().source_units()[place.root().source_unit().index()].resolution();
    let root = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == place.root().symbol())
        .unwrap();
    assert_eq!(root.name(), root_name);
    assert!(matches!(
        root.kind(),
        SymbolKind::Variable | SymbolKind::ValueParameter
    ));
    assert_eq!(place.fields().len(), usize::from(field_name.is_some()));
    if let Some(name) = field_name {
        let field = place.fields()[0];
        let resolution =
            checked.unit.names.names().source_units()[field.source_unit().index()].resolution();
        assert_eq!(
            resolution
                .symbols()
                .iter()
                .find(|symbol| symbol.id() == field.symbol())
                .unwrap()
                .name(),
            name
        );
    }
    assert_eq!(place.element().is_some(), element);
}

// Preserve the two existing recovery representations; neither is a selected construction.
pub(super) fn assert_failed_case_raw(checked: &Evidence, member: &str) {
    let id = checked.expression(member);
    let unit_id = checked.unit_expression(member);
    let token = case_token_span(&checked.file, id);
    let reference = checked
        .names
        .references()
        .iter()
        .find(|reference| reference.span() == token && reference.namespace() == Namespace::Value)
        .expect("failed case still has its original value reference");
    let ReferenceTarget::Symbol(symbol) = reference.target() else {
        panic!("real source enum case symbol");
    };
    let case = checked
        .raw
        .enum_cases()
        .iter()
        .find(|case| case.value_symbol() == *symbol)
        .expect("same-analysis case descriptor");
    assert!(case.payloads().is_empty());
    assert_eq!(checked.raw.expression_type(id), Some(case.root_type()));
    assert!(
        matches!(checked.raw.types().get(case.root_type()), Some(TypeKind::Nominal { nominal, arguments }) if *nominal == case.root() && !arguments.is_empty())
    );
    let unit_ty = checked
        .unit
        .raw
        .expression_type(unit_id)
        .expect("real failed Member type");
    assert_eq!(
        checked.unit.raw.types().get(unit_ty),
        Some(&UnitTypeKind::Error)
    );
    assert!(checked.raw.construction(id).is_none());
    assert!(checked.unit.raw.construction(unit_id).is_none());
    for diagnostics in [checked.raw.diagnostics(), checked.unit.raw.diagnostics()] {
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code().to_string(), "L0144");
        assert_eq!(diagnostics[0].primary_span(), token);
    }
}
