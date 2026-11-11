//! A live header is a source equation, not the zero-iteration P projection.
use super::*;
use crate::ownership_checking::UnitOwnerCallable;
use crate::type_checking::UnitItemId;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) { val previous = replace(&held, "next"); again = false }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn actual_normal_backedge_requires_a_header_source_product() {
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let callable = UnitOwnerCallable::Function(UnitItemId::new(
        fixture.source(0),
        fixture.parsed[0].roots()[0],
    ));
    let observation = owned
        .owner_observations()
        .iter()
        .find(|value| value.callable() == callable)
        .unwrap();
    assert!(observation.replace_break_observation_v1().is_none());
    assert!(observation.loop_observation_v1().is_none());
    assert!(observation.invariant_loop_observation_v1().is_none());
    assert!(
        observation.open_loop_observation_v1().is_some(),
        "normal backedge requires its actual header-source product, not fixed P/Q exits"
    );
}

#[test]
fn normal_backedge_keeps_two_real_catalogs_and_header_source_equation() {
    use crate::ownership_checking::{
        UnitOwnerHeaderSourceV1 as H, UnitOwnerOpenLoopObservationV1 as O, UnitOwnerOrigin,
    };
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let products = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.open_loop_observation_v1())
        .collect::<Vec<_>>();
    let [O::NormalBackedge(product)] = products.as_slice() else {
        panic!("one changing header product")
    };
    assert!(product.is_consistent());
    let [H::Entry(p), H::Backedge(q)] = product.incoming() else {
        panic!("ordered entry/backedge equation")
    };
    assert_ne!(p, q);
    assert_eq!(product.input_producers().definitions().len(), 1);
    assert_eq!(product.body_producers().definitions().len(), 1);
    assert_eq!(
        product.input_producers().definition(p).unwrap().origin(),
        UnitOwnerOrigin::String(product.initial())
    );
    assert_eq!(
        product.body_producers().definition(q).unwrap().origin(),
        UnitOwnerOrigin::String(product.argument())
    );
    assert_eq!(product.prefix_events().len(), 2);
    assert_eq!(product.body_events().len(), 6);
    assert_ne!(product.return_expression(), product.return_payload());
}

#[test]
fn normal_backedge_declines_unproved_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        SOURCE.replace("again = false", "again = flag"),
        SOURCE.replace("while(again)", "while((again))"),
        SOURCE.replace("again = false", "again = false; continue"),
        SOURCE.replace("return held", "return (held)"),
    ] {
        let fixture = Fixture::new(&[("neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| !matches!(o.open_loop_observation_v1(), Some(O::NormalBackedge(_))))
        );
    }
}

#[test]
fn ci_only_normal_backedge_rejects_damaged_completion() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let fixture = Fixture::new(&[("normal-backedge.ko", SOURCE)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(v)) => Some(v),
            _ => None,
        })
        .unwrap();
    for damage in 0..5 {
        let mut product = (**original).clone();
        match damage {
            0 => product.core.assignment_completed = false,
            1 => product.core.body_completed = false,
            2 => product.core.cleaned = false,
            3 => product.core.increment = Some(product.core.assignment),
            _ => {
                product.core.events.pop();
            }
        }
        assert!(!product.is_consistent());
    }
}

