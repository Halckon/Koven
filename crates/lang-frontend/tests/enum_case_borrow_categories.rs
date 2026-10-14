//! SPEC-0320: enum construction values, genuine places and call-scoped Borrow loans.
//! Tests-only: ordinary source -> original public frontend APIs -> immutable assertions.
use lang_frontend::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SourceUnitId, SourceUnitInput, SymbolKind,
        UnitReferenceTarget, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitConstantOwnership, CompilationUnitOwnership, ConstructionDeliveryKind,
        ConstructionRootKind, DropPoint, DropTarget, LoanEndPoint, LoanKind, LoanTarget,
        OwnershipCheckedFile, OwnershipCheckingError, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitDropTarget, UnitLoanTarget, check_compilation_unit_constant_ownership,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::{Expression, Item, NameMarker, ParsedFile, parse_file},
    source::{SourceId, SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstructionTarget, Copyability, ExpressionCategory,
        FinalTypeAdmissionError, ParameterMode, TypeKind, TypedFile, UnitConstructionTarget,
        UnitExpressionId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[path = "enum_case_borrow_categories/contracts.rs"]
mod contracts;
#[path = "enum_case_borrow_categories/support.rs"]
mod support;
use contracts::*;
use support::*;

#[test]
fn selected_case_values_are_temporary_but_bindings_and_fields_are_places() {
    let grouped = DIRECT.replace("read(Flag.On)", "read(((Flag.On)))");
    // Collect every raw/ownership/final result before asserting a category. A final-gate
    // failure cannot consume the independent category or original ownership evidence.
    let evidence = [SHORT, DIRECT, &grouped, GENERIC, NAMED, PLACES].map(collect);
    let original_probes = [P49, Q08].map(collect);
    let constant = collect(
        "const val NUMBER: Int = 9\nfun readNumber(value: Int): Int = value\nfun constantUse(): Int = readNumber(NUMBER)\n",
    );
    for checked in &evidence {
        checked.clean_raw();
    }
    for (checked, construction, operand, call, parameter, generic) in [
        (&evidence[0], "On", "On", "read(On)", 0, false),
        (
            &evidence[1],
            "Flag.On",
            "Flag.On",
            "read(Flag.On)",
            0,
            false,
        ),
        (
            &evidence[2],
            "Flag.On",
            "((Flag.On))",
            "read(((Flag.On)))",
            0,
            false,
        ),
        (
            &evidence[3],
            "Maybe.None",
            "Maybe.None",
            "read(Maybe.None)",
            0,
            true,
        ),
        (
            &evidence[4],
            "Flag.On",
            "Flag.On",
            "pair(second = Flag.On, first = Flag.Off)",
            1,
            false,
        ),
        (
            &evidence[4],
            "Flag.Off",
            "Flag.Off",
            "pair(second = Flag.On, first = Flag.Off)",
            0,
            false,
        ),
    ] {
        assert_case_descriptor(checked, construction, 0, generic);
        assert_argument_category(
            checked,
            call,
            operand,
            parameter,
            ParameterMode::Borrow,
            ExpressionCategory::Temporary,
        );
    }
    assert_case_descriptor(&evidence[3], "Maybe.Some<Int>(1)", 1, true);
    constant.clean_raw();
    assert_argument_category(
        &constant,
        "readNumber(NUMBER)",
        "NUMBER",
        0,
        ParameterMode::Borrow,
        ExpressionCategory::Temporary,
    );
    assert!(
        constant
            .raw
            .construction(constant.expression("NUMBER"))
            .is_none()
    );
    assert!(
        constant
            .unit
            .raw
            .construction(constant.unit_expression("NUMBER"))
            .is_none()
    );
    for (checked, call_text, operand_text, start, end) in [
        (
            &original_probes[0],
            "name(Color.Green)",
            "Color.Green",
            177,
            188,
        ),
        (&original_probes[1], "show(Msg.Quit)", "Msg.Quit", 187, 195),
    ] {
        // The frozen p49 source repeats Color.Green in its when. Select the actual
        // argument by its outer call AST, never by a name/type-text heuristic.
        let call = checked.expression(call_text);
        let Expression::Call { arguments, .. } = checked
            .file
            .ast()
            .expressions()
            .get(call)
            .unwrap()
            .payload()
        else {
            panic!("probe call");
        };
        assert_eq!(arguments.len(), 1);
        let operand = arguments[0].value;
        assert_eq!(
            checked.sources.slice(span(&checked.file, operand)),
            Ok(operand_text)
        );
        assert_eq!(
            (
                span(&checked.file, operand).start(),
                span(&checked.file, operand).end()
            ),
            (start, end)
        );
        assert_case_descriptor_at(checked, operand, 0, false);
        assert_argument_category_at(
            checked,
            call,
            operand,
            0,
            ParameterMode::Borrow,
            ExpressionCategory::Temporary,
        );
        // ownership/final were collected unchanged. The original when backend gates
        // have independent oracles; this test does not assert whole-probe admission.
    }

    for (call, operand, root, field, element) in [
        ("read(local)", "local", "local", None, false),
        ("read((parameter))", "(parameter)", "parameter", None, false),
        (
            "read(holder.current)",
            "holder.current",
            "holder",
            Some("current"),
            false,
        ),
        ("read(Flag.On)", "Flag.On", "Flag", Some("On"), false),
        ("read(values[0])", "values[0]", "values", None, true),
    ] {
        assert_argument_category(
            &evidence[5],
            call,
            operand,
            0,
            ParameterMode::Borrow,
            ExpressionCategory::Place,
        );
        assert_place_loan(
            &evidence[5],
            call,
            operand,
            root,
            field,
            element,
            LoanKind::Shared,
        );
        assert!(
            evidence[5]
                .raw
                .construction(evidence[5].expression(operand))
                .is_none()
        );
        assert!(
            evidence[5]
                .unit
                .raw
                .construction(evidence[5].unit_expression(operand))
                .is_none()
        );
    }
}

#[test]
fn selected_case_qualifiers_keep_final_admission_precise() {
    let grouped = DIRECT.replace("read(Flag.On)", "read((Flag.On))");
    let positive = [SHORT, DIRECT, &grouped, GENERIC, PLACES].map(collect);
    let negative = [
        ("enum class Flag { On, Off }\nfun invalid(Flag: String): Int = Flag.On\n", "Flag.On", "On", "L0080"),
        ("enum class Flag { On, Off }\nvalue class Holder(val Flag: String)\nfun invalid(holder: Holder): Int = holder.Flag.On\n", "holder.Flag.On", "On", "L0080"),
        ("enum class Maybe<T> { Some(item: T), None }\nfun invalid(): Unit { val missing = Maybe.None }\n", "Maybe.None", "None", "L0144"),
    ].map(|(text, operand, token, code)| (collect(text), operand, token, code));
    let mixed = collect(MIXED_FAILURES);
    for (checked, operand, _, _) in &negative {
        log_unselected_member(checked, operand);
    }
    for member in ["Maybe.None", "n.fooBar", "(Maybe).None"] {
        log_unselected_member(&mixed, member);
    }
    assert_failed_case_raw(&negative[2].0, "Maybe.None");
    assert_failed_case_raw(&mixed, "Maybe.None");
    // Rejected runtime receivers and unfinished constructions must not acquire static
    // qualifier privileges. These controls are examined independently of positive final.
    for (checked, operand, token, code) in &negative {
        let Expression::Member { name_span, .. } = checked
            .file
            .ast()
            .expressions()
            .get(checked.expression(operand))
            .unwrap()
            .payload()
        else {
            panic!("negative source Member");
        };
        let expected_primary = *name_span;
        assert!(
            checked
                .raw
                .construction(checked.expression(operand))
                .is_none()
        );
        assert!(
            checked
                .unit
                .raw
                .construction(checked.unit_expression(operand))
                .is_none()
        );
        for result in [&checked.final_result, &checked.unit.final_result] {
            let diagnostics = source_diagnostics(result);
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), *code);
            assert_eq!(
                diagnostics[0].primary_span(),
                expected_primary,
                "reject the actual runtime member or unfinished case token"
            );
            assert_eq!(
                checked.sources.slice(diagnostics[0].primary_span()),
                Ok(*token)
            );
            assert_eq!(
                diagnostics[0].primary_span().source_id(),
                checked.file.source_id()
            );
        }
    }
    let expected = [
        (
            "L0144".to_owned(),
            case_token_span(&mixed.file, mixed.expression("Maybe.None")),
        ),
        (
            "L0080".to_owned(),
            case_token_span(&mixed.file, mixed.expression("n.fooBar")),
        ),
        (
            "L0080".to_owned(),
            case_token_span(&mixed.file, mixed.expression("(Maybe).None")),
        ),
    ];
    for result in [&mixed.final_result, &mixed.unit.final_result] {
        let actual = source_diagnostics(result)
            .iter()
            .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.primary_span()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "the failed case cannot suppress independent runtime roots"
        );
    }
    for (checked, construction) in [
        (&positive[1], "Flag.On"),
        (&positive[2], "Flag.On"),
        (&positive[3], "Maybe.None"),
    ] {
        let id = checked.expression(construction);
        let Expression::Member { receiver, .. } =
            checked.file.ast().expressions().get(id).unwrap().payload()
        else {
            panic!("qualified construction");
        };
        let qualifier_span = span(&checked.file, *receiver);
        let ConstructionTarget::EnumCase(case_id) = checked.raw.construction(id).unwrap().target()
        else {
            panic!("selected enum case");
        };
        let case = checked
            .raw
            .enum_cases()
            .iter()
            .find(|case| case.id() == case_id)
            .unwrap();
        assert!(
            checked
                .names
                .references()
                .iter()
                .any(|reference| reference.span() == qualifier_span
                    && reference.namespace() == Namespace::Type
                    && reference.target() == &ReferenceTarget::Symbol(case.root().symbol()))
        );
        assert!(
            checked
                .unit
                .names
                .names()
                .references()
                .iter()
                .any(|reference| reference.span() == qualifier_span
                    && reference.namespace() == Some(Namespace::Type)
                    && matches!(reference.target(), UnitReferenceTarget::Declaration(_)))
        );
    }
    for checked in &positive {
        checked.clean_raw();
        assert!(
            checked.final_result.is_ok(),
            "single final: {:?}",
            checked.final_result
        );
        assert!(
            checked.unit.final_result.is_ok(),
            "unit final: {:?}",
            checked.unit.final_result
        );
    }
}

