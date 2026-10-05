//! SPEC-0276: production cache bounds do not replace entry-local planner limits.
use super::*;

const GROW: &str = "package p\n\
    fun <T> grow(): Unit {\n\
        val values: List<T> = listOf()\n\
        grow<List<T>>()\n\
    }";

#[test]
fn unit_generic_body_native_production_limit_rejects_growth_preserving_outputs() {
    let analysis = analyze_sources(GROW, "package q\nfun entry(): Unit { p.grow<Int>() }");
    let arena = analysis.typed.types().types();
    let int = arena.builtin(BuiltinType::Int).unwrap();
    assert!(
        arena
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![int],
            })
            .is_some(),
        "body-only cache must prepare the actual growth prefix"
    );
    let span = analysis
        .provider
        .ast()
        .items()
        .get(
            analysis.names.names().index().declarations()
                [analysis.declaration("p", "grow").index()]
            .root(),
        )
        .unwrap()
        .span();
    assert_eq!(span.source_id(), analysis.provider_source);
    let arena_len = arena.len();
    let snapshot = |directory: &Path| {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    for existing_target in [false, true] {
        let directory = TestDirectory::create();
        let object = directory.join("growth.o");
        fs::write(directory.join("neighbor.bin"), b"neighbor\0unchanged").unwrap();
        if existing_target {
            fs::write(&object, b"old-object\0bytes").unwrap();
        }
        let before = snapshot(&directory.0);
        // This entry is never linked or executed: its actual graph must be
        // rejected by the existing Phase 4 specialization budget before emit.
        let error = emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect_err("the production planner rejects the selected growing instance graph");
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        assert_eq!(error.span(), Some(span));
        assert_eq!(
            error.to_string(),
            "native object UnsupportedSource: frontend lowering failed with InstanceLimitExceeded"
        );
        assert_eq!(arena.len(), arena_len);
        assert_eq!(
            snapshot(&directory.0),
            before,
            "all names and bytes survive"
        );
        assert_eq!(object.exists(), existing_target);
        assert_no_sibling_temporary(&directory.0);
    }
}

#[test]
fn unit_generic_body_native_unselected_growth_seeds_preserve_normal_entry() {
    for closed_seed in [false, true] {
        let provider = if closed_seed {
            format!("{GROW}\nfun unused(): Unit {{ grow<Int>() }}")
        } else {
            GROW.to_owned()
        };
        let analysis = analyze_sources(
            &provider,
            "package q\nfun entry(): Unit { println(\"normal-growth\") }",
        );
        let arena = analysis.typed.types().types();
        let int = arena.builtin(BuiltinType::Int).unwrap();
        assert_eq!(
            arena
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![int],
                })
                .is_some(),
            closed_seed,
            "only the real closed unused() call seeds the bounded growth cache"
        );
        let calls = analysis
            .typed
            .types()
            .calls()
            .iter()
            .filter(|call| {
                call.target()
                    == lang_frontend::type_checking::UnitCallTarget::Declaration(
                        analysis.declaration("p", "grow"),
                    )
                    && call.instance().type_arguments() == [int]
            })
            .count();
        assert_eq!(calls, usize::from(closed_seed));
        let arena_len = arena.len();
        // The selected normal entry only prints. The recursive source remains
        // present, including the real closed seed in the second control.
        run(&analysis, b"normal-growth\n");
        assert_eq!(arena.len(), arena_len);
    }
}
