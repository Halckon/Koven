//! SPEC-0264: direct field loans retain the original parent through calls and exits.
use super::{
    LoweringErrorKind,
    model::Operation,
    unit_field_replace_tests::{link_and_run, lower, run_native},
};

#[test]
fn unit_field_borrow_native_reads_then_replaces_without_copying_owner() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var text: String, val marker: Int)
            fun inspect(text: String): Unit { println(text) }
            fun entry(): Unit {
                val holder = Holder("old" + "value", 7)
                inspect(holder.text)
                inspect((holder.text))
                val old = replace(&holder.text, "new" + "value")
                println(old)
                inspect(holder.text)
                if (holder.marker == 7) { println("parent-live") }
                println(holder.text)
                println("after-parent")
            }
            "#,
            constants,
            3,
            b"oldvalue\noldvalue\noldvalue\nnewvalue\nparent-live\nnewvalue\nafter-parent\n",
        );
    }
}

#[test]
fn unit_field_borrow_nested_call_ends_before_later_argument_replace() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var text: String)
            fun inspect(text: String): Int {
                println(text)
                return 7
            }
            fun finish(own marker: Int, text: String): Unit {
                if (marker == 7) { println(text) }
            }
            fun entry(): Unit {
                val holder = Holder("old" + "field")
                finish(inspect(holder.text), replace(&holder.text, "new" + "field"))
                println(holder.text)
                println("after")
            }
            "#,
            constants,
            3,
            b"oldfield\noldfield\nnewfield\nafter\n",
        );
    }
}

#[test]
fn unit_field_borrow_later_cfg_argument_and_exits_preserve_loans() {
    for exit in ["return", "break", "continue"] {
        let source = format!(
            r#"
            class Holder(var text: String)
            fun inspect(text: String, own marker: Int): Unit {{
                println(text)
                if (marker == 7) {{ println("callee") }}
            }}
            fun test(flag: Boolean): Unit {{
                val holder = Holder("field" + "value")
                var once = true
                loop {{
                    println("loop-head")
                    if (!once) {{ break }}
                    once = false
                    inspect(holder.text, if (flag) {{ {exit} }} else {{ 7 }})
                    println("after-call")
                    break
                }}
                println(holder.text)
                println("after-loop")
            }}
            fun entry(): Unit {{
                test(true)
                println("caller-true")
                test(false)
                println("caller-false")
            }}
            "#
        );
        assert_eq!(
            lower(&source, false)
                .err()
                .expect("basic control-flow boundary")
                .kind,
            LoweringErrorKind::UnsupportedNode,
            "{exit}"
        );
        let prefix = match exit {
            "return" => "loop-head\n",
            "break" => "loop-head\nfieldvalue\nafter-loop\n",
            "continue" => "loop-head\nloop-head\nfieldvalue\nafter-loop\n",
            _ => unreachable!(),
        };
        let stdout = format!(
            "{prefix}caller-true\nloop-head\nfieldvalue\ncallee\nafter-call\nfieldvalue\nafter-loop\ncaller-false\n"
        );
        run_native(&source, true, 4, stdout.as_bytes());
    }
}

#[test]
fn unit_field_borrow_native_abort_keeps_parent_and_field_without_unwind() {
    for constants in [false, true] {
        let (program, entry) = lower(
            r#"
            class Holder(val text: String)
            fun inspect(text: String, own marker: Int): Unit { println(text) }
            fun entry(): Unit {
                val holder = Holder("field" + "value")
                inspect(holder.text, error("stop"))
                println("unexpected-caller")
            }
            "#,
            constants,
        )
        .expect("Abort retains only evaluated field loan prefix");
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.id == entry)
            .unwrap();
        let prefix = function
            .instructions
            .iter()
            .position(|instruction| matches!(instruction.operation, Operation::FieldPlace { .. }))
            .unwrap();
        assert!(
            function.instructions[prefix..]
                .iter()
                .all(|instruction| !matches!(
                    instruction.operation,
                    Operation::Drop { .. } | Operation::BorrowEnd { .. }
                ))
        );
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .unwrap()
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@unexpected_free(")
            .replace("@abort(", "@expected_abort(");
        link_and_run(
            &llvm,
            r#"
#include <stdlib.h>
#include <assert.h>
#include <stdio.h>
static void *live[2];
static int allocated;
void *counted_malloc(size_t n) { assert(n && allocated < 2); void *p = malloc(n); assert(p); live[allocated++] = p; return p; }
void unexpected_free(void *p) { (void)p; _Exit(41); }
void expected_abort(void) { assert(allocated == 2 && live[0] && live[1] && live[0] != live[1]); assert(fflush(stdout) == 0); _Exit(0); }
__attribute__((destructor)) static void unexpected_return(void) { _Exit(42); }
"#,
            b"",
        );
    }
}

