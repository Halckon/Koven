//! SPEC-0179：compiler-bound 借用迭代的 Phase 2 契约。
//! 原八组同源测试及事实已由 type_iteration_unit 的双入口固定 oracle 承接；
//! 此处保留名称失败后 raw typing 不级联的独立 Single 恢复 witness。
#[path = "type_iteration/map_binding_controls/analysis.rs"]
mod map_binding_analysis;

use lang_frontend::{
    name_resolution::resolve_names,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[path = "type_iteration/map_borrowed.rs"]
mod map_borrowed;

#[path = "type_iteration/map_binding_controls.rs"]
mod map_binding_controls;

#[path = "type_iteration/pattern_recovery_neighbors.rs"]
mod pattern_recovery_neighbors;

#[test]
fn upstream_errors_inside_element_types_do_not_publish_recovery_plans() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "poison.ko",
            "fun run(xs: List<Missing>) { for (n in xs) {} }",
        )
        .unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "poison");
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert_eq!(names.diagnostics().len(), 1);
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(
        typed.diagnostics().is_empty(),
        "no iteration cascade: {:?}",
        typed.diagnostics()
    );
    assert!(typed.sequential_iterations().is_empty());
}
