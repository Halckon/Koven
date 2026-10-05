use super::*;
use crate::ssa::model::{ClosureCaptureType, Program};

fn borrowed(module: &mut Module) -> SsaTypeId {
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let reference = module
        .add_shared_reference_type(integer)
        .expect("reference");
    let environment = module
        .add_aggregate_type("Env", vec![reference])
        .expect("environment");
    module
        .add_concrete_closure_type(
            "Borrowed",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: integer,
            }],
        )
        .expect("closure")
}

#[test]
fn capture_type_graph_deep_storage_uses_iteration_and_visits_each_edge_once() {
    let mut program = Program::default();
    let id = program.add_module("deep-capture-types");
    let module = program.module_mut(id).expect("module");
    let mut current = borrowed(module);
    // This is an IR type-classification test, not a claim of source/native deep-layout support.
    for index in 0..2000 {
        current = module
            .add_aggregate_type(format!("Layer{index}"), vec![current])
            .expect("layer");
    }
    let proof = TypeCaptures::compute(module);
    assert!(proof.contains(current));
    assert_eq!(proof.stats.node_visits, 2004);
    assert_eq!(proof.stats.storage_edge_visits, 2001);
    assert_eq!(proof.stats.propagation_edge_visits, 2000);
}

#[test]
fn capture_type_graph_shared_dag_counts_storage_edges_instead_of_expanding_paths() {
    let mut program = Program::default();
    let id = program.add_module("shared-capture-types");
    let module = program.module_mut(id).expect("module");
    let mut current = borrowed(module);
    // Each layer has two paths to one existing identity, never two newly cloned subgraphs.
    for index in 0..256 {
        current = module
            .add_aggregate_type(format!("Pair{index}"), vec![current, current])
            .expect("pair");
    }
    let proof = TypeCaptures::compute(module);
    assert!(proof.contains(current));
    assert_eq!(proof.stats.node_visits, 260);
    assert_eq!(proof.stats.storage_edge_visits, 513);
    assert_eq!(proof.stats.propagation_edge_visits, 512);
}

#[test]
fn capture_type_graph_owner_cycles_reach_a_fixed_point_and_clean_cycles_stay_clean() {
    let mut program = Program::default();
    let id = program.add_module("recursive-capture-types");
    let module = program.module_mut(id).expect("module");
    let closure = borrowed(module);
    let owner = module.declare_heap_owner("Recursive").expect("owner");
    let payload = module
        .add_aggregate_type("Payload", vec![owner, closure])
        .expect("payload");
    module
        .define_heap_owner(owner, payload)
        .expect("recursive payload");
    let clean_owner = module
        .declare_heap_owner("CleanRecursive")
        .expect("clean owner");
    let clean_payload = module
        .add_aggregate_type("CleanPayload", vec![clean_owner])
        .expect("clean payload");
    module
        .define_heap_owner(clean_owner, clean_payload)
        .expect("clean recursive payload");
    let proof = TypeCaptures::compute(module);
    assert!(proof.contains(owner));
    assert!(proof.contains(payload));
    assert!(!proof.contains(clean_owner));
    assert!(!proof.contains(clean_payload));
    assert_eq!(proof.stats.node_visits, 8);
    assert_eq!(proof.stats.storage_edge_visits, 6);
    assert_eq!(proof.stats.propagation_edge_visits, 3);
}

#[test]
fn capture_type_graph_does_not_treat_signatures_or_reference_targets_as_owned_storage() {
    let mut program = Program::default();
    let id = program.add_module("capture-storage-boundaries");
    let module = program.module_mut(id).expect("module");
    let closure = borrowed(module);
    let pointer = module
        .add_function_pointer_type(vec![closure], vec![closure])
        .expect("pointer signature");
    let reference = module
        .add_shared_reference_type(closure)
        .expect("reference target");
    let wrapper = module
        .add_aggregate_type("LeafWrapper", vec![pointer, reference])
        .expect("wrapper");
    let proof = TypeCaptures::compute(module);
    assert!(proof.contains(closure));
    for leaf in [pointer, reference, wrapper] {
        assert!(!proof.contains(leaf));
    }
    assert_eq!(proof.stats.node_visits, 7);
    assert_eq!(proof.stats.storage_edge_visits, 3);
    assert_eq!(proof.stats.propagation_edge_visits, 0);
}
