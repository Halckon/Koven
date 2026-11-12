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

#[test]
fn terminating_constructor_prefix_lowers_on_the_retained_route() {
    let source = r#"
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
    let (result, routed) = route(source);
    assert!(
        !routed,
        "the scalar-field construction stays on retained Unit"
    );
    crate::ssa::verify::verify_program(&result.expect("the abandoned constructor lowers"))
        .expect("both return edges and the evaluated prefix verify");
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

pub(in crate::ssa) mod temporary_alias {
//! Ordinary Borrow temporaries: real producer identity and checked edge columns.
use crate::ssa::lowering_entry_contract_tests::*;
use crate::test_support::diagnostics::diagnostic;

#[test]
fn paired_borrowed_group_temporaries_keep_distinct_current_columns() {
    let text = "package pair\nclass Node()\n\
                fun observe(first: Node, second: Node, own value: Int): Int = value\n\
                fun entry(own number: Int): Int = observe(((Node())), Node(), number + 1)";
    // Borrow preserves the outer temporary origin. Production lower(Group) registers
    // each temporary Group on return; this observer checks the resulting SSA, not a
    // private lowerer map or a key count inferred only from the parentheses.
    let observations = std::cell::RefCell::new(Vec::new());
    let observe = |sources: &SourceMap, program: &Program, entry| {
        // Only this read-only query is caught. The existing bridge still runs all
        // real analysis/lowering/SSA/LLVM gates directly, with its original hook.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_borrowed_temporary_columns(sources, program, entry);
        }));
        observations.borrow_mut().push(result);
    };
    let first = paired_in_order_observed(text, false, false, observe);
    let second = paired_in_order_observed(text, false, true, observe);
    let observations = observations.into_inner();
    assert_eq!(
        observations.len(),
        6,
        "two rooted unit and four unit observations"
    );
    let mut failures = Vec::new();
    for (index, result) in observations.into_iter().enumerate() {
        match result {
            Ok(()) => diagnostic!("temporary alias observation {}: ok", index + 1),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string query panic payload");
                let failure = format!("temporary alias observation {}: {message}", index + 1);
                diagnostic!("{failure}");
                failures.push(failure);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(first, second);
}

fn assert_borrowed_temporary_columns(sources: &SourceMap, program: &Program, entry: FunctionId) {
    use crate::ssa::model::{CheckedArithmeticOperator, EntityType, LoanKind};

    let module = &program.modules[0];
    let function = module.function(entry).unwrap();
    let checked = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 1);
    let checked = checked[0];
    assert!(matches!(
        checked.operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Add,
            ..
        }
    ));
    assert_eq!(sources.slice(checked.origin.span()).unwrap(), "number + 1");
    let [EntityId::Value(result), EntityId::Value(failed)] = checked.results.as_slice() else {
        panic!("checked arithmetic produces its scalar and failure flag");
    };
    let before = function.block(checked.block).unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &before.terminator.as_ref().unwrap().kind
    else {
        panic!("one checked operation branches to failure and success");
    };
    assert_eq!(condition, failed);
    assert_eq!(when_true.arguments, when_false.arguments);
    let failure = function.block(when_true.target).unwrap();
    let success = function.block(when_false.target).unwrap();
    assert!(matches!(
        failure.terminator.as_ref().unwrap().kind,
        TerminatorKind::Abort
    ));
    assert!(
        failure.instructions.is_empty(),
        "Abort does not unwind either owner"
    );
    assert_eq!(
        when_false.arguments.len(),
        4,
        "two owner columns and two loan columns"
    );
    assert_eq!(success.parameters.len(), 4);
    assert_eq!(failure.parameters.len(), 4);
    let allocations = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
        .collect::<Vec<_>>();
    assert_eq!(allocations.len(), 2, "each real Node constructor runs once");
    assert_ne!(allocations[0].origin.span(), allocations[1].origin.span());
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::AggregateConstruct { .. })
            })
            .count(),
        2,
        "each allocation has its own empty payload"
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| { matches!(instruction.operation, Operation::RootPlace { .. }) })
            .count(),
        2
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::BorrowBegin { .. })
            })
            .count(),
        2
    );
    let pre_position = |id| {
        before
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    let current = |source| {
        let columns = when_false
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (*argument == source).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            columns.len(),
            1,
            "every exact source has one physical column"
        );
        let index = columns[0];
        let parameter = success.parameters[index];
        assert_eq!(
            function.entity(parameter).unwrap().definition,
            Definition::BlockParameter {
                block: success.id,
                index
            }
        );
        assert_eq!(
            function.entity(parameter).unwrap().ty,
            function.entity(source).unwrap().ty
        );
        assert_eq!(
            function.entity(failure.parameters[index]).unwrap().ty,
            function.entity(source).unwrap().ty
        );
        parameter
    };
    let mut current_owners = Vec::new();
    let mut current_loans = Vec::new();
    for allocation in allocations {
        assert_eq!(allocation.block, before.id);
        assert_eq!(sources.slice(allocation.origin.span()).unwrap(), "Node()");
        let Operation::HeapAllocate {
            owner: owner_type,
            payload,
        } = allocation.operation
        else {
            unreachable!()
        };
        let [EntityId::Value(owner)] = allocation.results.as_slice() else {
            panic!("one heap owner per constructor");
        };
        assert_eq!(
            function.entity(EntityId::Value(*owner)).unwrap().ty,
            EntityType::Value(owner_type)
        );
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function
            .entity(EntityId::Value(payload))
            .unwrap()
            .definition
        else {
            panic!("the allocated payload comes from its real constructor");
        };
        let payload = function.instruction(instruction).unwrap();
        assert!(
            matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty())
        );
        assert_eq!(payload.origin.span(), allocation.origin.span());
        assert!(pre_position(payload.id) < pre_position(allocation.id));
        let roots = function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::RootPlace { owner: actual } if actual == *owner)
        }).collect::<Vec<_>>();
        assert_eq!(roots.len(), 1);
        let root = roots[0];
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("the borrowed owner has one actual root place");
        };
        let borrows = function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::BorrowBegin { place: actual, kind: LoanKind::Shared } if actual == *place)
        }).collect::<Vec<_>>();
        assert_eq!(borrows.len(), 1);
        let borrow = borrows[0];
        let [EntityId::Loan(loan)] = borrow.results.as_slice() else {
            panic!("each temporary has its own shared loan");
        };
        assert_eq!(
            function.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner_type
            }
        );
        assert!(pre_position(allocation.id) < pre_position(root.id));
        assert!(pre_position(root.id) < pre_position(borrow.id));
        assert!(pre_position(borrow.id) < pre_position(checked.id));
        current_owners.push(current(EntityId::Value(*owner)));
        current_loans.push(current(EntityId::Loan(*loan)));
    }
    assert_ne!(current_owners[0], current_owners[1]);
    assert_eq!(
        function.entity(current_owners[0]).unwrap().ty,
        function.entity(current_owners[1]).unwrap().ty,
        "same type does not mean same owner"
    );
    assert_ne!(current_loans[0], current_loans[1]);
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    let call = calls[0];
    assert_eq!(call.block, success.id);
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    let crate::ssa::model::Origin::Source(callee_span) = &module.function(*callee).unwrap().origin
    else {
        panic!("observe must be the real source declaration");
    };
    let declaration = "fun observe(first: Node, second: Node, own value: Int): Int = value";
    let source = sources.source_text(callee_span.source_id()).unwrap();
    let start = source
        .find(declaration)
        .expect("the exact observe declaration exists");
    assert_eq!(
        source_key(sources, *callee_span),
        SourceKey("pair/entry.ko".to_owned(), start, start + declaration.len())
    );
    assert_eq!(sources.slice(*callee_span).unwrap(), declaration);
    assert!(receiver.is_none());
    assert_eq!(
        arguments,
        &[current_loans[0], current_loans[1], EntityId::Value(*result)]
    );
    let position = |id| {
        success
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    let ends = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::BorrowEnd { loan } => Some((position(instruction.id), EntityId::Loan(loan))),
            _ => None,
        })
        .collect::<Vec<_>>();
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some((position(instruction.id), EntityId::Value(owner))),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2);
    assert_eq!(drops.len(), 2);
    for (owner, loan) in current_owners.iter().zip(&current_loans) {
        let matching_ends = ends
            .iter()
            .filter(|(_, actual)| actual == loan)
            .collect::<Vec<_>>();
        let matching_drops = drops
            .iter()
            .filter(|(_, actual)| actual == owner)
            .collect::<Vec<_>>();
        assert_eq!(matching_ends.len(), 1, "end the current loan exactly once");
        assert_eq!(
            matching_drops.len(),
            1,
            "drop the current owner exactly once"
        );
        assert!(position(call.id) < matching_ends[0].0);
        assert!(matching_ends[0].0 < matching_drops[0].0);
    }
    assert!(
        ends.iter().map(|(index, _)| index).max().unwrap()
            < drops.iter().map(|(index, _)| index).min().unwrap()
    );
    let TerminatorKind::Return { values } = &success.terminator.as_ref().unwrap().kind else {
        panic!("the normal path returns after its borrowed temporaries are cleaned up");
    };
    assert_eq!(values.len(), 1);
    assert_eq!(call.results.as_slice(), &[EntityId::Value(values[0])]);
}
}
