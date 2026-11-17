//! SPEC-0455: existing whole-root nullable view/take must use normal refinement.
use super::route;
use crate::{llvm::render_verified_program, ssa::verify::verify_program};

#[test]
fn owned_pointer_nullable_when_views_and_takes_refine_through_the_normal_unit_entry() {
    for (family, declarations, inner) in [
        ("class", "class Node {}", "Node"),
        ("Box", "value class Cell(val value: Int)", "Box<Cell>"),
        ("Rc", "", "Rc<Int>"),
    ] {
        for (mode, parameter, exits) in [
            ("view", "", false),
            ("take", "own ", false),
            ("take-then-return", "own ", true),
        ] {
            let (formal, argument, continuation) = if exits {
                (", own result: Int", ", return", "\nprintln(\"after\")")
            } else {
                ("", "", "")
            };
            let source = format!(
                "{declarations}\n\
                 fun accept({parameter}value: {inner}{formal}): Unit {{}}\n\
                 fun f(own subject: {inner}?): Unit {{\n\
                   when (subject) {{ null -> {{}}; else -> {{ accept((subject){argument}) }} }}{continuation}\n\
                 }}"
            );
            let (lowered, refined) = route(&source);
            let program = lowered.unwrap_or_else(|error| {
                panic!("existing {family} {mode} capability must remain successful: {error:?}")
            });
            verify_program(&program).expect("normal Unit SSA and ownership verify");
            render_verified_program(&program).expect("normal Unit LLVM verifies");
            assert!(
                refined,
                "SPEC-0455: existing {family} {mode} must refine, not just lower successfully"
            );
        }
    }
}

#[test]
fn all_frozen_owned_nullable_when_functions_refine_from_their_original_entry() {
    let common = [
        "repeated",
        "later",
        "earlyReturn",
        "earlyBreak",
        "earlyContinue",
        "temporary",
    ];
    for (family, source) in [
        (
            "Node",
            include_str!("../../native_nullable_when_tests/sources/node.ko"),
        ),
        (
            "Box",
            include_str!("../../native_nullable_when_tests/sources/box.ko"),
        ),
        (
            "Rc",
            include_str!("../../native_nullable_when_tests/sources/rc.ko"),
        ),
    ] {
        let (result, routed) = super::routing::routes_from_entry(source, "entry");
        let program =
            result.unwrap_or_else(|error| panic!("{family}: original Unit failed: {error:?}"));
        verify_program(&program).expect("complete original Unit SSA/ownership");
        render_verified_program(&program).expect("complete original Unit LLVM");
        let mut expected = common.to_vec();
        if family == "Node" {
            expected.extend([
                "temporaryReturn",
                "temporaryBreak",
                "temporaryContinue",
                "doubleReturn",
                "temporaryDoubleReturn",
                "takenReturn",
            ]);
        }
        for function in expected {
            let prefix = format!("koven.p.{function}.d");
            assert!(
                routed.iter().any(|route| route.starts_with(&prefix)),
                "{family}.{function}: the original successful body must refine; routes={routed:?}"
            );
        }
    }
}

#[test]
fn complete_nullable_when_documents_keep_single_and_project_layouts() {
    for source in [
        include_str!("../../native_nullable_when_tests/sources/node.ko"),
        include_str!("../../native_nullable_when_tests/sources/box.ko"),
        include_str!("../../native_nullable_when_tests/sources/rc.ko"),
    ] {
        let outcomes =
            super::super::integer_range_value_tests::collect_sources(&[source.to_owned()]);
        assert_eq!(
            outcomes.len(),
            2,
            "both real single/project layouts execute"
        );
        for (layout, outcome) in outcomes.into_iter().enumerate() {
            let program = outcome.unwrap_or_else(|error| panic!("layout {layout}: {error}"));
            verify_program(&program).expect("layout ownership/SSA verifies");
            render_verified_program(&program).expect("layout LLVM verifies");
        }
    }
}

#[test]
fn nullable_when_input_order_and_uncalled_sibling_keep_the_reached_product() {
    let source = r#"class Node {}
fun observe(value: Node): Unit {}
fun scan(own subject: Node?): Unit {
    when (subject) { null -> {}; else -> { observe((subject)) } }
}
fun entry(): Unit { val subject: Node? = Node(); scan(subject) }
"#;
    let lower = |sibling: &str, reverse| {
        let (result, mut routes) = super::routing::routes_with_sibling(source, sibling, reverse);
        let program = result.expect("the original reached nullable When still lowers");
        verify_program(&program).expect("project ownership/SSA verifies");
        let llvm = render_verified_program(&program).expect("project LLVM verifies");
        routes.sort();
        assert!(routes.iter().any(|name| name.starts_with("koven.p.scan.d")));
        (super::render_program(&program), llvm, routes)
    };
    let control = "fun unused(): Unit {}";
    let forward = lower(control, false);
    assert_eq!(
        forward,
        lower(control, true),
        "same-source reverse input: exact SSA/LLVM/routes"
    );
    for sibling in [
        "fun unused(): Unit { val text: String = \"unreached\" }",
        "fun unused(): Unit { val wide: Long = 1 }",
    ] {
        assert_eq!(
            forward,
            lower(sibling, false),
            "uncalled local types must not pollute reached products"
        );
    }
}
