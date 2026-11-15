//! Complete tagged construction/projection/drop positive and negative cases.

use super::*;

#[test]
fn tagged_payload_construction_place_and_recursive_drop_form_one_linear_flow() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("tagged_flow");
    let module = program.module_mut(module_id).expect("module must exist");
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "TaggedResource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let payload_ty = module
        .add_aggregate_type("Event.Some.payload", vec![resource])
        .expect("payload must be valid");
    let tagged = module
        .add_tagged_union_type("Event", vec![payload_ty])
        .expect("tagged union must be valid");
    let (function_id, entry, parameters) =
        add_function(module, "tagged", &[resource], Vec::new(), &origin);
    let resource = parameters[0];
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let payload_value = value(
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: payload_ty,
                fields: vec![resource],
            },
            vec![EntityType::Value(payload_ty)],
            &origin,
        )
        .1[0],
    );
    let tagged_value = value(
        append(
            function,
            entry,
            Operation::TaggedConstruct {
                tagged,
                variant: 0,
                payload: payload_value,
            },
            vec![EntityType::Value(tagged)],
            &origin,
        )
        .1[0],
    );
    append(
        function,
        entry,
        Operation::TaggedPayloadPlace {
            owner: tagged_value,
            variant: 0,
        },
        vec![EntityType::Place(payload_ty)],
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop {
            owner: tagged_value,
        },
        Vec::new(),
        &origin,
    );
    terminate(function, entry, &origin);

    verify_program(&program).expect("valid tagged owner flow must verify");
    let rendered = render_program(&program);
    assert_diagnostic!(rendered.contains("tagged.construct"), "{rendered}");
    assert_diagnostic!(rendered.contains("tagged.payload_place"), "{rendered}");
}

#[test]
fn tagged_operations_reject_out_of_range_case_and_wrong_payload_type() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid_tagged");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let expected_payload = module
        .add_aggregate_type("Expected.payload", vec![integer])
        .expect("payload must be valid");
    let wrong_payload = module
        .add_aggregate_type("Wrong.payload", Vec::new())
        .expect("payload must be valid");
    let tagged = module
        .add_tagged_union_type("Expected", vec![expected_payload])
        .expect("tagged union must be valid");
    let (function_id, entry, parameters) = add_function(
        module,
        "invalid_tagged",
        &[wrong_payload, tagged],
        Vec::new(),
        &origin,
    );
    let payload = parameters[0];
    let tagged_owner = parameters[1];
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let (wrong_construct, _) = append(
        function,
        entry,
        Operation::TaggedConstruct {
            tagged,
            variant: 0,
            payload,
        },
        vec![EntityType::Value(tagged)],
        &origin,
    );
    let (wrong_case, _) = append(
        function,
        entry,
        Operation::TaggedPayloadPlace {
            owner: tagged_owner,
            variant: 1,
        },
        vec![EntityType::Place(expected_payload)],
        &origin,
    );
    terminate(function, entry, &origin);

    let errors = errors(&program);
    for instruction in [wrong_construct, wrong_case] {
        assert!(has_error_at(&errors, instruction, |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}