#[test]
fn borrowed_case_values_publish_call_scoped_temporary_loans() {
    let grouped = DIRECT.replace("read(Flag.On)", "read(((Flag.On)))");
    let move_short = "enum class Msg { Text(text: String), Quit; fun fromShort(): Int = read(Quit) }\nfun read(value: Msg): Int = 7\n";
    let evidence = [
        SHORT, move_short, DIRECT, &grouped, MOVE_ONLY, GENERIC, PAYLOADS, MOVE_GROUP,
    ]
    .map(collect);
    // The short-name case has its own source analysis. In particular, a correct raw
    // Temporary category must not hide ownership choosing an EnumVariant as a Place.
    for (checked, construction, operand, call, move_only, payload, generic) in [
        (&evidence[0], "On", "On", "read(On)", false, None, false),
        (
            &evidence[7],
            "Msg.Quit",
            "((Msg.Quit))",
            "read(((Msg.Quit)))",
            true,
            None,
            false,
        ),
        (
            &evidence[1],
            "Quit",
            "Quit",
            "read(Quit)",
            true,
            None,
            false,
        ),
        (
            &evidence[2],
            "Flag.On",
            "Flag.On",
            "read(Flag.On)",
            false,
            None,
            false,
        ),
        (
            &evidence[3],
            "Flag.On",
            "((Flag.On))",
            "read(((Flag.On)))",
            false,
            None,
            false,
        ),
        (
            &evidence[4],
            "Msg.Quit",
            "Msg.Quit",
            "read(Msg.Quit)",
            true,
            None,
            false,
        ),
        (
            &evidence[4],
            "Msg.Text(\"payload\")",
            "Msg.Text(\"payload\")",
            "read(Msg.Text(\"payload\"))",
            true,
            Some(ConstructionDeliveryKind::DeliverTemporary),
            false,
        ),
        (
            &evidence[5],
            "Maybe.None",
            "Maybe.None",
            "read(Maybe.None)",
            false,
            None,
            true,
        ),
        (
            &evidence[5],
            "Maybe.Some<Int>(1)",
            "Maybe.Some<Int>(1)",
            "read(Maybe.Some<Int>(1))",
            false,
            Some(ConstructionDeliveryKind::DeliverTemporary),
            true,
        ),
        (
            &evidence[6],
            "Msg.Text(text)",
            "Msg.Text(text)",
            "readMsg(Msg.Text(text))",
            true,
            Some(ConstructionDeliveryKind::Move),
            false,
        ),
        (
            &evidence[6],
            "Count.Value(number)",
            "Count.Value(number)",
            "readCount(Count.Value(number))",
            false,
            Some(ConstructionDeliveryKind::Copy),
            false,
        ),
    ] {
        checked.clean_raw();
        assert_temporary_loan(
            checked,
            call,
            operand,
            construction,
            move_only,
            payload,
            generic,
        );
    }
}

