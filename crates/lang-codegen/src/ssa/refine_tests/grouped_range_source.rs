//! Nested primitive sources retain the existing default-refine boundary.
use super::*;

#[test]
fn nested_range_sources_keep_existing_default_refine_boundary() {
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
                let Err(error) = lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types_env, &typed, &owned, entry,
                ) else {
                    panic!("nested primitive source remains outside default refine");
                };
                assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
                assert_eq!(
                    sources.slice(error.span.expect("original range expression")),
                    Ok(format!("rangeView({source}, 0, 1)").as_str()),
                );
            }
        }
    }
}