#[test]
fn unit_field_borrow_keeps_nested_generic_inline_and_owned_parameter_boundaries() {
    for source in [
        "class Inner(val text: String)\nclass Outer(val inner: Inner)\nfun entry(): Unit {\nval outer = Outer(Inner(\"text\"))\nprintln(outer.inner.text)\n}",
        "class Holder<T>(val text: T)\nfun entry(): Unit {\nval holder = Holder(\"text\")\nprintln(holder.text)\n}",
        "value class Holder(val text: String)\nfun entry(): Unit {\nval holder = Holder(\"text\")\nprintln(holder.text)\n}",
        "class Holder(val text: String)\nfun inspect(own holder: Holder): Unit { println(holder.text) }\nfun entry(): Unit { inspect(Holder(\"text\")) }",
    ] {
        for constants in [false, true] {
            assert_eq!(
                lower(source, constants)
                    .err()
                    .expect("outside owned local direct class field boundary")
                    .kind,
                LoweringErrorKind::UnsupportedNode,
                "{source}"
            );
        }
    }
}

#[test]
fn unit_field_borrow_cross_source_same_names_keep_field_identity_and_order() {
    use super::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(
        &mut sources,
        "p/model.ko",
        "package p\nclass Holder(val text: String, val marker: Int)\nfun make(): Holder = Holder(\"left\", 1)",
    );
    let (right_id, right) = parsed(
        &mut sources,
        "q/model.ko",
        "package q\nclass Holder(val marker: Int, val text: String)\nfun make(): Holder = Holder(2, \"right\")",
    );
    let (entry_id, file) = parsed(
        &mut sources,
        "test/entry.ko",
        r#"
        package test
        fun inspect(text: String): Unit { println(text) }
        fun entry(): Unit {
            val left = p.make()
            val right = q.make()
            inspect(left.text)
            inspect(right.text)
            println("after")
        }
    "#,
    );
    let inputs = [
        SourceUnitInput::new("root", "p/model.ko", left_id, &left),
        SourceUnitInput::new("root", "q/model.ko", right_id, &right),
        SourceUnitInput::new("root", "test/entry.ko", entry_id, &file),
    ];
    let reversed = [inputs[2], inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let mut rendered = Vec::new();
    for inputs in [&inputs, &reversed] {
        let (names, typed, owned) = analyze(&sources, inputs, &name_environment, &environment);
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .unwrap();
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.id == entry)
            .unwrap();
        let fields = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::FieldPlace { field, .. } => Some(field),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            [0, 1],
            "same-spelled fields use their declaration's layout"
        );
        assert!(
            function.instructions.iter().all(|instruction| !matches!(
                instruction.operation,
                Operation::Read { .. } | Operation::Copy { .. }
            )),
            "field loans never materialize owned copies"
        );
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        link_and_run(&llvm, "", b"left\nright\nafter\n");
        rendered.push(super::render::render_program(&program));
    }
    assert_eq!(rendered[0], rendered[1]);
}

#[test]
fn unit_field_borrow_native_frees_old_then_current_field_then_parent() {
    for constants in [false, true] {
        let (program, entry) = lower(
            r#"
            class Holder(var text: String)
            fun entry(): Unit {
                val holder = Holder("old" + "field")
                println(holder.text)
                val old = replace(&holder.text, "new" + "field")
                println(old)
                println(holder.text)
                println("after")
            }
        "#,
            constants,
        )
        .unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .unwrap()
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        link_and_run(
            &llvm,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[3];
static int allocated, released;
static const int order[3] = {0, 2, 1};
void *counted_malloc(size_t n) {
    assert(n && allocated < 3); void *p = malloc(n); assert(p);
    for (int i = 0; i < allocated; ++i) assert(live[i] != p);
    live[allocated++] = p; return p;
}
void counted_free(void *p) {
    assert(p && released < 3);
    int slot = order[released++]; assert(live[slot] == p); live[slot] = 0; free(p);
}
__attribute__((destructor)) static void verify(void) {
    assert(allocated == 3 && released == 3);
    for (int i = 0; i < 3; ++i) assert(!live[i]);
}
"#,
            b"oldfield\noldfield\nnewfield\nafter\n",
        );
    }
}

