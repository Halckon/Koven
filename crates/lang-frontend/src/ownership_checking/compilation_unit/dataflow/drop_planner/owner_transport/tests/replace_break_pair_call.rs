//! Fresh finite ordinary sources; no foreign handles, injection, or execution counters.
use super::*;

#[test]
fn pair_call_seals_actual_selected_owner() {
    for (parameters, arguments) in [
        ("own value: String, own count: Int", "held, 7"),
        ("own count: Int, own value: String", "7, held"),
        (
            "own value: String, own count: Int",
            "count = 7, value = held",
        ),
        (
            "own count: Int, own value: String",
            "value = held, count = 7",
        ),
    ] {
        let text = format!(
            r#"fun sink({parameters}) {{}}
fun run(flag: Boolean) {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; sink({arguments}) }}"#
        );
        let fixture = Fixture::new(&[("pair.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        let boundaries = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        assert_eq!(
            boundaries.len(),
            1,
            "missing pair-call owner receipt: {arguments}"
        );
        let boundary = boundaries[0];
        let consumer = boundary
            .suffix_call_consumer_v1()
            .expect("actual Value handoff");
        assert_eq!(
            consumer.incoming(),
            &[
                boundary.condition_false_exit().clone(),
                boundary.break_boundary().clone()
            ]
        );
        assert!(
            boundary
                .input_producers()
                .definition(consumer.incoming()[0].owner())
                .is_some()
        );
        assert!(
            boundary
                .body_producers()
                .definition(consumer.incoming()[1].owner())
                .is_some()
        );
        let call = fixture.typed.types().call(consumer.call()).unwrap();
        assert_eq!(consumer.arity(), 2);
        let mapped = call
            .arguments()
            .iter()
            .find(|a| a.argument_index() == consumer.argument_index())
            .unwrap();
        assert_eq!(mapped.parameter_index(), consumer.parameter_index());
        let expected_parameter = usize::from(parameters.starts_with("own count"));
        let expected_argument =
            usize::from(arguments.starts_with("7") || arguments.starts_with("count"));
        assert_eq!(
            (consumer.argument_index(), consumer.parameter_index()),
            (expected_argument, expected_parameter)
        );
        assert_eq!(consumer.actions().len(), 2);
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn pair_call_keeps_borrow_and_nonliteral_neighbors_outside_receipt() {
    for (parameters, arguments) in [
        ("own value: String, count: Int", "held, 7"),
        ("own value: String, own count: Int", "held, (7)"),
        ("own value: String, own count: Int", "held, 1 + 6"),
        ("own value: String, own count: Boolean", "held, true"),
        ("own value: String, own a: Int, own b: Int", "held, 7, 8"),
    ] {
        let text = format!(
            r#"fun sink({parameters}) {{}}
fun run(flag: Boolean) {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; sink({arguments}) }}"#
        );
        let fixture = Fixture::new(&[("neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.replace_break_observation_v1().is_none())
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}
