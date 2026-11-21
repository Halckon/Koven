//! Local destructuring and temporary sequential-source routing contracts.

use super::route;

#[test]
fn temporary_sequential_for_routes_with_local_destructuring_on_either_side() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for constructor in ["arrayOf", "listOf", "mutableListOf"] {
        let iteration = format!(
            "for ((value, _) in {constructor}(Pair(20, 22))) {{ if (value == 20) {{ println(\"loop\") }} }}"
        );
        for (before, after) in [(pattern, iteration.as_str()), (&iteration, pattern)] {
            let source = format!(
                "value class Pair(val first: Int, val second: Int)\n\
                 fun f(): Int {{ {before}; {after}; return first }}"
            );
            let (result, routed) = route(&source);
            assert!(
                routed,
                "local destructuring requires refine: {source}\n{:?}",
                result.as_ref().err()
            );
            let program = result.expect("admitted body must lower");
            crate::ssa::verify::verify_program(&program).expect("program verifies");
        }
    }
}

#[test]
fn temporary_sequential_for_does_not_inherit_sibling_local_destructuring() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun sibling(): Int { val (second, first) = Pair(20, 22); return first }
    fun f(): Unit { for (item in listOf("kept")) { println(item) } }
    "#;
    let (result, routed) = route(source);
    assert!(!routed, "f has no local destructuring of its own");
    crate::ssa::verify::verify_program(&result.expect("the retained route lowers"))
        .expect("program verifies");
}

#[test]
fn while_and_temporary_for_route_after_local_destructuring() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for constructor in ["arrayOf", "listOf", "mutableListOf"] {
        let iteration = format!(
            "while (false) {{}}; for ((value, _) in {constructor}(Pair(20, 22))) {{ if (value == 20) {{ println(\"loop\") }} }}"
        );
        for (before, after) in [(pattern, iteration.as_str()), (&iteration, pattern)] {
            let source = format!(
                "value class Pair(val first: Int, val second: Int)\n\
                 fun f(): Int {{ {before}; {after}; return first }}"
            );
            let (result, routed) = route(&source);
            assert!(routed, "{source}\n{:?}", result.as_ref().err());
            crate::ssa::verify::verify_program(&result.expect("combined provider lowers"))
                .expect("verified program");
        }
    }
}

#[test]
fn while_local_destructuring_refine_invalidates_reassigned_nullable_view() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(flag: Boolean): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (flag && first == 20) {
                outer = "second"
                break
            }
        }
        for ((value, _) in listOf(Pair(20, 22))) {
            if (value == first) { println("loop") }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("nullable loop lowers"))
        .expect("verified program");
}

#[test]
fn while_and_temporary_for_keep_legacy_when_destructuring_is_not_reached() {
    let cases = [
        "fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
        "fun sibling(): Int { val (second, first) = Pair(20, 22); return first }\n\
         fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
        "fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) }; return; val (second, first) = Pair(20, 22) }",
        "fun f(): Unit { val nested = { val (second, first) = Pair(20, 22); first }; while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
    ];
    for code in cases {
        let source = format!("value class Pair(val first: Int, val second: Int)\n{code}");
        let (result, routed) = route(&source);
        assert!(!routed, "{code}");
        crate::ssa::verify::verify_program(&result.expect("legacy route still lowers"))
            .expect("verified program");
    }
}

#[test]
fn constant_while_edges_cannot_lend_destructuring_route() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for code in [
        format!(
            "while (false) {{ {pattern} }}; for (item in listOf(\"kept\")) {{ println(item) }}"
        ),
        format!("for (item in listOf(\"kept\")) {{ println(item) }}; while (true) {{}}; {pattern}"),
    ] {
        let source = format!(
            "value class Pair(val first: Int, val second: Int)\nfun f(): Unit {{ {code} }}"
        );
        let (result, routed) = route(&source);
        assert!(!routed, "{code}");
        let program = result.unwrap_or_else(|error| panic!("{code}: {error:?}"));
        crate::ssa::verify::verify_program(&program).expect("reachable loop edges verify");
    }
}

