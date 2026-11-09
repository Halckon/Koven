//! Constant branch lowering must keep the only possible short-circuit edge.

use super::lower_short_circuit_fixture;
use crate::ssa::refine::observe_routing;

#[test]
fn constant_never_rhs_does_not_create_a_normal_false_exit() {
    for tail in [
        "if (true || ((return 7) == 0)) { return 8 }",
        "while (true || ((return 7) == 0)) {}",
    ] {
        let source = format!(
            "fun entry(flag: Boolean): Int {{
                 when {{ flag -> {{}}; else -> {{}} }}
                 {tail}
             }}"
        );
        let (program, routed) = observe_routing(|| lower_short_circuit_fixture(&source));
        assert!(routed.iter().any(|name| name.contains(".entry.")), "{tail}");
        crate::llvm::render_verified_program(&program).expect("verified Const control flow");
    }
}