#[test]
fn unit_field_borrow_parameter_projects_child_without_consuming_parent() {
    let source = "class Holder(val text: String)\nfun inspect(holder: Holder): Unit { println(holder.text) }\nfun entry(): Unit { inspect(Holder(\"text\")) }";
    for constants in [false, true] {
        let (program, entry) =
            lower(source, constants).expect("Borrow parameter field is supported");
        let inspect = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("inspect"))
            .unwrap();
        let parent = inspect
            .block(inspect.entry_block().unwrap())
            .unwrap()
            .parameters[0];
        let projections = inspect
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::SharedHeapFieldLoan { base, field } = instruction.operation {
                    Some((
                        super::model::EntityId::Loan(base),
                        field,
                        instruction.results[0],
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(base, field, child)] = projections.as_slice() else {
            panic!("one field child loan")
        };
        assert_eq!((*base, *field), (parent, 0));
        assert!(
            inspect
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation,
            Operation::BorrowEnd { loan } if super::model::EntityId::Loan(loan) == *child))
        );
        assert!(
            inspect
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation,
            Operation::BorrowEnd { loan } if super::model::EntityId::Loan(loan) == parent))
        );
        assert!(
            inspect
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation,
            Operation::Read { source: super::model::PlaceAccess::Loan(loan) }
                if super::model::EntityId::Loan(loan) == parent))
        );
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("field loan LLVM");
        link_and_run(&llvm, "", b"text\n");
    }
}

#[test]
fn unit_field_borrow_sibling_replace_preserves_both_evaluation_orders() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var first: String, var second: String)
            fun inspect(text: String): String { println(text); return "next" }
            fun finish(text: String, own old: String): Unit { println(text); println(old) }
            fun entry(): Unit {
                val holder = Holder("first" + "!", "second" + "!")
                val old = replace(&holder.first, inspect(holder.second))
                println(old)
                finish(holder.first, replace(&holder.second, "last"))
                println(holder.first)
                println(holder.second)
            }
            "#,
            constants,
            3,
            b"second!\nfirst!\nnext\nsecond!\nnext\nlast\n",
        );
    }
}

pub(in crate::ssa) mod unit_lower_short_circuit_tests {
    use crate::test_support::diagnostics::assert_text_eq;
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::UnitDropPoint,
        parser::{BinaryOperator, Expression},
        source::SourceMap,
        type_checking::{UnitExpressionId, standard_environments},
    };

    use super::super::{
        model::{
            BlockId, EntityId, Function, FunctionId, Operation, ScalarConstant, TerminatorKind,
        },
        render::render_program,
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };

    #[test]
    fn lowers_cross_file_and_or_with_exact_short_edges_and_carried_owner() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
         fun left(): Boolean = true\n\
         fun right(): Boolean = false\n\
         fun consume(own input: String): Unit {}",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\n\
         fun entry(): Boolean {\n\
             val owner = \"kept\"\n\
             val both = p.left() && p.right()\n\
             val either = p.left() || p.right()\n\
             val same = both == either\n\
             val consumed = p.consume(owner)\n\
             return same\n\
         }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "q", "entry");

        let (forward, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("cross-file short-circuit expressions lower to verified SSA");
        let (backward, _) = lower_scalar_unit_with_entry(
            &sources,
            &reversed,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("input permutation preserves short-circuit CFG");
        assert_text_eq!(render_program(&forward), render_program(&backward));

        let module = &forward.modules[0];
        let entry = module
            .functions
            .iter()
            .find(|function| function.name.contains("q.entry"))
            .expect("entry function exists");
        let left = function_id(module, "p.left");
        let right = function_id(module, "p.right");
        let consume = function_id(module, "p.consume");
        let left_calls = call_blocks(entry, left);
        let right_calls = call_blocks(entry, right);
        assert_eq!(left_calls.len(), 2);
        assert_eq!(right_calls.len(), 2);

        let and_merge = assert_short_circuit(entry, left_calls[0], right_calls[0], false, true, 1);
        let or_merge = assert_short_circuit(entry, left_calls[1], right_calls[1], true, false, 2);
        let owner_type = entry
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .and_then(|instruction| instruction.results.first())
            .and_then(|owner| entry.entity(*owner))
            .map(|owner| owner.ty)
            .expect("String owner type exists");
        let and_owner = parameter_with_type(entry, and_merge, owner_type);
        let second_left = entry
            .block(left_calls[1])
            .and_then(|block| block.terminator.as_ref())
            .expect("second left call has a terminator");
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &second_left.kind
        else {
            panic!("second left call forms a conditional");
        };
        assert!(when_true.arguments.contains(&and_owner));
        assert!(when_false.arguments.contains(&and_owner));
        let or_owner = parameter_with_type(entry, or_merge, owner_type);
        let consume_arguments = entry
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.operation {
                Operation::DirectCall {
                    callee, arguments, ..
                } if *callee == consume => Some(arguments.as_slice()),
                _ => None,
            })
            .expect("consume call exists after both merges");
        assert_eq!(consume_arguments, &[or_owner]);
        let compare = entry
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::Compare { left, right, .. } => Some((left, right)),
                _ => None,
            })
            .expect("both and either are compared after the second merge");
        let merge_parameters = &entry
            .block(or_merge)
            .expect("second short-circuit merge exists")
            .parameters;
        assert_eq!(
            Some(EntityId::Value(compare.1)),
            merge_parameters.first().copied()
        );
        assert!(
            merge_parameters[1..].contains(&EntityId::Value(compare.0)),
            "the first result remains carried while the second result occupies slot zero"
        );
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            0,
            "the carried String owner transfers after both short expressions"
        );
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|function| function.instructions.iter())
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            1,
            "the callee drops the owner exactly once after both short-circuit merges"
        );
    }

    #[test]
    fn rhs_only_move_uses_published_short_circuit_cleanup_on_both_exits() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\nfun consume(own input: String): Boolean { println(input); return true }",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            r#"package q