const TERMINATING_CONSTRUCTOR_PREFIX: &str = r#"
        class Plain(val first: Int, val second: Int)
        fun firstArgument(): Int { println("first argument"); return 1 }
        fun f(flag: Boolean): Unit {
            val result = Plain(firstArgument(), if (flag) {
                while (true) { return }
                1
            } else {
                while (true) { return }
                2
            })
        }
    "#;

#[path = "temporary_sequential_for/mixed_constructor_loop.rs"]
mod mixed_constructor_loop;

#[test]
fn terminating_constructor_prefix_lowers_on_the_retained_route() {
    let source = TERMINATING_CONSTRUCTOR_PREFIX;
    let (result, routed) = route(source);
    assert!(
        !routed,
        "the scalar-field construction stays on retained Unit"
    );
    crate::ssa::verify::verify_program(&result.expect("the abandoned constructor lowers"))
        .expect("both return edges and the evaluated prefix verify");
}

#[test]
fn observe_terminating_constructor_prefix_frontend_boundary() {
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        parser::{Expression, Statement},
        source::SourceMap,
        type_checking::{UnitExpressionId, UnitStatementId, standard_environments},
    };

    let source = TERMINATING_CONSTRUCTOR_PREFIX;
    let continuing = source.replace("while (true) { return }\n                2", "2");
    assert_ne!(continuing, source);
    for (text, terminating) in [(source, true), (&continuing, false)] {
        let mut sources = SourceMap::new();
        let source_text = format!("package p\n{text}");
        let (id, file) =
            crate::ssa::unit_lower_test_support::parsed(&mut sources, "p/main.ko", &source_text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (env, types) = standard_environments();
        let (names, typed, owned) =
            crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &env, &types);
        let [unit] = names.names().index().source_units() else {
            panic!("single source");
        };
        let (expr, call) = file
            .ast()
            .expressions()
            .iter()
            .find(|(_, expr)| {
                matches!(expr.payload(), Expression::Call { .. })
                    && sources
                        .slice(expr.span())
                        .unwrap()
                        .starts_with("Plain(firstArgument(),")
            })
            .expect("exact original constructor expression");
        let Expression::Call { arguments, .. } = call.payload() else {
            unreachable!()
        };
        let [first, conditional] = arguments.as_slice() else {
            panic!("two exact arguments");
        };
        let [call, first, conditional] = [expr, first.value, conditional.value]
            .map(|expr| UnitExpressionId::new(unit.id(), expr));
        let facts = typed.types();
        let loops: Vec<_> = file
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::While { .. }))
            .map(|(id, node)| {
                (
                    node.span(),
                    facts.loop_falls_through(UnitStatementId::new(unit.id(), id)),
                )
            })
            .collect();
        let returns: Vec<_> = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Return { .. }))
            .map(|(id, node)| {
                (
                    node.span(),
                    facts.expression_falls_through(UnitExpressionId::new(unit.id(), id)),
                )
            })
            .collect();
        assert_eq!(facts.expression_falls_through(first), Some(true));
        assert_eq!(loops.len(), if terminating { 2 } else { 1 });
        assert!(returns.iter().all(|(_, flow)| *flow == Some(false)));
        let plans = owned
            .ownership()
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == call)
            .collect::<Vec<_>>();
        // The existing ordinary producer describes both syntactic operands and a root;
        // it does not issue the runtime-only termination proof for the literal loop.
        assert_eq!(facts.expression_falls_through(conditional), Some(true));
        assert!(loops.iter().all(|(_, flow)| *flow == Some(true)));
        assert!(facts.construction(call).is_some() && facts.call(call).is_none());
        let [plan] = plans.as_slice() else {
            panic!("one existing construction plan");
        };
        assert!(plan.terminating_operand().is_none());
        assert!(plan.root_obligation().is_some());
        assert_eq!(plan.deliveries().len(), 2);

        // This deliberately records the current conservative parent completion. The
        // desired never-allocates claim is a separate RED, not a new admission rule.
        assert_eq!(facts.expression_falls_through(call), Some(true));
    }
}

