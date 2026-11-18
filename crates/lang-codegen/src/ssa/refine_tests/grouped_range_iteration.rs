//! Existing range iteration lowering consumes the actual call behind transparent groups.
use super::*;

#[test]
fn grouped_range_iteration_reaches_verified_ssa_and_llvm() {
    for depth in [0, 1, 3] {
        for backing in ["source", "listOf(3, 4)"] {
            for body in ["total = total + item", "break", "continue", "return item"] {
                let expression = format!(
                    "{}rangeView({backing}, 0, 1){}",
                    "(".repeat(depth),
                    ")".repeat(depth)
                );
                let (setup, tail) = if backing == "source" {
                    ("val source = listOf(1, 2);", "total + source.size")
                } else {
                    ("", "total")
                };
                let text = format!(
                    "package p\nfun main(): Int {{ {setup} var total = 0; for(item in {expression}) {{ {body} }}; return {tail} }}"
                );
                let mut sources = SourceMap::new();
                let (id, file) = parsed(&mut sources, "p/main.ko", &text);
                for input in [
                    SourceUnitInput::memory_document(id, &file),
                    SourceUnitInput::new("root", "p/main.ko", id, &file),
                ] {
                    let inputs = [input];
                    let (names_env, mut types_env) = standard_environments();
                    types_env.authorize_range_source(&sources, id).unwrap();
                    let (names, typed, owned) = analyze(&sources, &inputs, &names_env, &types_env);
                    let entry = declaration(&names, "p", "main");
                    let ((program, _), routed) =
                        super::super::super::refine::observe_routing(|| {
                            lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types_env, &typed, &owned, entry,
                            )
                            .expect("grouped iteration uses existing range lowering")
                        });
                    assert!(
                        routed.iter().any(|name| name.starts_with("koven.p.main.d")),
                        "{routed:?}"
                    );
                    crate::ssa::verify::verify_program(&program).unwrap();
                    crate::llvm::render_verified_program(&program).unwrap();
                }
            }
        }
    }
}