#[test]
fn counter_control_seals_only_exact_normal_steps() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = r#"fun run(limit: Int): String {
        var held = "outer"
        var index = 0
        while(index < limit) { val previous = replace(&held, "next"); index = index + 1 }
        return held
    }"#;
    let texts = [
        source.to_owned(),
        source.replace("index = 0", "index = 1"),
        source.replace("index + 1", "index + 2"),
        source.replace("index < limit", "index <= limit"),
        source.replace("index + 1", "index - 1"),
        source.replace("index + 1", "(index + 1)"),
        source.replace("index < limit", "(index < limit)"),
        source.replace("index = index + 1", "index = index + 1; continue"),
    ];
    for (index, text) in texts.iter().enumerate() {
        let fixture = Fixture::new(&[("counter.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| match o.open_loop_observation_v1() {
                Some(O::NormalBackedge(v)) => Some(v),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(products.len(), usize::from(index == 0), "neighbor {index}");
        if let Some(product) = products.first() {
            assert!(product.is_consistent());
        }
    }
}

#[test]
fn continue_header_is_a_distinct_completed_jump() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        SOURCE.replace("again = false }", "again = false; continue }"),
        SOURCE
            .replace("flag: Boolean", "flag: Int")
            .replace("var again = flag", "var again = 0")
            .replace("while(again)", "while(again < flag)")
            .replace("again = false }", "again = again + 1; continue }")
            .replace("run(false)", "run(0)")
            .replace("run(true)", "run(7)"),
    ] {
        let fixture = Fixture::new(&[("continue.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::ContinueBackedge(product)] = products.as_slice() else {
            panic!("distinct Continue product");
        };
        assert!(product.is_consistent());
        assert_ne!(product.jump(), product.return_expression());
        assert_eq!(product.body_events().len(), 6);
        assert_eq!(product.prefix_events().len(), 2);
    }
}

#[test]
fn ci_only_continue_header_rejects_role_target_and_cleanup_damage() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    use crate::ownership_checking::compilation_unit::owner_transport::HeaderCompletion;
    let text = SOURCE.replace("again = false }", "again = false; continue }");
    let fixture = Fixture::new(&[("continue-damage.ko", &text)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::ContinueBackedge(v)) => Some(v),
            _ => None,
        })
        .unwrap();
    for damage in 0..7 {
        let mut p = (**original).clone();
        match damage {
            0 => p.stopped = false,
            1 => p.core.completion = HeaderCompletion::Fallthrough,
            2 => p.target = p.core.body,
            3 => p.identity = std::sync::Arc::new(()),
            4 => p.snapshot = p.core.entry.clone(),
            5 => p.jump = p.core.assignment,
            _ => p.core.cleaned = false,
        }
        assert!(!p.is_consistent());
    }
}

const MIXED_SOURCE: &str = r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < 2) { continue } else { break }
    }
    return held
}"#;

