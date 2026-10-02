use super::*;

#[test]
fn loop_origin_fixed_point_reaches_prior_bindings_after_multiple_backedges() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            var h: () -> Unit = {}
            for (_ in flags) {
                { h = g }
                { g = f }
                { f = ({ val marker = 7 }) }
            }
            val first = h()
            val second = g()
            val third = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    for name in ["f", "g", "h"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for bindings in [plan.closure_flow().header(), plan.closure_flow().exit()] {
            assert!(
                bindings.iter().any(|binding| binding.symbol() == symbol
                    && binding.origins().iter().any(|&id| sources
                        .slice(parsed.ast().expressions().get(id).unwrap().span())
                        .unwrap()
                        .contains("marker"))),
                "{name}: the last lambda reaches every carried binding; one simulated iteration is insufficient"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_keeps_jump_edges_and_unreachable_tail_separate() {
    for (transfer, in_header, in_exit) in [
        ("continue", true, true),
        ("break", false, true),
        ("return", false, false),
        ("error(\"stop\")", false, false),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flags: List<Boolean>) {{
                var f: () -> Unit = {{}}
                for (_ in flags) {{
                    {{ f = ({{ val selected = 1 }}) }}
                    {transfer}
                    {{ f = ({{ val unreachable = 2 }}) }}
                }}
                val used = f()
            }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{transfer}: {:?}",
            owned.diagnostics()
        );
        let flow = owned.iterations()[0].closure_flow();
        for (rows, expected) in [(flow.header(), in_header), (flow.exit(), in_exit)] {
            let values = rows
                .iter()
                .flat_map(|row| row.origins())
                .map(|&id| {
                    sources
                        .slice(parsed.ast().expressions().get(id).unwrap().span())
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                values.iter().any(|text| text.contains("selected")),
                expected,
                "{transfer}: {values:?}"
            );
            assert!(
                !values.iter().any(|text| text.contains("unreachable")),
                "{transfer}: dead tail cannot add an origin"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_preserves_elvis_value_and_non_null_continuation() {
    {
        let (sources, parsed, owned) = checked(
            "fun run(flags: List<Boolean>) {
                var f: () -> Unit = {}
                val none: Nothing? = null
                for (_ in flags) { { f = (none ?: ({ val selected = 1 })) }\nbreak }
                val used = f()
            }",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(
            owned.iterations()[0]
                .closure_flow()
                .exit()
                .iter()
                .flat_map(|row| row.origins())
                .any(|&id| sources
                    .slice(parsed.ast().expressions().get(id).unwrap().span())
                    .unwrap()
                    .contains("selected")),
            "Elvis delivers its selected lambda to the binding"
        );
    }
    for (operand, continues) in [("maybe", true), ("none", false)] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flags: List<Boolean>, maybe: Int?) {{
                var f: () -> Unit = {{}}
                val none: Nothing? = null
                for (_ in flags) {{
                    val observed = {operand} ?: return
                    {{ f = ({{ val selected = 1 }}) }}
                }}
                val used = f()
            }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let flow = owned.iterations()[0].closure_flow();
        for rows in [flow.header(), flow.exit()] {
            assert_eq!(
                rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                    .slice(parsed.ast().expressions().get(id).unwrap().span())
                    .unwrap()
                    .contains("selected")),
                continues,
                "only the nullable non-null path bypasses the returning RHS"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_propagates_outer_backedges_into_nested_and_later_loops() {
    let (sources, parsed, owned) = checked(
        "fun run(outer: List<Boolean>, inner: List<Boolean>, after: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in outer) {
                for (_ in inner) {
                    val g = f
                    { f = g }
                }
                { f = ({ val later = 1 }) }
            }
            for (_ in after) { val used = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 3);
    for plan in owned.iterations() {
        let rows = plan.closure_flow().header();
        assert!(
            rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("later")),
            "every loop, including the inner and following loops, sees the outer backedge source"
        );
        assert!(
            rows.iter().all(|row| row
                .origins()
                .windows(2)
                .all(|pair| pair[0].index() < pair[1].index())),
            "origins are unique and deterministically ordered"
        );
    }
}

#[test]
fn loop_origin_fixed_point_consumes_inner_jumps_and_drops_lexical_aliases() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in flags) {
                val alias = f
                { f = alias }
                loop { break }
                { f = ({ val reached = 1 }) }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let alias = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "alias")
        .unwrap()
        .id();
    let flow = owned.iterations()[0].closure_flow();
    for rows in [flow.header(), flow.exit()] {
        assert!(
            rows.iter().all(|row| row.symbol() != alias),
            "dead local alias cannot become a carried owner"
        );
        assert!(
            rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("reached")),
            "the inner break does not terminate the outer body"
        );
    }
}

#[test]
fn loop_origin_fixed_point_isolates_nested_callable_bindings_and_jumps() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in flags) {
                val action: () -> Unit = { for (_ in flags) { break } }
                val invoked = action()
                { f = ({ val continued = 1 }) }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 2);
    let outer = owned
        .iterations()
        .iter()
        .find(|plan| {
            sources
                .slice(
                    parsed
                        .ast()
                        .statements()
                        .get(plan.descriptor().statement())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .contains("action")
        })
        .unwrap();
    let inner = owned
        .iterations()
        .iter()
        .find(|plan| plan.descriptor().statement() != outer.descriptor().statement())
        .unwrap();
    assert!(
        inner.closure_flow().header().is_empty(),
        "callable state includes captures, not arbitrary creating-scope bindings"
    );
    assert!(
        outer
            .closure_flow()
            .header()
            .iter()
            .flat_map(|row| row.origins())
            .any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("continued")),
        "break in the nested callable cannot terminate the creating callable's loop"
    );
}

#[test]
fn loop_origin_fixed_point_keeps_while_condition_effects_on_exhaustion() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>, flag: Boolean) {
            var f: () -> Unit = {}
            while (if (flag) { { f = ({ val selected = 1 }) }\nfalse } else false) { return }
            for (_ in flags) { val invoked = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    assert!(
        owned.iterations()[0]
            .closure_flow()
            .header()
            .iter()
            .flat_map(|row| row.origins())
            .any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("selected")),
        "while has no backedge; the following loop receives the condition's completed state, not the entry header"
    );
}
