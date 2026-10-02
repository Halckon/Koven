//! SPEC-0211：循环 provider 的 source 与 element 必须保持借用能力。
use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn checked(text: &str) -> (SourceMap, ParsedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        owned.drops().iter().all(|fact| !matches!(
            fact.target(),
            lang_frontend::ownership_checking::DropTarget::Named(_)
        ) || fact.owner().is_some()),
        "every named cleanup must identify its value definition"
    );
    (sources, parsed, owned)
}

#[path = "ownership_iteration/call_entry_and_pending.rs"]
mod call_entry_and_pending;
#[path = "ownership_iteration/capture_escape.rs"]
mod capture_escape;
#[path = "ownership_iteration/capture_owner_identity.rs"]
mod capture_owner_identity;
#[path = "ownership_iteration/conditional_cleanup.rs"]
mod conditional_cleanup;
#[path = "ownership_iteration/control_exits.rs"]
mod control_exits;
#[path = "ownership_iteration/elvis_continuations.rs"]
mod elvis_continuations;
#[path = "ownership_iteration/enclosing_leaf_snapshots.rs"]
mod enclosing_leaf_snapshots;
#[path = "ownership_iteration/enclosing_owned_captures.rs"]
mod enclosing_owned_captures;
#[path = "ownership_iteration/formed_capture_instances.rs"]
mod formed_capture_instances;
#[path = "ownership_iteration/loop_carried_choices.rs"]
mod loop_carried_choices;
#[path = "ownership_iteration/nested_instance_replay.rs"]
mod nested_instance_replay;
#[path = "ownership_iteration/origin_fixed_point.rs"]
mod origin_fixed_point;
#[path = "ownership_iteration/phi_edges.rs"]
mod phi_edges;
#[path = "ownership_iteration/phi_layout.rs"]
mod phi_layout;
#[path = "ownership_iteration/phi_source_replay.rs"]
mod phi_source_replay;
#[path = "ownership_iteration/prior_owned_instances.rs"]
mod prior_owned_instances;
#[path = "ownership_iteration/recursive_capture_graph.rs"]
mod recursive_capture_graph;
#[path = "ownership_iteration/replacement_snapshots.rs"]
mod replacement_snapshots;
#[path = "ownership_iteration/sibling_source_loans.rs"]
mod sibling_source_loans;
#[path = "ownership_iteration/source_loans.rs"]
mod source_loans;