#[test]
fn mixed_header_seals_distinct_continue_and_break_arms() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let fixture = Fixture::new(&[("mixed.ko", MIXED_SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let products = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.open_loop_observation_v1())
        .collect::<Vec<_>>();
    let [O::MixedBackedge(product)] = products.as_slice() else {
        panic!("independent mixed product");
    };
    assert!(product.is_consistent());
    assert_ne!(product.continue_jump(), product.break_jump());
    assert_ne!(product.continue_body(), product.break_body());
    assert_ne!(product.split_condition(), product.condition());
    assert_eq!(product.body_events().len(), 6);
    assert_eq!(product.prefix_events().len(), 2);
    assert_eq!(product.body_producers().definitions().len(), 1);
    assert_eq!(product.input_producers().definitions().len(), 1);
}

#[test]
fn mixed_header_declines_unproved_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        MIXED_SOURCE.replace("index < 2", "index < 3"),
        MIXED_SOURCE.replace("index < 2", "index <= 2"),
        MIXED_SOURCE.replace("index < 2", "(index < 2)"),
        MIXED_SOURCE.replace("{ continue } else { break }", "{ break } else { continue }"),
        MIXED_SOURCE.replace(
            "{ continue } else { break }",
            "{ continue } else { continue }",
        ),
        MIXED_SOURCE.replace("index + 1", "index + 2"),
    ] {
        let fixture = Fixture::new(&[("mixed-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(
            !owned
                .owner_observations()
                .iter()
                .any(|o| matches!(o.open_loop_observation_v1(), Some(O::MixedBackedge(_))))
        );
    }
}

#[test]
fn ci_only_mixed_header_rejects_role_identity_and_arm_damage() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    use crate::ownership_checking::compilation_unit::owner_transport::HeaderCompletion;
    let fixture = Fixture::new(&[("mixed-damage.ko", MIXED_SOURCE)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::MixedBackedge(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..11 {
        let mut p = (**original).clone();
        match damage {
            0 => p.stopped = false,
            1 => p.core.completion = HeaderCompletion::Continue(p.jumps[0]),
            2 => p.jumps[1] = p.jumps[0],
            3 => p.bodies[1] = p.bodies[0],
            4 => p.identity = std::sync::Arc::new(()),
            5 => p.transferred[1] = p.core.entry.clone(),
            6 => p.completed[0] = p.core.entry.clone(),
            7 => p.split_condition = p.core.condition,
            8 => p.jumps.swap(0, 1),
            9 => p.roles.swap(0, 1),
            _ => p.core.cleaned = false,
        }
        assert!(!p.is_consistent());
    }
}

fn parameter_mixed_source() -> String {
    MIXED_SOURCE
        .replace("limit: Int", "limit: Int, stop: Int")
        .replace("index < 2", "index < stop")
}
#[test]
fn parameter_mixed_header_binds_current_callable_thresholds() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = parameter_mixed_source();
    for text in [
        source.clone(),
        source.replace("limit: Int, stop: Int", "stop: Int, limit: Int"),
        source
            .replace("limit", "maximum")
            .replace("stop", "threshold"),
    ] {
        let fixture = Fixture::new(&[("parameter-mixed.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::MixedBackedge(p)] = products.as_slice() else {
            panic!("exact parameter mixed product");
        };
        assert!(p.is_consistent());
        let t = p.threshold.as_ref().unwrap();
        assert_eq!(p.parameter_count, 2);
        assert_eq!(t.callable, p.callable());
        assert_ne!(t.symbols[0], t.symbols[1]);
        assert_ne!(t.operands[0], t.operands[1]);
    }
}
#[test]
fn parameter_mixed_header_declines_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = parameter_mixed_source();
    for text in [
        source.replace("index < stop", "index < limit"),
        source.replace("index < stop", "index < (stop)"),
        source.replace("index < stop", "index < stop + 0"),
        source.replace("index < stop", "index <= stop"),
        source.replace("index < stop", "index < 2"),
        source.replace("stop: Int", "stop: Int, unused: Int"),
        source.replace("if(index < stop) { continue } else { break }", "continue"),
        source.replace("if(index < stop) { continue } else { break }", ""),
        source
            .replace("stop: Int", "stop: Boolean")
            .replace("index < stop", "stop"),
        source.replace("index < limit", "index < stop"),
        source.replace("index + 1", "index + 2"),
    ] {
        let fixture = Fixture::new(&[("parameter-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(!owned.owner_observations().iter().any(|o| matches!(
            o.open_loop_observation_v1(),
            Some(O::MixedBackedge(_) | O::NormalBackedge(_) | O::ContinueBackedge(_))
        )));
    }
}
#[test]
fn ci_only_parameter_mixed_rejects_threshold_identity_damage() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = parameter_mixed_source();
    let fixture = Fixture::new(&[("parameter-damage.ko", &source)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::MixedBackedge(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..9 {
        let mut p = (**original).clone();
        if damage == 0 {
            p.threshold = None;
        } else if damage == 1 {
            p.parameter_count = 1;
        } else {
            let t = p.threshold.as_mut().unwrap();
            match damage {
                2 => t.symbols[1] = t.symbols[0],
                3 => t.parameters[1] = t.parameters[0],
                4 => t.operands[1] = t.operands[0],
                5 => t.condition = p.split_condition,
                6 => t.callable = crate::ownership_checking::UnitOwnerCallable::Lambda(p.split),
                _ => t.operands[1] = p.core.assignment,
            }
        }
        assert!(!p.is_consistent());
    }
}

fn conditional_break_source() -> String {
    parameter_mixed_source().replace("{ continue } else { break }", "{ break }")
}
#[test]
fn conditional_break_header_seals_real_implicit_continuation() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = conditional_break_source();
    for text in [
        source.clone(),
        source.replace("limit: Int, stop: Int", "stop: Int, limit: Int"),
        source.replace("stop", "threshold"),
    ] {
        let fixture = Fixture::new(&[("conditional-break.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::ConditionalBreakBackedge(p)] = products.as_slice() else {
            panic!("independent implicit false product");
        };
        assert!(p.is_consistent());
        assert_eq!(p.snapshots.len(), 8);
        assert_ne!(p.split_condition(), p.condition());
        assert_ne!(p.break_body(), p.body());
        assert_eq!(p.body_events().len(), 6);
        assert_eq!(p.body_producers().definitions().len(), 1);
    }
}
#[test]
fn conditional_break_header_declines_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = conditional_break_source();
    for text in [
        source.replace("index < stop", "index < limit"),
        source.replace("index < stop", "index <= stop"),
        source.replace("index < stop", "index < (stop)"),
        source.replace("index < stop", "index < 2"),
        source.replace("{ break }", "{ continue }"),
        source.replace("{ break }", "{ break } else {}"),
        source.replace("{ break }", "{} else { break }"),
        source.replace("index + 1", "index + 2"),
        source.replace("stop: Int", "stop: Int, extra: Int"),
    ] {
        let fixture = Fixture::new(&[("conditional-break-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(!owned.owner_observations().iter().any(|o| matches!(
            o.open_loop_observation_v1(),
            Some(O::ConditionalBreakBackedge(_))
        )));
    }
}
#[test]
fn ci_only_conditional_break_rejects_false_completion_and_role_damage() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    use crate::ownership_checking::compilation_unit::owner_transport::{
        HeaderCompletion, LoopBodyCompletion,
    };
    let source = conditional_break_source();
    let fixture = Fixture::new(&[("conditional-break-damage.ko", &source)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::ConditionalBreakBackedge(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..11 {
        let mut p = (**original).clone();
        match damage {
            0 => p.split_sealed = false,
            1 => {
                p.snapshots.pop();
            }
            2 => p.snapshots[5] = p.core.entry.clone(),
            3 => p.snapshots[6] = p.core.entry.clone(),
            4 => p.snapshots[7] = p.core.entry.clone(),
            5 => p.jump_role = LoopBodyCompletion::Continue(p.break_jump),
            6 => p.core.completion = HeaderCompletion::Mixed(p.split),
            7 => p.identity = std::sync::Arc::new(()),
            8 => p.break_body = p.core.body,
            9 => p.threshold.symbols[1] = p.threshold.symbols[0],
            _ => p.core.cleaned = false,
        }
        assert!(!p.is_consistent());
    }
}

fn conditional_return_source() -> String {
    conditional_break_source().replace("{ break }", "{ return held }")
}
#[test]
fn conditional_return_header_seals_distinct_delivery_and_normal_edge() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = conditional_return_source();
    for text in [
        source.clone(),
        source.replace("limit: Int, stop: Int", "stop: Int, limit: Int"),
        source.replace("held", "owned"),
    ] {
        let fixture = Fixture::new(&[("conditional-return.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::ConditionalReturnBackedge(p)] = products.as_slice() else {
            panic!("independent conditional Return product");
        };
        assert!(p.is_consistent());
        assert!(p.body_return().is_consistent());
        assert_eq!(p.snapshots.len(), 6);
        assert_eq!(p.body_return().root(), p.root());
        assert_eq!(p.body_return().actions().len(), 2);
        assert_ne!(p.body_return_expression(), p.return_expression());
        assert_ne!(p.body_return_payload(), p.return_payload());
        assert_eq!(p.body_events().len(), 6);
        assert_eq!(p.true_events.len(), 2);
        assert_eq!(p.body_producers().definitions().len(), 1);
    }
}
#[test]
fn conditional_return_header_declines_unproved_legal_neighbors() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    let source = conditional_return_source();
    for text in [
        source.replace("index < stop", "index < limit"),
        source.replace("index < stop", "index <= stop"),
        source.replace("index < stop", "index < (stop)"),
        source.replace("index < stop", "index < 2"),
        source.replace("{ return held }", "{ return (held) }"),
        source.replace("{ return held }", "{ return held } else {}"),
        source.replace("{ return held }", "{ break }"),
        source.replace("index + 1", "index + 2"),
        source.replace("stop: Int", "stop: Int, extra: Int"),
    ] {
        let fixture = Fixture::new(&[("conditional-return-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(!owned.owner_observations().iter().any(|o| matches!(
            o.open_loop_observation_v1(),
            Some(O::ConditionalReturnBackedge(_))
        )));
    }
}
#[test]
fn ci_only_conditional_return_rejects_delivery_and_false_edge_damage() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    use crate::ownership_checking::compilation_unit::owner_transport::HeaderCompletion;
    let source = conditional_return_source();
    let fixture = Fixture::new(&[("conditional-return-damage.ko", &source)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::ConditionalReturnBackedge(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..13 {
        let mut p = (**original).clone();
        match damage {
            0 => p.arm_stopped = false,
            1 => p.body_return.cleaned = false,
            2 => {
                p.body_return.actions.pop();
            }
            3 => p.body_return.taken = p.core.entry.clone(),
            4 => p.body_return.target = p.core.target.clone(),
            5 => p.true_events.clear(),
            6 => p.snapshots[4] = p.core.entry.clone(),
            7 => {
                p.snapshots.pop();
            }
            8 => p.core.completion = HeaderCompletion::ConditionalBreak(p.split),
            9 => p.identity = std::sync::Arc::new(()),
            10 => p.threshold.symbols[1] = p.threshold.symbols[0],
            11 => p.true_events.swap(0, 1),
            _ => p.core.cleaned = false,
        }
        assert!(!p.is_consistent());
    }
}

const PARAMETER_HEADER_SOURCE: &str = r#"fun run(limit: Int, own incoming: String): String {
    var held = incoming
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
    }
    return held
}"#;
#[test]
fn parameter_normal_header_preserves_actual_entry_producer() {
    use crate::ownership_checking::{
        UnitOwnerOpenLoopObservationV1 as O, UnitOwnerOrigin, UnitOwnerTransportEvent as E,
    };
    for text in [
        PARAMETER_HEADER_SOURCE.to_string(),
        PARAMETER_HEADER_SOURCE.replace(
            "limit: Int, own incoming: String",
            "own incoming: String, limit: Int",
        ),
        PARAMETER_HEADER_SOURCE.replace("incoming", "provided"),
    ] {
        let fixture = Fixture::new(&[("parameter-header.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::NormalBackedge(p)] = products.as_slice() else {
            panic!("parameter normal header");
        };
        let parameter = p.input_parameter().unwrap();
        assert!(p.is_consistent());
        assert_eq!(p.initial_origin(), UnitOwnerOrigin::Parameter(parameter));
        assert_ne!(parameter, p.root());
        assert_eq!(p.input_producers().definitions().len(), 1);
        assert_eq!(p.body_producers().definitions().len(), 1);
        let [
            E::Move {
                expression,
                symbol,
                owner,
            },
            E::Result {
                expression: result,
                owner: result_owner,
            },
            E::Binding {
                symbol: bound,
                initializer,
                owner: bound_owner,
            },
        ] = p.prefix_events()
        else {
            panic!("real Move Result Binding");
        };
        assert_eq!(
            (*expression, *result, *initializer),
            (p.initial(), p.initial(), p.initial())
        );
        assert_eq!((*symbol, *bound), (parameter, p.root()));
        assert_eq!(owner, result_owner);
        assert_eq!(owner, bound_owner);
        assert_eq!(
            p.input_producers()
                .definition(owner.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::Parameter(parameter)
        );
    }
}
#[test]
fn parameter_normal_header_declines_legal_neighbor_families() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        PARAMETER_HEADER_SOURCE.replace("var held = incoming", "var held = (incoming)"),
        PARAMETER_HEADER_SOURCE.replace("var held = incoming", "var held = \"other\""),
        PARAMETER_HEADER_SOURCE.replace("index = index + 1", "index = index + 1\n        continue"),
        PARAMETER_HEADER_SOURCE.replace(
            "index = index + 1",
            "index = index + 1\n        if(index < 2) {break}",
        ),
        PARAMETER_HEADER_SOURCE.replace("index + 1", "index + 2"),
        PARAMETER_HEADER_SOURCE.replace("limit: Int,", "limit: Int, stop: Int,"),
        PARAMETER_HEADER_SOURCE.replace("index < limit", "index <= limit"),
    ] {
        let fixture = Fixture::new(&[("parameter-header-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(!owned.owner_observations().iter().any(|o| matches!(
            o.open_loop_observation_v1(),
            Some(
                O::NormalBackedge(_)
                    | O::ContinueBackedge(_)
                    | O::MixedBackedge(_)
                    | O::ConditionalBreakBackedge(_)
                    | O::ConditionalReturnBackedge(_)
            )
        )));
    }
}
#[test]
fn ci_only_parameter_normal_header_rejects_origin_and_transport_damage() {
    use crate::ownership_checking::compilation_unit::owner_transport::HeaderCompletion;
    use crate::ownership_checking::{UnitOwnerOpenLoopObservationV1 as O, UnitOwnerOrigin};
    let fixture = Fixture::new(&[("parameter-header-damage.ko", PARAMETER_HEADER_SOURCE)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBackedge(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..7 {
        let mut p = (**original).clone();
        match damage {
            0 => p.core.initial_origin = UnitOwnerOrigin::String(p.initial()),
            1 => p.core.initial_origin = UnitOwnerOrigin::Parameter(p.root()),
            2 => {
                p.core.prefix.remove(0);
            }
            3 => p.core.prefix.swap(0, 1),
            4 => p.core.increment = None,
            5 => p.core.completion = HeaderCompletion::Continue(p.core.assignment),
            _ => p.core.cleaned = false,
        }
        assert!(!p.is_consistent());
    }
}

const HEADER_BIND_SOURCE: &str = r#"fun run(limit: Int): Unit {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
    }
    val retired = held
}"#;
#[test]
fn normal_header_bind_drop_seals_actual_terminal_consumption() {
    use crate::ownership_checking::{UnitDropPoint, UnitOwnerOpenLoopObservationV1 as O};
    for text in [
        HEADER_BIND_SOURCE.to_string(),
        HEADER_BIND_SOURCE.replace("retired", "finished"),
        HEADER_BIND_SOURCE.replace("limit", "maximum"),
    ] {
        let fixture = Fixture::new(&[("header-bind.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let products = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.open_loop_observation_v1())
            .collect::<Vec<_>>();
        let [O::NormalBindDrop(p)] = products.as_slice() else {
            panic!("normal header bind/drop product");
        };
        assert!(p.is_consistent());
        assert_ne!(p.retired(), p.root());
        assert_eq!(
            p.drop_point(),
            UnitDropPoint::AfterStatement(p.binding_statement())
        );
        assert_eq!(p.input_producers().definitions().len(), 1);
        assert_eq!(p.body_producers().definitions().len(), 1);
        assert_eq!(p.body_events().len(), 6);
        assert_eq!(p.prefix_events().len(), 2);
    }
}
#[test]
fn normal_header_bind_drop_declines_legal_neighbor_protocols() {
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1 as O;
    for text in [
        HEADER_BIND_SOURCE.replace("val retired = held", "val retired = (held)"),
        HEADER_BIND_SOURCE.replace("val retired = held", "var retired = held"),
        HEADER_BIND_SOURCE.replace(
            "val retired = held",
            "val alias = held\n    val retired = alias",
        ),
        HEADER_BIND_SOURCE.replace("index + 1", "index + 2"),
        HEADER_BIND_SOURCE.replace("index = index + 1", "index = index + 1\n        continue"),
        HEADER_BIND_SOURCE.replace("index < limit", "index <= limit"),
        HEADER_BIND_SOURCE
            .replace("limit: Int", "limit: Int, own incoming: String")
            .replace("var held = \"outer\"", "var held = incoming"),
    ] {
        let fixture = Fixture::new(&[("header-bind-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert!(
            !owned
                .owner_observations()
                .iter()
                .any(|o| matches!(o.open_loop_observation_v1(), Some(O::NormalBindDrop(_))))
        );
    }
}
#[test]
fn ci_only_normal_header_bind_drop_rejects_incomplete_consumption() {
    use crate::ownership_checking::compilation_unit::owner_transport::HeaderCompletion;
    use crate::ownership_checking::{UnitDropPoint, UnitOwnerOpenLoopObservationV1 as O};
    let fixture = Fixture::new(&[("header-bind-damage.ko", HEADER_BIND_SOURCE)]);
    let owned = fixture.shared();
    let original = owned
        .owner_observations()
        .iter()
        .find_map(|o| match o.open_loop_observation_v1() {
            Some(O::NormalBindDrop(p)) => Some(p),
            _ => None,
        })
        .unwrap();
    for damage in 0..8 {
        let mut p = (**original).clone();
        match damage {
            0 => p.empty = false,
            1 => p.bound.owner = p.core.backedge.owner(),
            2 => p.taken = p.core.backedge.clone(),
            3 => p.point = UnitDropPoint::AfterStatement(p.core.body),
            4 => p.retired = p.root(),
            5 => p.initializer = p.core.initial,
            6 => p.core.completion = HeaderCompletion::Continue(p.core.assignment),
            7 => p.core.body_completed = false,
            _ => p.result_completed = false,
        }
        assert!(!p.is_consistent());
    }
}