#[test]
fn terminating_constructor_prefix_retains_both_native_exits() {
    use crate::ssa::model::{Operation, TerminatorKind};

    let source = TERMINATING_CONSTRUCTOR_PREFIX;
    let continuing = source.replace("while (true) { return }\n                2", "2");
    assert_ne!(continuing, source);
    for (text, terminating) in [(source, true), (&continuing, false)] {
        let (lowered, refined) = route(text);
        let program = lowered.expect("retained lowering still accepts both valid programs");
        crate::ssa::verify::verify_program(&program).unwrap();
        let [module] = program.modules.as_slice() else {
            panic!("one source module");
        };
        let function = module
            .functions
            .iter()
            .find(|f| f.name.starts_with("koven.p.f.d"))
            .unwrap();
        let prefix = module
            .functions
            .iter()
            .find(|f| f.name.starts_with("koven.p.firstArgument.d"))
            .unwrap();
        assert_eq!(function.instructions.iter().filter(|op| matches!(op.operation, Operation::DirectCall { callee, .. } if callee == prefix.id)).count(), 1);
        if terminating {
            assert!(
                !refined,
                "preserve the verified original Compatibility boundary"
            );
            assert!(!function.instructions.iter().any(|op| matches!(
                op.operation,
                Operation::HeapAllocate { .. }
                    | Operation::HeapFieldRead { .. }
                    | Operation::Drop { .. }
            )));
            assert_eq!(function.blocks.iter().filter_map(|b| b.terminator.as_ref()).filter(|t| matches!(&t.kind, TerminatorKind::Return { values } if values.is_empty())).count(), 2);
        } else {
            // A continuing alternative must not acquire a never-allocates certificate.
            assert!(
                function
                    .instructions
                    .iter()
                    .any(|op| matches!(op.operation, Operation::HeapAllocate { .. }))
            );
        }
        crate::llvm::render_verified_program(&program).unwrap();
    }
    let executable = crate::native_tests::emit_link_and_run(
        "terminated-constructor-compatibility.ko",
        &format!("{source}\nfun main(): Unit {{ f(true); f(false) }}"),
        "main",
    );
    assert!(executable.status.success(), "{executable:?}");
    assert_eq!(executable.stdout, b"first argument\nfirst argument\n");
}

#[test]
fn bare_loop_without_break_cannot_lend_destructuring_route() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(own pair: Pair): Unit {
        while (false) {}
        for (item in listOf("kept")) { println(item) }
        loop {}
        val (second, first) = pair
    }
    "#;
    let (result, routed) = route(source);
    assert!(
        !routed,
        "the local destructuring is after a loop without an exit"
    );
    crate::ssa::verify::verify_program(&result.expect("legacy route still lowers"))
        .expect("verified program");
}

#[test]
fn named_sequential_for_shares_the_qualified_while_route() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(): Int {
        val (second, first) = Pair(20, 22)
        val xs = listOf(Pair(20, 22))
        while (false) {}
        for ((value, _) in xs) { if (value == first) { println("loop") } }
        return first
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("named provider lowers"))
        .expect("verified program");
}

#[test]
fn qualified_sequential_for_invalidates_reassigned_nullable_view() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (false) {}
            for (item in listOf("kept")) { outer = "second" }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("sequential loop lowers"))
        .expect("verified program");
}

#[test]
fn local_destructuring_qualifies_while_root_reassignment_without_a_for() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(flag: Boolean): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (flag && first == 20) {
                outer = "second"
                break
            }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("standalone while lowers"))
        .expect("verified program");
}