fun original(): Boolean {
 val owner = "kept"
 return true || p.consume(owner)
}
fun either(flag: Boolean): Boolean {
 val owner = "kept".clone()
 return flag || p.consume(owner)
}
fun both(flag: Boolean): Boolean {
 val owner = "kept".clone()
 return flag && p.consume(owner)
}
fun entry(): Unit {
 if (original()) { println("original skip") }
 if (either(true) && either(false) && both(true) && !both(false)) { println("both exits") }
}"#,
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let short_expression = consumer
            .ast()
            .expressions()
            .iter()
            .find_map(|(expression, node)| match node.payload() {
                Expression::Binary {
                    operator: BinaryOperator::LogicalOr,
                    ..
                } => Some(expression),
                _ => None,
            })
            .expect("logical-or expression exists");

        let consumer_unit = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|item| item.name() == "original")
            .unwrap()
            .source_unit();
        assert!(
            owned.ownership().drops().iter().any(|fact| matches!(
                fact.point(), UnitDropPoint::BranchExit { control, branch: 1 }
                if control == UnitExpressionId::new(consumer_unit, short_expression)
            )),
            "the original skip edge must carry a frontend owner cleanup fact"
        );
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("published path-specific owner cleanup must lower to verified SSA");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"original skip\nkept\nkept\nboth exits\n");
    }

    fn function_id(module: &super::super::model::Module, name: &str) -> FunctionId {
        module
            .functions
            .iter()
            .find(|function| function.name.contains(name))
            .expect("reachable helper function exists")
            .id
    }

    fn call_blocks(function: &Function, callee: FunctionId) -> Vec<BlockId> {
        function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee: actual, .. } if actual == callee => {
                    Some(instruction.block)
                }
                _ => None,
            })
            .collect()
    }

    fn parameter_with_type(
        function: &Function,
        block: BlockId,
        ty: super::super::model::EntityType,
    ) -> EntityId {
        function
            .block(block)
            .expect("merge block exists")
            .parameters
            .iter()
            .copied()
            .find(|parameter| {
                function
                    .entity(*parameter)
                    .is_some_and(|entity| entity.ty == ty)
            })
            .expect("merge carries the String owner parameter")
    }

    fn assert_short_circuit(
        function: &Function,
        left_block: BlockId,
        right_block: BlockId,
        short_value: bool,
        right_when_true: bool,
        carried_count: usize,
    ) -> BlockId {
        let left = function.block(left_block).expect("left call block exists");
        let left_call = function
            .instruction(*left.instructions.last().expect("left call exists"))
            .expect("left call instruction exists");
        let [EntityId::Value(left_result)] = left_call.results.as_slice() else {
            panic!("Boolean helper call has one value result");
        };
        let Some(terminator) = &left.terminator else {
            panic!("left call block has a terminator");
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &terminator.kind
        else {
            panic!("left result must form a conditional");
        };
        assert_eq!(condition, left_result);
        let (right_edge, short_edge) = if right_when_true {
            (when_true, when_false)
        } else {
            (when_false, when_true)
        };
        assert_eq!(right_edge.target, right_block);
        assert_eq!(right_edge.arguments.len(), carried_count);
        assert_eq!(short_edge.arguments.len(), carried_count);

        let short = function
            .block(short_edge.target)
            .expect("short-value block exists");
        let short_instruction = function
            .instruction(*short.instructions.last().expect("short constant exists"))
            .expect("short constant instruction exists");
        assert!(matches!(
            short_instruction.operation,
            Operation::Constant(ScalarConstant::Boolean(actual)) if actual == short_value
        ));

        let right = function
            .block(right_block)
            .expect("right call block exists");
        let right_call = function
            .instruction(*right.instructions.last().expect("right call exists"))
            .expect("right call instruction exists");
        let [right_result] = right_call.results.as_slice() else {
            panic!("right Boolean helper has one result");
        };
        let [short_result] = short_instruction.results.as_slice() else {
            panic!("short constant has one result");
        };
        let right_merge = branch_target(right, *right_result);
        let short_merge = branch_target(short, *short_result);
        assert_eq!(right_merge, short_merge);
        assert_eq!(
            function
                .block(right_merge)
                .expect("short-circuit merge exists")
                .parameters
                .len(),
            carried_count + 1,
            "the Boolean result precedes every carried binding"
        );
        right_merge
    }

    fn branch_target(block: &super::super::model::Block, result: EntityId) -> BlockId {
        let Some(terminator) = &block.terminator else {
            panic!("short-circuit exit has a terminator");
        };
        let TerminatorKind::Branch(edge) = &terminator.kind else {
            panic!("short-circuit exit branches to one merge");
        };
        assert_eq!(edge.arguments.first(), Some(&result));
        assert_eq!(&edge.arguments[1..], block.parameters.as_slice());
        edge.target
    }

    #[test]
    fn comparison_operand_return_keeps_evaluation_order_on_both_unit_routes() {
        let source = r#"fun lhs(): Int { println("lhs"); return 1 }
fun rhs(): Int { error("unreachable rhs") }
fun exitsOnLeft(): Int { /*route*/ ((return 3) == rhs()) }
fun exitsOnRight(): Int { /*route*/ lhs() == (return 4) }
fun exitsBeforeShortCircuit(): Int { /*route*/ val unreachable = ((return 5) == 0) || false }
fun maybeExitsInShortCircuit(flag: Boolean): Int { /*route*/
    val choice = flag || ((return 6) == 0)
    if (choice) { return 7 }
    return 0
}
fun exitsOnCondition(): Int { /*route*/
    if ((return 8) == 0) { return 9 }
    return 0
}
fun exitsBeforeNullableRhs(flag: Boolean): Int { /*route*/
    (return 10) == (if (flag) null else null)
}
fun exitsInWhileCondition(): Int { /*route*/
    while ((return 11) == 0) { error("unreachable loop") }
    return 0
}
fun exitsOnRequiredRhs(): Int { /*route*/ true && ((return 12) == 0) }
fun exitsBeforeBypass(flag: Boolean): Int { /*route*/
    val ignored = ((if (flag) { println("left"); return 41 } else true) && false) && (rhs() == 0)
    return 7
}
fun main(): Unit {
    if (exitsOnLeft() == 3 && exitsOnRight() == 4 && exitsBeforeShortCircuit() == 5
        && maybeExitsInShortCircuit(false) == 6 && maybeExitsInShortCircuit(true) == 7
        && exitsOnCondition() == 8 && exitsBeforeNullableRhs(false) == 10
        && exitsInWhileCondition() == 11 && exitsOnRequiredRhs() == 12
        && exitsBeforeBypass(true) == 41 && exitsBeforeBypass(false) == 7) {
        println("exit order")
    } else { error("wrong return") }
}
"#;
        let run = |text| crate::native_tests::emit_link_and_run("operand-exit.ko", text, "main");
        let refine_source = source.replace("/*route*/", "val anchor: Int? = null;");
        let (refined, routed) = super::super::refine::observe_routing(|| run(&refine_source));
        for function in [
            "exitsOnLeft",
            "exitsOnRight",
            "exitsBeforeShortCircuit",
            "maybeExitsInShortCircuit",
            "exitsOnCondition",
            "exitsBeforeNullableRhs",
            "exitsInWhileCondition",
            "exitsOnRequiredRhs",
            "exitsBeforeBypass",
        ] {
            assert!(
                routed
                    .iter()
                    .any(|name| name.split('.').any(|part| part == function)),
                "{function} must stay on the refinement route: {routed:?}"
            );
        }
        crate::native_tests::boxed_enum_tests::assert_success(&refined, b"lhs\nleft\nexit order\n");
        let per_family = super::super::refine::with_per_family_lowering(|| run(source));
        crate::native_tests::boxed_enum_tests::assert_success(
            &per_family,
            b"lhs\nleft\nexit order\n",
        );
    }
}
