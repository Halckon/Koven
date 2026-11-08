use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

// This guard is not a replacement for Single conditional owner/phi witnesses.
fn check(text: &str, rejected_at: Option<&str>) {
    let rejected = rejected_at.is_some();
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("iteration.ko", text).unwrap();
        let parsed =
            super::parser_test_assertions::parse_file_twice(&sources, source, "loop choice Unit");
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        let inputs = [if rooted {
            SourceUnitInput::new("root", "iteration.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        }];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        assert!(names.names().diagnostics().is_empty());
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();

        assert!(owned.is_compatible_with(&typed));
        assert!(owned.deferred().is_empty());
        let codes = owned
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            if rejected { vec!["L0164"] } else { vec![] },
            "rooted={rooted}: {text}"
        );
        assert_eq!(owned.clone().validate().is_ok(), !rejected);
        if let Some(origin) = rejected_at {
            assert_eq!(
                sources.slice(owned.diagnostics()[0].primary_span()),
                Ok(origin),
                "rooted={rooted}: {text}"
            );
            assert!(owned.drops().is_empty());
            assert!(owned.captures().is_empty());
            assert!(owned.iterations().is_empty());
        }
    }
}

#[test]
fn new_shared_choice_cannot_survive_loop_edges_without_owner_transport() {
    for transfer in ["", "continue", "break"] {
        for loop_head in ["for (flag in flags)", "while (flag)", "loop"] {
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>, flag: Boolean) {{ var f: () -> Unit = {{}}\n{loop_head} {{ f = if (flag) ({{ read(xs) }}) else ({{ read(ys) }})\n{transfer} }}\nval used = f() }}"
            );
            // A loop without a break has no reachable post-loop call, but its
            // next iteration still overwrites an untracked shared environment.
            if loop_head == "loop" && transfer != "break" {
                continue;
            }
            check(&text, Some("xs"));
        }
    }
}

#[test]
fn established_and_body_local_closures_keep_their_supported_paths() {
    for loop_head in ["for (flag in flags)", "while (flag)", "while (false)"] {
        for transfer in ["", "continue", "break"] {
            for body in [
                "f()",
                "val local: () -> Unit = { read(xs) }; local()",
                "val noCapture: () -> Unit = {}; noCapture()",
            ] {
                let text = format!(
                    "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flags: List<Boolean>, flag: Boolean) {{ val f: () -> Unit = {{ read(xs) }}; {loop_head} {{ {body}\n{transfer} }}\nf() }}"
                );
                check(&text, None);
            }
        }
    }
}

#[test]
fn capture_free_and_copyable_rebinding_remain_supported() {
    for rhs in ["{}", "{ val n = number }"] {
        let text = format!(
            "fun run(flags: List<Boolean>, number: Int) {{ var f: () -> Unit = {{}}; for (flag in flags) {{ f = {rhs} }}\nf() }}"
        );
        check(&text, None);
    }
}

#[test]
fn borrowed_parameter_capture_does_not_create_a_local_drop_obligation() {
    for parameter in ["xs: List<Int>", "inout xs: List<Int>"] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run({parameter}, flags: List<Boolean>) {{ var f: () -> Unit = {{}}; for (flag in flags) {{ f = {{ read(xs) }} }}\nf() }}"
        );
        check(&text, None);
    }
}

#[test]
fn nested_and_repeating_environment_transport_is_rejected() {
    for body in [
        "val inner: () -> Unit = { read(xs) }; f = move { inner() }",
        "for (other in flags) { f = { read(xs) }; break }",
        "f(); f = { read(xs) }",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flags: List<Boolean>) {{ var f: () -> Unit = {{}}; for (flag in flags) {{ {body} }}\nf() }}"
        );
        check(&text, Some("xs"));
    }
}

#[test]
fn new_environment_ending_inside_the_body_does_not_need_exit_transport() {
    check(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) { var f: () -> Unit = {}; for (flag in flags) { f = { read(xs) }; f() } }",
        None,
    );
}

#[test]
fn statically_empty_loops_do_not_reject_unexecuted_capture_transport() {
    for head in [
        "while (false)",
        "while ((false))",
        "for (flag in listOf<Boolean>())",
        "for (flag in (listOf<Boolean>()))",
        "for (flag in MutableList<Boolean>())",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>) {{ var f: () -> Unit = {{}}; {head} {{ f = {{ read(xs) }} }}\nf() }}"
        );
        check(&text, None);
    }
}

#[test]
fn zero_iteration_exemption_requires_independent_literal_or_intrinsic_proof() {
    for head in [
        "for (flag in listOf(true))",
        "while (falseFlag())",
        "for (flag in empty())",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun falseFlag(): Boolean = false\nfun empty(): List<Boolean> = listOf<Boolean>()\nfun run(own xs: List<Int>) {{ var f: () -> Unit = {{}}; {head} {{ f = {{ read(xs) }} }}\nf() }}"
        );
        check(&text, Some("xs"));
    }
}
