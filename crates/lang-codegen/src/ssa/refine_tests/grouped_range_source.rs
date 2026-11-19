//! Nested primitive sources now lower and verify through default refine (P1-3).
use super::*;

#[test]
fn nested_range_sources_lower_and_verify_in_default_refine() {
    for depth in [0, 1, 3] {
        for backing in ["source", "listOf(3, 4)"] {
            let source = format!(
                "{}rangeView({backing}, 0, 1){}",
                "(".repeat(depth),
                ")".repeat(depth)
            );
            let text = format!(
                "package p\nfun observe(view: View<Int>): Int = view.size\nfun consume(own source: List<Int>): Int = source.size\nfun main(): Int {{ val source = listOf(1, 2); val count = observe(rangeView({source}, 0, 1)); return count + consume(source) }}"
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
                let (program, _) = lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types_env, &typed, &owned, entry,
                )
                .unwrap_or_else(|error| {
                    panic!("nested primitive source must lower in default refine: {error:?}")
                });
                let llvm = crate::llvm::render_verified_program(&program)
                    .unwrap_or_else(|error| panic!("verified LLVM failed: {error:?}"));
                assert!(!llvm.is_empty());
            }
        }
    }
}