#[test]
fn cross_file_case_aliases_preserve_source_qualified_identity() {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "p/api.ko",
        "package p\nenum class Flag { On, Off }\nfun read(value: Flag): Int = 7\n",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "q/use.ko",
        "package q\nimport p.Flag as Choice\nimport p.read\nfun qualified(): Int = read(p.Flag.On)\nfun aliasSame(): Int = read(Choice.On)\nfun aliased(): Int = read(Choice.Off)\n",
    );
    let (constant_id, constant) = parsed(
        &mut sources,
        "q/constants.ko",
        "package q\nconst val NUMBER: Int = 9\nfun readNumber(value: Int): Int = value\nfun constantUse(): Int = readNumber(NUMBER)\n",
    );
    let base = [
        SourceUnitInput::new("root", "p/api.ko", provider_id, &provider),
        SourceUnitInput::new("root", "q/use.ko", consumer_id, &consumer),
    ];
    let with_constants = [
        base[0],
        base[1],
        SourceUnitInput::new("root", "q/constants.ko", constant_id, &constant),
    ];
    let evidence = [
        collect_unit(&sources, &base, false),
        collect_unit(&sources, &[base[1], base[0]], false),
        collect_unit(&sources, &with_constants, true),
        collect_unit(&sources, &[with_constants[2], base[1], base[0]], true),
    ];
    let mut identities = Vec::new();
    for (index, checked) in evidence.iter().enumerate() {
        assert!(
            checked.raw.diagnostics().is_empty(),
            "{:?}",
            checked.raw.diagnostics()
        );
        assert_eq!(checked.raw.clone().validate().is_ok(), index < 2);
        assert!(checked.raw.clone().validate_constants().is_ok());
        let provider_unit = source_unit(&checked.names, provider_id);
        let consumer_unit = source_unit(&checked.names, consumer_id);
        let mut targets = Vec::new();
        for (operand, call, case_name) in [
            ("p.Flag.On", "read(p.Flag.On)", "On"),
            ("Choice.On", "read(Choice.On)", "On"),
            ("Choice.Off", "read(Choice.Off)", "Off"),
        ] {
            let value =
                UnitExpressionId::new(consumer_unit, expression(&sources, &consumer, operand));
            let call = UnitExpressionId::new(consumer_unit, expression(&sources, &consumer, call));
            let descriptor = checked
                .raw
                .construction(value)
                .expect("selected cross-file case");
            let UnitConstructionTarget::EnumCase(target) = descriptor.target() else {
                panic!("enum target");
            };
            assert_eq!(target.source_unit(), provider_unit);
            let case = checked
                .raw
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| declaration.nominal())
                .flat_map(|nominal| nominal.enum_cases())
                .find(|case| case.value_symbol() == target)
                .expect("provider case signature");
            assert_eq!(case.name(), case_name);
            assert_eq!(case.name_span().source_id(), provider_id);
            assert!(descriptor.arguments().is_empty());
            assert_eq!(descriptor.result_type(), case.value_type());
            assert_unit_case_descriptor(checked, &consumer, value, 0, false);
            let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = checked.raw.types().get(descriptor.result_type())
            else {
                panic!("full provider enum root");
            };
            assert!(arguments.is_empty());
            let Expression::Member { receiver, .. } = consumer
                .ast()
                .expressions()
                .get(value.expression())
                .unwrap()
                .payload()
            else {
                panic!("qualified source case");
            };
            let qualifier = consumer.ast().expressions().get(*receiver).unwrap();
            let type_span = match qualifier.payload() {
                Expression::Name => qualifier.span(),
                Expression::Member {
                    receiver,
                    name_span,
                    ..
                } => {
                    let package_span = span(&consumer, *receiver);
                    assert!(checked.names.names().references().iter().any(|reference| {
                        reference.source_unit() == consumer_unit
                            && reference.span() == package_span
                            && matches!(reference.target(), UnitReferenceTarget::Package(_))
                    }));
                    *name_span
                }
                _ => panic!("static type qualifier AST"),
            };
            assert!(checked.names.names().references().iter().any(|reference| {
                reference.source_unit() == consumer_unit
                    && reference.span() == type_span
                    && reference.namespace() == Some(Namespace::Type)
                    && reference.target() == &UnitReferenceTarget::Declaration(*declaration)
            }));
            assert_eq!(
                checked.raw.expression_category(value),
                Some(ExpressionCategory::Temporary)
            );
            let argument = checked.raw.call(call).expect("outer read call").arguments()[0];
            assert_eq!(
                (
                    argument.argument_index(),
                    argument.parameter_index(),
                    argument.mode(),
                    argument.category()
                ),
                (0, 0, ParameterMode::Borrow, ExpressionCategory::Temporary)
            );
            assert_eq!(argument.parameter_type(), descriptor.result_type());
            assert_unit_loan(
                &sources,
                &consumer,
                checked,
                call,
                value,
                LoanKind::Shared,
                true,
            );
            targets.push((value, target));
        }
        assert_eq!(
            targets[0].1, targets[1].1,
            "qualified and aliased On share provider case identity"
        );
        assert_ne!(
            targets[0].0, targets[1].0,
            "two constructions remain different source expressions"
        );
        assert_ne!(
            targets[0].1, targets[2].1,
            "same enum type must not merge two case targets"
        );
        identities.push(targets);
        if index >= 2 {
            let constant_unit = source_unit(&checked.names, constant_id);
            let value =
                UnitExpressionId::new(constant_unit, expression(&sources, &constant, "NUMBER"));
            assert_eq!(
                checked.raw.expression_category(value),
                Some(ExpressionCategory::Temporary)
            );
            assert!(checked.raw.construction(value).is_none());
            let UnitOwnershipResult::Constants(Ok(owned)) = &checked.owned else {
                panic!("constant-specific original result: {:?}", checked.owned);
            };
            assert!(
                owned
                    .materializations()
                    .expect("constant plans")
                    .iter()
                    .any(|plan| plan.descriptor().expression() == value)
            );
            assert!(
                owned.ownership().clone().validate().is_err(),
                "constant product cannot become base capability"
            );
        }
        assert!(
            checked.final_result.is_ok(),
            "cross-file final: {:?}",
            checked.final_result
        );
    }
    assert_eq!(identities[0], identities[1]);
    assert_eq!(identities[2], identities[3]);
}

