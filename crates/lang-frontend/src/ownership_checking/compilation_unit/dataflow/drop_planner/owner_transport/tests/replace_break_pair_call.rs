//! Fresh finite ordinary sources; no foreign handles, injection, or execution counters.
use super::*;

#[test]
fn pair_call_seals_actual_selected_owner() {
    for (parameters, arguments) in [
        ("own value: String, count: Int", "held, 7"),
        ("count: Int, own value: String", "7, held"),
        ("own value: String, count: Int", "count = 7, value = held"),
        ("count: Int, own value: String", "value = held, count = 7"),
    ] {
        let text = format!(r#"fun sink({parameters}) {{}}
fun run(flag: Boolean) {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; sink({arguments}) }}"#);
        let fixture = Fixture::new(&[("pair.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        let boundaries = owned.owner_observations().iter()
            .filter_map(|o| o.replace_break_observation_v1()).collect::<Vec<_>>();
        assert_eq!(boundaries.len(), 1, "missing pair-call owner receipt: {arguments}");
        let boundary = boundaries[0];
        let consumer = boundary.suffix_call_consumer_v1().expect("actual Value handoff");
        assert_eq!(consumer.incoming(), &[boundary.condition_false_exit().clone(), boundary.break_boundary().clone()]);
        assert!(boundary.input_producers().definition(consumer.incoming()[0].owner()).is_some());
        assert!(boundary.body_producers().definition(consumer.incoming()[1].owner()).is_some());
        assert_eq!(consumer.actions().len(), 2);
        compare(&fixture, &fixture.parts(&owned));
    }
}
