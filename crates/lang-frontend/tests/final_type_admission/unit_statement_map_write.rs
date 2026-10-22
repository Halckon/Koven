//! Real source producer controls. No mutated facts, foreign identities, or native execution.
use super::*;
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    parser::{AssignmentOperator, Expression},
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};

// These bytes are copied unchanged from the existing native failure sources.
const ORIGINAL_UNIT: &str =
    include_str!("../fixtures/unit-assignment-admission-0341/unit_array_read_after_if.ko");
const ORIGINAL_MAPS: [&str; 2] = [
    include_str!("../fixtures/unit-assignment-admission-0341/map_copyable_put_bindings.ko"),
    include_str!("../fixtures/unit-assignment-admission-0341/map_put.ko"),
];

fn with_clean_unit(
    text: &str,
    memory: bool,
    inspect: impl FnOnce(&SourceMap, &ParsedFile, &CompilationUnitTypes, SourceUnitId),
) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let input = if memory {
        SourceUnitInput::memory_document(source, &file)
    } else {
        SourceUnitInput::new("root", "main.ko", source, &file)
    };
    let inputs = [input];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = names.names().index().source_units()[0].id();
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    inspect(&sources, &file, &raw, unit);
    let before = raw.clone();
    let complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        .expect("original real source must pass strict Unit admission");
    assert!(std::ptr::eq(complete.types(), &raw));
    assert!(complete.is_same_analysis(&raw));
    assert_eq!(raw, before, "qualification is read-only");
    let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
        .unwrap()
        .into_result()
        .expect("these ordinary sources retain their Basic capability");
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    owned
        .validate()
        .expect("original ownership facts must complete");
}

fn assert_integer_assignments(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    expected: usize,
) {
    let mut found = 0;
    for (expression, node) in file.ast().expressions().iter() {
        let Expression::Assignment {
            target,
            operator,
            value,
            ..
        } = node.payload()
        else {
            continue;
        };
        if *operator == AssignmentOperator::Assign {
            continue;
        }
        found += 1;
        let expression = UnitExpressionId::new(unit, expression);
        let target = UnitExpressionId::new(unit, *target);
        let value = UnitExpressionId::new(unit, *value);
        let descriptor = typed
            .assignment(expression)
            .expect("real compound producer fact");
        assert_eq!(descriptor.expression(), expression);
        assert_eq!(descriptor.target(), target);
        assert_eq!(descriptor.value(), value);
        assert_eq!(descriptor.operator(), *operator);
        assert!(descriptor.falls_through());
        assert_eq!(
            typed.expression_type(target),
            Some(descriptor.target_type())
        );
        assert_eq!(typed.expression_type(value), Some(descriptor.target_type()));
        assert_eq!(
            typed
                .expression_type(expression)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        );
    }
    assert_eq!(found, expected);
}

#[test]
fn original_unit_storage_statement_completes_both_unit_input_modes() {
    for memory in [true, false] {
        with_clean_unit(ORIGINAL_UNIT, memory, |_, file, typed, unit| {
            assert_integer_assignments(file, typed, unit, 1);
        });
    }
}

#[test]
fn integer_statement_operators_publish_storage_typed_facts() {
    for (ty, literal) in [
        ("Byte", "1"),
        ("Short", "1"),
        ("Int", "1"),
        ("Long", "1"),
        ("UByte", "1u"),
        ("UShort", "1u"),
        ("UInt", "1u"),
        ("ULong", "1u"),
    ] {
        let text = format!(
            "fun entry(): Unit {{ var number: {ty} = {literal}; \
             number += {literal}; number -= {literal}; number *= {literal}; \
             number /= {literal}; number %= {literal} }}"
        );
        with_clean_unit(&text, true, |_, file, typed, unit| {
            assert_integer_assignments(file, typed, unit, 5);
        });
    }
}

#[test]
fn statement_roles_flow_through_real_group_and_control_owners() {
    let text = "fun entry(flag: Boolean): Unit { var number = 1; \
        (number += 1); if (flag) { number -= 1 } else { number *= 1 }; \
        when (flag) { true -> number /= 1; false -> number %= 1 } }";
    with_clean_unit(text, true, |_, file, typed, unit| {
        assert_integer_assignments(file, typed, unit, 5);
    });
}

fn assert_rhs_rejection(text: &str, rhs: &str, ordinary_assignment: bool) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    if ordinary_assignment {
        assert!(raw.assignments().is_empty());
    }
    let [diagnostic] = raw.diagnostics() else {
        panic!("{:?}", raw.diagnostics())
    };
    assert_eq!(diagnostic.code().to_string(), "L0084");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok(rhs));
    let Err(FinalTypeAdmissionError::Diagnostics(actual)) =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
    else {
        panic!("the original source mismatch must reject before ownership")
    };
    assert_eq!(actual, raw.diagnostics());
}

