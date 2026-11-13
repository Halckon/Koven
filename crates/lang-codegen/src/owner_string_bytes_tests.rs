//! Ordinary String ABI data boundaries through existing parent/child owner receipts.
use super::{OwnedProducts, UnitSource};

struct BytesCase {
    seed: &'static str,
    seed_bytes: &'static str,
    initial: &'static str,
    initial_bytes: &'static str,
    child_a: &'static str,
    child_a_bytes: &'static str,
    child_b: &'static str,
    parent: &'static str,
    parent_bytes: &'static str,
}
fn cases() -> [BytesCase; 2] {
    [
        BytesCase {
            seed: r#""首\0"+"尾""#,
            seed_bytes: "首\0尾",
            initial: r#""""#,
            initial_bytes: "",
            child_a: r#""子\0甲""#,
            child_a_bytes: "子\0甲",
            child_b: r#""🙂\0乙""#,
            parent: r#""\0末""#,
            parent_bytes: "\0末",
        },
        BytesCase {
            seed: r#"""+"""#,
            seed_bytes: "",
            initial: r#""界\0é""#,
            initial_bytes: "界\0é",
            child_a: r#""""#,
            child_a_bytes: "",
            child_b: r#""\0中""#,
            parent: r#""终🙂\0""#,
            parent_bytes: "终🙂\0",
        },
    ]
}
fn fixture(case: &BytesCase, constant: bool) -> (UnitSource, Vec<u8>) {
    let mut calls = String::new();
    let mut expected = Vec::new();
    for (outer, inner) in [(0, 2), (1, 0), (1, 1), (2, 2)] {
        let count = if constant && outer == 2 {
            "TWO".into()
        } else {
            outer.to_string()
        };
        calls.push_str(&format!("run({count},{inner},{});", case.seed));
        let a = if outer > 0 && inner > 0 {
            case.child_a_bytes
        } else {
            case.seed_bytes
        };
        let b = if outer > 0 {
            case.parent_bytes
        } else {
            case.initial_bytes
        };
        expected.extend_from_slice(a.as_bytes());
        expected.push(b'\n');
        expected.extend_from_slice(b.as_bytes());
        expected.push(b'\n');
    }
    let text = format!(
        r#"{}
        fun run(outer:Int,inner:Int,own seed:String) {{
            var a=seed; var b={}; var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ val x=replace(&a,{}); val y=replace(&b,{});
                    n1=n1-1; g1=n1>0 }};
                val old=replace(&b,{}); n0=n0-1; g0=n0>0
            }}; println(a); println(b)
        }}
        fun entry() {{ {calls} }}"#,
        if constant { "const val TWO:Int=2" } else { "" },
        case.initial,
        case.child_a,
        case.child_b,
        case.parent
    );
    let source = UnitSource::analyze("owner-string-bytes.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        matches!(source.products, OwnedProducts::Constant(..)),
        constant
    );
    (source, expected)
}
fn assert_existing_receipts(source: &UnitSource) {
    let product = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .unwrap();
    assert!(product.child_generation_rule_v1().is_some());
    assert!(product.parent_boundary_v1().is_some());
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    for marker in [
        "child-cycle-ssa-complete",
        "parent-generation-composition-ssa-complete",
    ] {
        assert_eq!(
            actions
                .iter()
                .filter(|(name, _)| *name == marker)
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [2]
        );
    }
}
#[test]
fn string_byte_boundaries_retain_parent_child_authority() {
    for case in cases() {
        for constant in [false, true] {
            let (source, _) = fixture(&case, constant);
            assert_existing_receipts(&source);
        }
    }
}
#[test]
fn string_byte_boundaries_survive_native_owner_transport() {
    for case in cases() {
        for constant in [false, true] {
            let (source, expected) = fixture(&case, constant);
            assert_existing_receipts(&source);
            super::assert_native_output(&source, &expected);
        }
    }
}