#[test]
fn inout_case_temporaries_keep_l0122_and_mutable_places_stay_valid() {
    let grouped = INOUT.replace("&Flag.On", "&((Flag.On))");
    let invalid = [INOUT, &grouped].map(collect);
    let valid = collect(MUTABLE);
    for checked in &invalid {
        let parameter = parameter_span(&checked.sources, &checked.file, "touch");
        assert_eq!(checked.sources.slice(parameter), Ok("inout value: Flag"));
        assert_eq!((parameter.start(), parameter.end()), (38, 55));
        let single_parameter = checked
            .names
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::ValueParameter && symbol.name() == "value")
            .expect("selected single parameter symbol")
            .span();
        assert_eq!((single_parameter.start(), single_parameter.end()), (44, 49));
        assert_eq!(checked.sources.slice(single_parameter), Ok("value"));
        let marker = checked
            .sources
            .span(checked.file.source_id(), 91, 92)
            .unwrap();
        assert_eq!(checked.sources.slice(marker), Ok("&"));
        for (diagnostics, parameter_label) in [
            (checked.raw.diagnostics(), single_parameter),
            (checked.unit.raw.diagnostics(), parameter),
        ] {
            assert_eq!(
                diagnostics.len(),
                1,
                "type-stage diagnostics: {diagnostics:?}"
            );
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.code().to_string(), "L0122");
            assert_eq!(diagnostic.primary_span(), marker);
            let labels = diagnostic
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(label.span()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                labels.contains(&parameter_label),
                "actual selected parameter label: {labels:?}"
            );
        }
        for result in [&checked.final_result, &checked.unit.final_result] {
            let diagnostics = source_diagnostics(result);
            assert_eq!(diagnostics.len(), 1, "no final cascade: {diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), "L0122");
            assert_eq!(diagnostics[0].primary_span(), marker);
        }
    }
    valid.clean_raw();
    assert_argument_category(
        &valid,
        "touch(&value)",
        "value",
        0,
        ParameterMode::Inout,
        ExpressionCategory::Place,
    );
    assert_place_loan(
        &valid,
        "touch(&value)",
        "value",
        "value",
        None,
        false,
        LoanKind::Exclusive,
    );
    assert!(valid.final_result.is_ok(), "{:?}", valid.final_result);
    assert!(
        valid.unit.final_result.is_ok(),
        "{:?}",
        valid.unit.final_result
    );
}