#[test]
fn numeric_for_routes_refine_without_using_local_destructuring_to_retire_outer_proof() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(own range: IntRange): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (false) {}
            for (item in range) { outer = "second" }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(
        routed,
        "numeric for has no legacy proof policy after SPEC-0388"
    );
    crate::ssa::verify::verify_program(&result.expect("numeric for lowers in refine"))
        .expect("verified program");
}

#[test]
fn nested_while_does_not_veto_enclosing_named_iteration() {
    for body in [
        "println(\"nested\")",
        "while (false) {}; println(\"nested\")",
    ] {
        let source = format!(
            r#"fun f(): Unit {{
                val nested: () -> Unit = {{ {body} }}
                nested()
                val xs = listOf("kept")
                for (item in xs) {{ println(item) }}
            }}"#
        );
        let (result, routed) = route(&source);
        assert!(
            routed,
            "outer route changed with nested body {body}: {:?}",
            result.as_ref().err()
        );
        crate::ssa::verify::verify_program(&result.expect("named iteration lowers"))
            .expect("the callable and iteration both verify");
    }
}

#[test]
fn while_in_the_same_callable_keeps_its_named_iteration_gate() {
    let source = r#"
        fun f(): Unit {
            while (false) {}
            val xs = listOf("kept")
            for (item in xs) { println(item) }
        }
    "#;
    let (result, routed) = route(source);
    assert!(
        !routed,
        "a while in f retains the existing route restriction"
    );
    crate::ssa::verify::verify_program(&result.expect("ordinary Unit lowers"))
        .expect("verified program");
}

pub(in crate::ssa) mod control_completion {
    //! Ordinary completion must agree with emitted CFG exits.
    use super::super::{render_program, route};

    #[test]
    fn named_nothing_call_uses_refine_and_stops_at_the_call() {
        for callee in [
            r#"fun stop(): Nothing = error("stop")"#,
            "fun stop(): Nothing = stop()",
            "fun stop(): Nothing { loop { continue } }",
        ] {
            let (result, routed) = route(&format!("{callee}\nfun f(): Int = stop()"));
            assert!(routed, "a direct Nothing result must qualify for refine");
            let program = result.unwrap();
            let rendered = render_program(&program);
            assert!(rendered.contains("noreturn"));
            assert!(rendered.contains("unreachable"));
        }
    }

    #[test]
    fn exhaustive_when_inside_loop_keeps_the_suffix_unreachable() {
        for (declarations, subject, arms) in [
            ("", "Boolean", "(true) -> continue; false -> continue"),
            (
                "",
                "Boolean",
                "true -> continue; false -> continue; else -> break",
            ),
            (
                "enum class Flag { A, B }",
                "Flag",
                "Flag.A -> continue; Flag.B -> continue; else -> break",
            ),
        ] {
            let source = format!(
                "{declarations}\nfun f(flag: {subject}): Int {{ loop {{ when (flag) {{ {arms} }}; break }} }}"
            );
            let (result, routed) = route(&source);
            let program = result.unwrap_or_else(|error| panic!("{arms}: {error:?}"));
            assert!(routed, "{arms}");
            crate::llvm::render_verified_program(&program).expect("loop lowers to verified LLVM");
        }
    }

    #[test]
    fn while_condition_without_a_false_edge_keeps_the_suffix_unreachable() {
        let (program, routed) = route(
            "fun unreachable(): Unit {}
         fun f(flag: Boolean): Int {
             loop {
                 when { flag -> {}; else -> {} }
                 while (flag || ((return 7) == 0)) {}
                 unreachable()
                 break
             }
             unreachable()
         }",
        );
        assert!(
            routed,
            "refine must skip the dead calls in both loop suffixes"
        );
        crate::llvm::render_verified_program(&program.unwrap()).expect("verified while control");
    }
}

#[path = "temporary_sequential_for/structural_constructor_sibling.rs"]
mod structural_constructor_sibling;
