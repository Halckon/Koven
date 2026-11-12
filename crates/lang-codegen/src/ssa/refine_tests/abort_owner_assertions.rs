//! Test-only oracle for the consume-then-abort fixtures. Owner identity crosses a
//! CFG edge by argument position, never by equality with a successor's fresh ID.
use std::collections::BTreeSet;

use crate::ssa::model::{Block, Edge, EntityId, Function, Operation, TerminatorKind};

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().expect("terminated fixture").kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. }
        | TerminatorKind::BorrowReturn { .. }
        | TerminatorKind::RangeReturn { .. }
        | TerminatorKind::Abort
        | TerminatorKind::Unreachable => Vec::new(),
    }
}

fn owner_lineage(function: &Function, owner: EntityId) -> BTreeSet<EntityId> {
    let mut lineage = BTreeSet::from([owner]);
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for edge in edges(block) {
                let target = function.block(edge.target).expect("valid edge target");
                // NullableBranch supplies the non-null proof as an extra final
                // parameter, not an edge argument. Only paired positions transfer.
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    if lineage.contains(argument) {
                        changed |= lineage.insert(*parameter);
                    }
                }
            }
        }
        if !changed {
            return lineage;
        }
    }
}

pub(super) fn check_consumed_call_owner(
    function: &Function,
    expected_op_check: impl Fn(&Operation) -> bool,
    consumed_parameter_index: usize,
) -> Result<usize, String> {
    let owner = function.blocks[0].parameters[consumed_parameter_index];
    assert!(matches!(owner, EntityId::Value(_)), "owned Value parameter");
    let lineage = owner_lineage(function, owner);
    let calls: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| expected_op_check(&instruction.operation))
        .filter(|instruction| {
            let arguments = match &instruction.operation {
                Operation::DirectCall { arguments, .. }
                | Operation::CallableInvoke { arguments, .. }
                | Operation::CallableInvokeView { arguments, .. }
                | Operation::BorrowCall { arguments, .. }
                | Operation::RangeCall { arguments, .. } => arguments,
                _ => return false,
            };
            arguments.iter().any(|argument| lineage.contains(argument))
        })
        .collect();
    if calls.len() != 1 {
        return Err(format!(
            "expected exactly one matching call consuming parameter {consumed_parameter_index}, found {}",
            calls.len()
        ));
    }

    // Only successors of the consuming call are forbidden from carrying this
    // owner. An abort before the call may legitimately receive it. These bounded
    // fixtures consume the input once; this is not a general ownership verifier.
    let mut pending = vec![calls[0].block];
    let mut visited = BTreeSet::new();
    let mut abort_count = 0;
    while let Some(block_id) = pending.pop() {
        if !visited.insert(block_id) {
            continue;
        }
        let block = function.block(block_id).expect("valid block");
        if matches!(
            block.terminator.as_ref().map(|terminator| &terminator.kind),
            Some(TerminatorKind::Abort)
        ) {
            abort_count += 1;
        }
        for edge in edges(block) {
            let target = function.block(edge.target).expect("valid edge target");
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                if lineage.contains(argument) {
                    return Err(format!(
                        "edge {block_id:?} -> {:?} forwards consumed owner {argument:?} as {parameter:?}",
                        edge.target
                    ));
                }
            }
            pending.push(edge.target);
        }
    }
    Ok(abort_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::{
        model::{AbortReason, EntityType, FunctionId, Origin, Program, SsaTypeKind, ValueId},
        verify::verify_program,
    };
    use lang_frontend::source::SourceMap;

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("fixture requires a value");
        };
        value
    }

    fn fixture(forward_consumed: bool, consume_target: bool) -> (Program, FunctionId) {
        let mut sources = SourceMap::new();
        let source = sources.add_source("owner-oracle.ko", "fun test").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 3).unwrap());
        let mut program = Program::default();
        let module_id = program.add_module("oracle");
        let module = program.module_mut(module_id).unwrap();
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let string = module.intern_type(SsaTypeKind::StringOwner);
        let owned = EntityType::Value(string);
        let callee_id = module
            .add_function("consume", vec![], origin.clone())
            .unwrap();
        let callee = module.function_mut(callee_id).unwrap();
        let entry = callee.add_block(vec![owned], origin.clone()).unwrap();
        let parameter = value(callee.block(entry).unwrap().parameters[0]);
        callee
            .append_instruction(
                entry,
                Operation::Drop { owner: parameter },
                vec![],
                origin.clone(),
            )
            .unwrap();
        callee
            .set_terminator(
                entry,
                TerminatorKind::Return { values: vec![] },
                origin.clone(),
            )
            .unwrap();

        let function_id = module.add_function("test", vec![], origin.clone()).unwrap();
        let function = module.function_mut(function_id).unwrap();
        let entry = function
            .add_block(
                vec![EntityType::Value(boolean), owned, owned],
                origin.clone(),
            )
            .unwrap();
        let call = function
            .add_block(vec![owned, owned], origin.clone())
            .unwrap();
        let before_call_abort = function
            .add_block(vec![owned, owned], origin.clone())
            .unwrap();
        let after_types = if forward_consumed {
            vec![owned, owned]
        } else {
            vec![owned]
        };
        let middle = function
            .add_block(after_types.clone(), origin.clone())
            .unwrap();
        let abort_a = function
            .add_block(after_types.clone(), origin.clone())
            .unwrap();
        let abort_b = function.add_block(after_types, origin.clone()).unwrap();
        let parameters = function.block(entry).unwrap().parameters.clone();
        let condition = value(parameters[0]);
        function
            .set_terminator(
                entry,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: call,
                        arguments: parameters[1..].to_vec(),
                    },
                    when_false: Edge {
                        target: before_call_abort,
                        arguments: parameters[1..].to_vec(),
                    },
                },
                origin.clone(),
            )
            .unwrap();
        let parameters = function.block(call).unwrap().parameters.clone();
        function
            .append_instruction(
                call,
                Operation::DirectCall {
                    callee: callee_id,
                    receiver: None,
                    arguments: vec![parameters[usize::from(!consume_target)]],
                },
                vec![],
                origin.clone(),
            )
            .unwrap();
        // Reorder the outgoing slots as well as renaming every successor value.
        let mut arguments = vec![parameters[1]];
        if forward_consumed {
            arguments.push(parameters[0]);
        }
        function
            .set_terminator(
                call,
                TerminatorKind::Branch(Edge {
                    target: middle,
                    arguments,
                }),
                origin.clone(),
            )
            .unwrap();
        let arguments = function.block(middle).unwrap().parameters.clone();
        function
            .set_terminator(
                middle,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: abort_a,
                        arguments: arguments.clone(),
                    },
                    when_false: Edge {
                        target: abort_b,
                        arguments,
                    },
                },
                origin.clone(),
            )
            .unwrap();
        for abort in [before_call_abort, abort_a, abort_b] {
            function
                .set_abort_terminator(abort, AbortReason::Text("test".into()), origin.clone())
                .unwrap();
        }
        (program, function_id)
    }

    fn direct_call(operation: &Operation) -> bool {
        matches!(operation, Operation::DirectCall { .. })
    }

    #[test]
    fn accepts_live_owners_and_abort_before_consumption() {
        let (program, function_id) = fixture(false, true);
        verify_program(&program).expect("clean fixture must satisfy the full SSA contract");
        let function = program
            .module(function_id.module())
            .unwrap()
            .function(function_id)
            .unwrap();
        assert_eq!(check_consumed_call_owner(function, direct_call, 1), Ok(2));
    }

    #[test]
    fn rejects_renamed_consumed_owner_that_old_assertion_accepts() {
        let (program, function_id) = fixture(true, true);
        let function = program
            .module(function_id.module())
            .unwrap()
            .function(function_id)
            .unwrap();
        let entry_owner = function.blocks[0].parameters[1];
        // Pre-fix negative control: the old assertion accepts every abort even
        // when the consumed owner crosses two more edges under fresh IDs.
        for block in &function.blocks {
            if matches!(
                block.terminator.as_ref().map(|t| &t.kind),
                Some(TerminatorKind::Abort)
            ) {
                assert!(!block.parameters.contains(&entry_owner));
            }
        }
        let error = check_consumed_call_owner(function, direct_call, 1).unwrap_err();
        assert!(error.contains("forwards consumed owner"), "{error}");
    }

    #[test]
    fn rejects_matching_operation_that_consumes_another_owner() {
        let (program, function_id) = fixture(false, false);
        let function = program
            .module(function_id.module())
            .unwrap()
            .function(function_id)
            .unwrap();
        let error = check_consumed_call_owner(function, direct_call, 1).unwrap_err();
        assert!(error.contains("found 0"), "{error}");
    }

    #[test]
    fn rejects_missing_expected_operation() {
        let (program, function_id) = fixture(false, true);
        let function = program
            .module(function_id.module())
            .unwrap()
            .function(function_id)
            .unwrap();
        let error =
            check_consumed_call_owner(function, |op| matches!(op, Operation::BorrowCall { .. }), 1)
                .unwrap_err();
        assert!(error.contains("found 0"), "{error}");
    }
}