#[test]
fn compound_rhs_mismatch_has_no_success_descriptor() {
    for rhs in ["true", "1L"] {
        let text = format!("fun entry(): Unit {{ var number = 1; number += {rhs} }}");
        assert_rhs_rejection(&text, rhs, true);
    }
}

fn assert_map_write_roles(file: &ParsedFile, typed: &CompilationUnitTypes, unit: SourceUnitId) {
    let mut found = 0;
    for (expression, node) in file.ast().expressions().iter() {
        let Expression::Assignment {
            target,
            value,
            operator,
            ..
        } = node.payload()
        else {
            continue;
        };
        let Expression::Index { receiver, index } =
            file.ast().expressions().get(*target).unwrap().payload()
        else {
            continue;
        };
        found += 1;
        assert_eq!(*operator, AssignmentOperator::Assign);
        let target = UnitExpressionId::new(unit, *target);
        let put = typed
            .map_put(target)
            .expect("the original write descriptor");
        assert_eq!(put.expression(), target);
        assert_eq!(put.receiver(), UnitExpressionId::new(unit, *receiver));
        assert_eq!(put.key(), UnitExpressionId::new(unit, *index));
        assert_eq!(put.value(), UnitExpressionId::new(unit, *value));
        assert!(typed.map_get(target).is_none(), "put is not a query");
        assert_eq!(typed.expression_type(target), None, "no invented read type");
        assert_eq!(
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        );
    }
    assert_eq!(found, 1);
}

#[test]
fn original_map_writes_complete_without_inventing_query_facts() {
    for text in ORIGINAL_MAPS {
        for memory in [true, false] {
            with_clean_unit(text, memory, |_, file, typed, unit| {
                assert_map_write_roles(file, typed, unit);
            });
        }
        let mut sources = SourceMap::new();
        let (_, file) = parsed(&mut sources, "main.ko", text);
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let raw = check_types(&sources, &file, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
        let before = raw.clone();
        let complete = complete_file_types(&sources, &file, &names, &types, &raw)
            .expect("the same selected write role closes the raw Single adapter");
        assert!(std::ptr::eq(complete.types(), &raw));
        assert!(complete.is_same_analysis(&before));
        assert!(raw.is_compatible_with_names(&names));
        assert_eq!(raw.source_id(), before.source_id());
        for typed in [&raw, &before] {
            if let Some(constants) = typed.constants() {
                assert!(constants.matches(typed));
            }
        }
        assert_eq!(format!("{raw:?}"), format!("{before:?}"));
    }
}

#[test]
fn map_write_wrong_rhs_keeps_its_original_type_diagnostic() {
    let text = "fun entry(): Unit { var m = mutableMapOf<Int, Int>(); m[1] = true }";
    assert_rhs_rejection(text, "true", false);
}

#[test]
fn bare_field_compound_keeps_its_real_projection_and_deferred_boundary() {
    let text = "class Item(var number: Int) { fun set(): Unit { number += 2 } }";
    for memory in [true, false] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "main.ko", text);
        let inputs = [if memory {
            SourceUnitInput::memory_document(source, &file)
        } else {
            SourceUnitInput::new("root", "main.ko", source, &file)
        }];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let unit = names.names().index().source_units()[0].id();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
        let assignments = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Assignment { .. }))
            .collect::<Vec<_>>();
        let [(id, node)] = assignments.as_slice() else {
            panic!("one original assignment")
        };
        let Expression::Assignment { target, .. } = node.payload() else {
            unreachable!()
        };
        let expression = UnitExpressionId::new(unit, *id);
        let target = UnitExpressionId::new(unit, *target);
        let projection = raw
            .aggregate_projection(target)
            .expect("real bare This field");
        assert_eq!(projection.expression(), target);
        assert!(matches!(
            projection.receiver(),
            lang_frontend::type_checking::UnitAggregateProjectionReceiver::This(_)
        ));
        assert_eq!(
            projection.kind(),
            lang_frontend::type_checking::UnitAggregateProjectionKind::Field
        );
        let field = projection.field();
        let symbol = &names.names().source_units()[field.source_unit().index()]
            .resolution()
            .symbols()[field.symbol().index()];
        assert_eq!(
            symbol.kind(),
            lang_frontend::name_resolution::SymbolKind::Field
        );
        assert_eq!(sources.slice(symbol.span()), Ok("number"));
        assert!(raw.assignment(expression).is_none());
        assert_eq!(
            raw.expression_type(expression)
                .and_then(|ty| raw.types().get(ty)),
            Some(&UnitTypeKind::Deferred(DeferredReason::Assignment))
        );
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        else {
            panic!("bare field remains outside this root statement slice")
        };
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("{diagnostics:?}")
        };
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert_eq!(diagnostic.primary_span(), node.span());
        assert_eq!(sources.slice(diagnostic.primary_span()), Ok("number += 2"));
    }
}
