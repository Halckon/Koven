//! Shared source cases describe the selected element/capture surfaces, not lowering behavior.

pub(in crate::ssa) struct Case {
    pub element: &'static str,
    pub declarations: &'static str,
    pub source: &'static str,
    pub initializers: [&'static str; 3],
}

pub(in crate::ssa) const CASES: [Case; 4] = [
    Case {
        element: "Int",
        declarations: "",
        source: "val scale = 7",
        initializers: [
            "{ index -> index }",
            "{ index -> index + scale }",
            "move { index -> index + scale }",
        ],
    },
    Case {
        element: "String",
        declarations: "",
        source: "val scale = \"suffix\"",
        initializers: [
            "{ index -> \"value\" }",
            "{ index -> \"value\" + scale }",
            "move { index -> \"value\" + scale }",
        ],
    },
    Case {
        element: "Leaf",
        declarations: "class Leaf(val number: Int) { deinit() { println(\"leaf\") } }",
        source: "val scale = Leaf(7)",
        initializers: [
            "{ index -> Leaf(index) }",
            "{ index -> Leaf(index + scale.number) }",
            "move { index -> Leaf(index + scale.number) }",
        ],
    },
    Case {
        element: "Unit",
        declarations: "fun touch(index: Int): Unit {}",
        source: "val scale = 7",
        initializers: [
            "{ index -> touch(index) }",
            "{ index -> touch(index + scale) }",
            "move { index -> touch(index + scale) }",
        ],
    },
];

pub(in crate::ssa) fn source(
    case: &Case,
    container: &str,
    environment: usize,
    named: bool,
    length: i32,
) -> String {
    let literal = case.initializers[environment];
    let callback = if named {
        format!("val callback: (Int) -> {} = {literal}\n", case.element)
    } else {
        String::new()
    };
    let initializer = if named { "callback" } else { literal };
    format!(
        "{}\nfun entry(): Int {{ {}\n{callback}\
         val items = {container}<{}>({length}, {initializer})\nreturn items.size }}",
        case.declarations, case.source, case.element,
    )
}

pub(in crate::ssa) fn assert_layout(program: &crate::ssa::model::Program, environment: usize) {
    use crate::ssa::model::{ClosureCaptureMode, SsaTypeKind};
    let captures = program.modules[0]
        .types
        .iter()
        .filter_map(|kind| match kind {
            SsaTypeKind::ConcreteClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .collect::<Vec<_>>();
    if environment == 0 {
        assert!(
            captures.is_empty(),
            "capture-free source uses a pointer ABI"
        );
        assert!(
            program.modules[0]
                .types
                .iter()
                .any(|kind| matches!(kind, SsaTypeKind::FunctionPointer { .. }))
        );
    } else {
        assert_eq!(
            captures.len(),
            1,
            "one actual lambda has one concrete layout"
        );
        assert_eq!(captures[0].len(), 1, "the actual source has one capture");
        assert_eq!(
            captures[0][0].mode,
            if environment == 1 {
                ClosureCaptureMode::Shared
            } else {
                ClosureCaptureMode::Owned
            },
            "ordinary expected Fn must preserve actual capture mode"
        );
    }
}