pub(in crate::ssa) mod operand_exit_tests {
//! Native cleanup and reachability witnesses for an exit inside a comparison operand.
use super::super::counted;

#[test]
fn comparison_operand_return_releases_pending_string_and_named_owner_once() {
    let source = r#"fun left(): String { println("left string"); return "temporary".clone() }
fun measure(value: String): Int { println(value); return 5 }
fun finish(): Int {
    val owner = "named".clone()
    left() == (return measure(owner))
}
fun main(): Unit {
    if (finish() == 5) { println("returned") }
    else { error("wrong return") }
}
"#;
    counted(source, 2, b"left string\nnamed\nreturned\n");
}

#[test]
fn comparison_operand_return_cleans_the_same_left_owner_on_each_return_edge() {
    let source = r#"fun left(): String { println("left string"); return "temporary".clone() }
fun finish(flag: Boolean): Int {
    val live = "two exits".clone()
    println(live)
    left() == (if (flag) { return 5 } else { return 6 })
}
fun nested(flag: Boolean): Int {
    val live = "nested exit".clone()
    println(live)
    left() == (return if (flag || ((return 7) == 0)) { return 8 } else { 9 })
}
fun main(): Unit {
    if (finish(true) == 5 && finish(false) == 6 && nested(false) == 7 && nested(true) == 8) {
        println("returned")
    } else { error("wrong return") }
}
"#;
    counted(
        source,
        8,
        b"two exits\nleft string\ntwo exits\nleft string\nnested exit\nleft string\nnested exit\nleft string\nreturned\n",
    );
}

#[test]
fn comparison_operand_return_does_not_require_drops_on_unreachable_arm() {
    let source = r#"fun finish(flag: Boolean): Int {
    val live = "condition".clone()
    println(live)
    if (flag || ((return 6) == 0)) { return 7 }
    else { val dead = "dead".clone(); println(dead); return 8 }
}
fun choose(flag: Boolean): Int {
    return when (flag) {
        ((return 9) == 0) -> 0
        else -> { val dead = "dead candidate".clone(); println(dead); 0 }
    }
}
fun chooseWithoutSubject(): Int {
    return when {
        ((return 11) == 0) -> 0
        else -> { val dead = "dead alternative".clone(); println(dead); 0 }
    }
}
fun exitsInHeader(): Int {
    val live = "header".clone()
    println(live)
    while ((return 10) == 0) {}
    val dead = "dead tail".clone()
    println(dead)
    return 0
}
fun main(): Unit {
    val live = "returned".clone()
    if (finish(true) == 7 && finish(false) == 6 && choose(true) == 9
        && exitsInHeader() == 10 && chooseWithoutSubject() == 11) {
        println(live)
    }
    else { error("wrong return") }
}
"#;
    counted(source, 4, b"condition\ncondition\nheader\nreturned\n");
}

#[test]
fn comparison_operand_return_restores_pending_call_loan_on_the_skip_edge() {
    let source = r#"fun use(text: String, flag: Boolean, value: Int): Int = error("unreachable call")
fun measure(text: String): Int { println(text); return 5 }
fun finish(flag: Boolean): Int {
    val owner = "pending".clone()
    return use(owner, flag || ((return measure(owner)) == 0), (return measure(owner)))
}
fun main(): Unit {
    if (finish(true) == 5 && finish(false) == 5) { println("returned") }
    else { error("wrong return") }
}
"#;
    counted(source, 2, b"pending\npending\nreturned\n");
}
}
