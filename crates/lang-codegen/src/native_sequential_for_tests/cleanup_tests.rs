//! SPEC-0182: native temporary Array<String> cleanup across four exit paths.
use super::super::{SymbolKind, UnitSource, boxed_enum_tests, emit_link_and_run};
use crate::test_support::diagnostics::diagnostic;

fn run_cleanup(case: &str, body: &str, expected: &[u8], allocations: usize) {
    let text = format!(
        r#"
        class Guard(val name: String) {{ deinit() {{ println(this.name) }} }}
        fun source(): Array<String> = arrayOf("fi" + "rst", "sec" + "ond")
        fun operand(item: String): Int {{ println(item); return 7 }}
        fun run(): Int {{
            for (item in source()) {{
                val first = Guard("earlier")
                val second = Guard("later")
                {body}
            }}
            println("after")
            return 0
        }}
        fun entry(): Unit {{
            val result = run()
            if (result == 7) {{ println("returned") }}
            println("done")
        }}
    "#
    );
    let source_name = format!("temporary_cleanup_{case}.ko");
    diagnostic!("{source_name}:\n{text}");
    let run = emit_link_and_run(&source_name, &text, "entry");
    boxed_enum_tests::assert_success(&run, expected);

    let analysis = UnitSource::analyze(&source_name, &text)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("supported String provider with body-local resource guards");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let counted = boxed_enum_tests::run_counted_allocations(&ir, allocations);
    boxed_enum_tests::assert_success(&counted, expected);
}

// Fixed stdout and allocation counts come from these source-level cases, not emitted IR:
// two nonempty concat buffers + one container buffer + two Guard payloads per visited body.
// String literals do not allocate; libc stdout allocations are outside the existing hook.
#[test]
fn normal_exit_drops_body_guards_and_temporary_buffers_once() {
    run_cleanup(
        "normal",
        "println(item)",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}

#[test]
fn continue_exit_preserves_temporary_buffers_until_exhaustion() {
    run_cleanup(
        "continue",
        "println(item); continue",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}

#[test]
fn break_exit_drops_unvisited_element_buffer_too() {
    run_cleanup(
        "break",
        "println(item); break",
        b"first\nlater\nearlier\nafter\ndone\n",
        5,
    );
}

#[test]
fn return_operand_runs_before_guards_and_source_cleanup() {
    run_cleanup(
        "return",
        "return operand(item)",
        b"first\nlater\nearlier\nreturned\ndone\n",
        5,
    );
}

#[test]
fn local_destructuring_and_temporary_list_release_each_owner_on_all_exits() {
    let source = r#"
        value class Pair(val first: Int, val second: Int)
        value class Entry(val index: Int, val label: String)
        fun source(): List<Entry> {
            println("source")
            return listOf(Entry(1, "fi" + "rst"), Entry(2, "sec" + "ond"), Entry(3, "th" + "ird"))
        }
        fun run(mode: Int): Int {
            val (second, first) = Pair(1, 2)
            for ((index, label) in source()) {
                println(label)
                if (index == first) { continue }
                if (mode == 1) { break }
                if (mode == 2) { return second }
            }
            println("after")
            return 0
        }
        fun entry(): Unit {
            run(0)
            run(1)
            if (run(2) == 2) { println("returned") }
        }
    "#;
    let name = "destructuring_temporary_list_cleanup.ko";
    let analysis = UnitSource::analyze(name, source).expect("source passes Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("destructuring and temporary iteration lower together");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Each invocation has three concatenated strings and one list buffer, even on early exit.
    let run = boxed_enum_tests::run_counted_allocations(&ir, 12);
    boxed_enum_tests::assert_success(
        &run,
        b"source\nfirst\nsecond\nthird\nafter\nsource\nfirst\nsecond\nafter\nsource\nfirst\nsecond\nreturned\n",
    );
}

#[test]
fn conditional_break_rebinds_source_and_drops_unvisited_buffer_once() {
    run_cleanup(
        "conditional_break",
        "println(item); if (item == \"first\") { break } else { continue }",
        b"first\nlater\nearlier\nafter\ndone\n",
        5,
    );
}

#[test]
fn nested_conditional_break_keeps_source_across_normal_sibling() {
    run_cleanup(
        "nested_conditional_break",
        "println(item); if (item == \"second\") { if (item == \"second\") { break } }",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}

pub(crate) mod conditional_owned_source_tests {
//! SPEC-0312 ordinary unmodified object/link/run. Output is not a free-count oracle.
use super::super::super::{boxed_enum_tests::assert_success, emit_link_and_run};

#[test]
fn owned_source_continue_native_provider_size_path_liveness_matrix() {
    for (container, factory) in [
        ("Array", "arrayOf"),
        ("List", "listOf"),
        ("MutableList", "mutableListOf"),
    ] {
        for size in [0, 1, 3] {
            for stop in [0, 1, 2] {
                for live in [false, true] {
                    let values = (1..=size)
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    // stop=0 never, stop=1 every iteration, stop=2 mixed for size=3.
                    let condition = if stop == 1 {
                        "key > 0".to_owned()
                    } else {
                        format!("key == {stop}")
                    };
                    let after = if live {
                        format!("if (keys.size == {size}) {{ println(\"live\") }}")
                    } else {
                        String::new()
                    };
                    let source = format!(
                        "fun source(): {container}<Int> {{ println(\"seed\"); return {factory}<Int>({values}) }}\nfun main(): Unit {{ var keys = source(); for (key in keys) {{ println(\"visit\"); if ({condition}) {{ continue }}; println(\"tail\") }}; {after}; println(\"done\") }}"
                    );
                    let mut expected = "seed\n".to_owned();
                    for key in 1..=size {
                        expected.push_str("visit\n");
                        if !(stop == 1 || key == stop) {
                            expected.push_str("tail\n");
                        }
                    }
                    if live {
                        expected.push_str("live\n");
                    }
                    expected.push_str("done\n");
                    eprintln!("{container} size={size} policy={stop} live={live}");
                    assert_success(
                        &emit_link_and_run("owned-source-continue.ko", &source, "main"),
                        expected.as_bytes(),
                    );
                }
            }
        }
    }
}

#[test]
fn owned_source_continue_native_body_cleanup_and_post_loop_permission() {
    let text = r#"
class Token(val name: String) { deinit() { println(this.name) } }
fun consume(own source: Array<Int>): Unit { if (source.size == 3) { println("moved") } }
fun main(): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) {
        val first = Token("first")
        val second = Token("second")
        if (key == 2) { continue }
        println("tail")
    }
    consume(keys)
    var list = mutableListOf(1, 2, 3)
    for (key in list) { if (key == 2) { continue } }
    list.add(4)
    list.removeLast()
    if (list.size == 3) { println("mutated") }
}
"#;
    assert_success(
        &emit_link_and_run("owned-source-cleanup.ko", text, "main"),
        b"tail\nsecond\nfirst\nsecond\nfirst\ntail\nsecond\nfirst\nmoved\nmutated\n",
    );
}

#[test]
fn owned_source_continue_native_owned_parameter_string_and_borrow_controls() {
    let text = r#"
fun owned(own keys: List<String>): Unit {
    for (key in keys) { println(key); if (key == "two") { continue }; println("tail") }
    if (keys.size == 3) { println("owned") }
}
fun borrowed(keys: Array<String>): Unit {
    for (key in keys) { println(key); if (key == "two") { continue }; println("tail") }
}
fun caller(): Unit {
    val keys = arrayOf("one", "two", "three")
    borrowed(keys)
    if (keys.size == 3) { println("borrowed") }
}
fun temporary(): Unit {
    for (key in arrayOf("one", "two", "three")) {
        println(key)
        if (key == "two") { continue }
        println("tail")
    }
    println("temporary")
}
fun main(): Unit { owned(listOf("one", "two", "three")); caller(); temporary() }
"#;
    assert_success(&emit_link_and_run("owned-source-modes.ko", text, "main"),
        b"one\ntail\ntwo\nthree\ntail\nowned\none\ntail\ntwo\nthree\ntail\nborrowed\none\ntail\ntwo\nthree\ntail\ntemporary\n");
}

#[test]
fn owned_source_continue_native_moveonly_destructure_and_discard() {
    let text = r#"
value class Item(val id: Int, val name: String)
fun destructured(): Unit {
    val items = listOf(Item(1, "one"), Item(2, "two"), Item(3, "three"))
    for ((id, name) in items) { println(name); if (id == 2) { continue }; println("tail") }
    if (items.size == 3) { println("retained") }
}
fun discarded(): Unit {
    val keys = arrayOf(1, 2, 3)
    for (_ in keys) { if (keys.size == 3) { continue }; println("wrong") }
    println("done")
}
fun main(): Unit { destructured(); discarded() }
"#;
    assert_success(
        &emit_link_and_run("owned-source-destructure.ko", text, "main"),
        b"one\ntail\ntwo\nthree\ntail\nretained\ndone\n",
    );
}

#[test]
fn owned_source_continue_native_fresh_inner_dynamic_instances() {
    assert_success(
        &emit_link_and_run("fresh-inner.ko", include_str!("../../tests/fixtures/owned-source-continue-0312/fresh-inner.ko"), "main"),
        b"first\nbody\ntail\nlive\nsecond\nbody\ntail\nlive\nthird\nbody\ntail\nfourth\nbody\ntail\n",
    );
}

#[test]
fn owned_source_continue_native_original_map_fixture() {
    assert_success(
        &emit_link_and_run("providers-cleanup.ko", include_str!("../../tests/fixtures/map-conditional-continue-0311/providers-cleanup.ko"), "main"),
        b"tail\ntail\nnamedArray\nsource kept\ntail\ntail\ntemporaryArray\ntail\ntail\nnamedList\nsource kept\ntail\ntail\ntemporaryList\ntail\ntail\nnamedMutableList\nsource kept\ntail\ntail\ntemporaryMutableList\n",
    );
}

#[test]
fn owned_source_continue_native_map_and_source_liveness() {
    for map_live in [false, true] {
        for source_live in [false, true] {
            eprintln!("Map/source liveness map={map_live}, source={source_live}");
            let text = format!(
                "fun main(): Unit {{ val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) {{ first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) {{ continue }}; first.put(9, \"tail one\".clone()); second.put(9, \"tail two\".clone()); println(\"tail\") }}; {}; {} }}",
                if map_live {
                    "if (first.contains(3)) { println(\"first map\") }; if (second.contains(3)) { println(\"second map\") }"
                } else {
                    ""
                },
                if source_live {
                    "if (keys.size == 3) { println(\"source\") }"
                } else {
                    ""
                },
            );
            let expected = format!(
                "tail\ntail\n{}{}",
                if map_live {
                    "first map\nsecond map\n"
                } else {
                    ""
                },
                if source_live { "source\n" } else { "" }
            );
            assert_success(
                &emit_link_and_run("map-source-liveness.ko", &text, "main"),
                expected.as_bytes(),
            );
        }
    }
}

#[test]
fn owned_source_continue_native_resource_source_old_unconditional_path() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun main(): Unit {
    val keys = arrayOf(Leaf("first"), Leaf("second"))
    for (key in keys) { println("visit"); continue }
    println("done")
}
"#;
    assert_success(
        &emit_link_and_run("resource-source-old-path.ko", source, "main"),
        b"visit\nvisit\ndone\nsecond\nfirst\n",
    );
}
}

pub(crate) mod packed_field_replace_tests_5 {
//! SPEC-0246：单文件 direct-field commit 与未扩展边界。
use crate::ssa::lower_frontend_tests::*;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::ssa::model::{
    Definition, EntityId, EntityType, Function, Instruction, LoanKind, Origin, Program,
    SsaTypeKind, ValueId,
};
use crate::test_support::diagnostics::assert_diagnostic;
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{ConstructionDeliveryKind, UnitDropPoint, UnitDropTarget},
    type_checking::UnitConstructionTarget,
};

fn analyze_field(source: &str) -> UnitSource {
    let analysis = UnitSource::analyze("lowering.ko", source)
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    analysis.assert_frontend_clean();
    analysis
}

fn lower_field(
    analysis: &UnitSource,
) -> Result<crate::ssa::model::Program, crate::ssa::LoweringError> {
    let entry = analysis.declaration("test", lang_frontend::name_resolution::SymbolKind::Function);
    analysis.lower_entry(entry).map(|(program, _)| program)
}

#[test]
fn field_replace_single_uses_dedicated_exchange_and_preserves_parent() {
    let analysis = analyze_field(
        r#"
        class Holder(var state: String, val id: Int)
        fun test(): String {
            val holder = Holder("old", 1)
            val old = replace(&holder.state, "new")
            if (holder.id == 1) println(old)
            return replace(&holder.state, "last")
        }
    "#,
    );
    let program = lower_field(&analysis).expect("direct field exchange");
    let rendered = render_program(&program);
    assert_diagnostic!(rendered.contains("heap.field_exchange"), "{rendered}");
    assert_diagnostic!(!rendered.contains("root_replace"), "{rendered}");
    render_verified_program(&program).expect("dedicated exchange verifies");
}

#[test]
fn field_replace_single_existing_field_storage_families() {
    for (declarations, ty, first, second) in [
        ("", "Int", "1", "2"),
        ("", "Boolean", "true", "false"),
        ("class Node(val id: Int)", "Node", "Node(1)", "Node(2)"),
        ("class Node(val id: Int)", "Node?", "Node(1)", "Node(2)"),
        (
            "value class Inline(val id: Int)",
            "Inline",
            "Inline(1)",
            "Inline(2)",
        ),
        (
            "value class Inline(val id: Int)",
            "Box<Inline>",
            "Box(Inline(1))",
            "Box(Inline(2))",
        ),
        ("", "Rc<Int>", "Rc(1)", "Rc(2)"),
        ("", "List<Int>", "listOf(1)", "listOf(2)"),
    ] {
        let source = format!(
            "{declarations}\nclass Holder(var state: {ty})\nfun test(): Unit {{ val initial: {ty} = {first}\nval holder = Holder(initial)\nval old = replace(&holder.state, {second}) }}"
        );
        let analysis = analyze_field(&source);
        let program =
            lower_field(&analysis).unwrap_or_else(|failure| panic!("{source}\n{failure:?}"));
        if ty == "Node?" {
            assert_nullable_field_exchange(&analysis, &program);
        }
        render_verified_program(&program).expect("existing field storage verifies");
    }
}

fn assert_nullable_field_exchange(analysis: &UnitSource, program: &Program) {
    let test = analysis.declaration("test", SymbolKind::Function);
    let span = analysis.declaration_span(test);
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin.span() == span)
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("原 test 的 source-qualified SSA function 必须唯一");
    };
    let module = program.module(function.id.module()).expect("entry module");
    assert_eq!(function.blocks.len(), 1);
    let source = |instruction: &Instruction| {
        let origin = instruction.origin.span();
        assert_eq!(origin.source_id(), span.source_id());
        analysis
            .sources()
            .slice(origin)
            .expect("same original source")
    };
    let allocations = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
        .collect::<Vec<_>>();
    let [first, holder, second] = allocations.as_slice() else {
        panic!("保留两个 Node 与一个 Holder 的真实 allocation");
    };
    assert_eq!(source(first), "Node(1)");
    assert_eq!(source(holder), "Holder(initial)");
    assert_eq!(source(second), "Node(2)");
    let first_owner = field_value_result(first);
    let holder_owner = field_value_result(holder);
    let second_owner = field_value_result(second);
    let wraps = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableWrap { .. }))
        .collect::<Vec<_>>();
    let [initial, replacement] = wraps.as_slice() else {
        panic!("初始化与 RHS 各 wrap 一次，exact 构造参数不可二次 wrap");
    };
    let Operation::NullableWrap { nullable, owner } = initial.operation else {
        unreachable!();
    };
    assert_eq!(owner, first_owner);
    assert_eq!(source(initial), "val initial: Node? = Node(1)");
    assert_eq!(
        replacement.operation,
        Operation::NullableWrap {
            nullable,
            owner: second_owner
        }
    );
    assert_eq!(source(replacement), "Node(2)");
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("字段必须保留独立 nullable handle 类型");
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_ne!(*inner, nullable);
    for value in [first_owner, second_owner] {
        assert_eq!(
            function.entity(EntityId::Value(value)).unwrap().ty,
            EntityType::Value(*inner)
        );
    }
    let initial_value = field_value_result(initial);
    let replacement_value = field_value_result(replacement);
    let Operation::HeapAllocate { payload, .. } = holder.operation else {
        unreachable!();
    };
    let payload_instruction = field_value_definition(function, payload);
    let Operation::AggregateConstruct {
        aggregate,
        ref fields,
    } = payload_instruction.operation
    else {
        panic!("Holder 仍从真实字段值构造 payload");
    };
    assert_eq!(source(payload_instruction), "Holder(initial)");
    assert_eq!(fields.as_slice(), [initial_value]);
    assert!(
        matches!(module.type_kind(aggregate), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.as_slice() == [nullable])
    );
    let holder_declaration = analysis.declaration("Holder", SymbolKind::Classifier);
    let plans = analysis
        .ownership()
        .construction_plans()
        .iter()
        .filter(|plan| plan.target() == UnitConstructionTarget::Nominal(holder_declaration))
        .collect::<Vec<_>>();
    let [plan] = plans.as_slice() else {
        panic!("原 Holder 构造必须有唯一 source-qualified ownership plan");
    };
    let [delivery] = plan.deliveries() else {
        panic!("原 initial 是唯一构造实参");
    };
    assert_eq!(delivery.kind(), ConstructionDeliveryKind::Move);
    assert_eq!(delivery.construction(), plan.construction());
    assert_eq!(
        delivery.argument().source_unit(),
        plan.construction().source_unit()
    );
    assert_eq!(delivery.parameter_index(), 0);
    let exchanges = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldExchange { .. }))
        .collect::<Vec<_>>();
    let [exchange] = exchanges.as_slice() else {
        panic!("原 replace 必须且只能提交一次字段交换");
    };
    let Operation::HeapFieldExchange {
        owner,
        field,
        loan,
        replacement,
    } = exchange.operation
    else {
        unreachable!();
    };
    assert_eq!(source(exchange), "replace(&holder.state, Node(2))");
    assert_eq!(owner, holder_owner);
    assert_eq!(field, 0);
    assert_eq!(replacement, replacement_value);
    assert_eq!(
        function.entity(EntityId::Loan(loan)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Exclusive,
            target: nullable
        }
    );
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Loan(loan)).unwrap().definition
    else {
        panic!("exchange 消费本次真实 field loan");
    };
    let begin = function.instruction(instruction).unwrap();
    assert!(matches!(
        begin.operation,
        Operation::BorrowBegin {
            kind: LoanKind::Exclusive,
            ..
        }
    ));
    let [loan_fact] = analysis.ownership().loans() else {
        panic!("原 replace 只有一个独占借用 fact");
    };
    assert_eq!(begin.origin.span(), loan_fact.begin_span());
    assert_eq!(exchange.origin.span(), loan_fact.end_span());
    assert_eq!(
        loan_fact.call().source_unit(),
        plan.construction().source_unit()
    );
    let [replacement_plan] = analysis.ownership().field_replacements() else {
        panic!("原 replace 必须有唯一独立字段提交能力");
    };
    assert_eq!(replacement_plan.descriptor().expression(), loan_fact.call());
    assert_eq!(
        replacement_plan.place().fields(),
        [delivery.parameter_symbol().expect("原 state 字段 symbol")]
    );
    let old = field_value_result(exchange);
    for value in [initial_value, replacement_value, old] {
        assert_eq!(
            function.entity(EntityId::Value(value)).unwrap().ty,
            EntityType::Value(nullable)
        );
    }
    assert_ne!(old, initial_value);
    assert_ne!(old, replacement_value);
    let [holder_fact, old_fact] = analysis.ownership().drops() else {
        panic!("原 source 只发布父 owner 与未使用 old 的两个 drop fact");
    };
    let holder_symbol = replacement_plan.place().root();
    assert_eq!(holder_fact.target(), UnitDropTarget::Named(holder_symbol));
    assert_eq!(
        holder_fact.point(),
        UnitDropPoint::CallReturn(loan_fact.call())
    );
    let UnitDropTarget::Named(old_symbol) = old_fact.target() else {
        panic!("exchange 返回值归属原 old binding，不是匿名 temporary");
    };
    assert_ne!(old_symbol, holder_symbol);
    assert_eq!(old_symbol.source_unit(), holder_symbol.source_unit());
    let UnitDropPoint::AfterStatement(old_statement) = old_fact.point() else {
        panic!("未使用 old 在其 local statement 完成后析构");
    };
    assert_eq!(old_statement.source_unit(), loan_fact.call().source_unit());
    for (fact, start, end, text) in [
        (holder_fact, 107, 113, "holder"),
        (old_fact, 136, 139, "old"),
    ] {
        let expected = analysis
            .sources()
            .span(span.source_id(), start, end)
            .unwrap();
        assert_eq!(analysis.sources().slice(expected), Ok(text));
        assert_eq!(fact.value_origin(), expected);
    }
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [holder_drop, old_drop] = drops.as_slice() else {
        panic!("父 owner 与旧字段各 drop 一次；已移交的 inner/wrapper 不再独立 drop");
    };
    assert_eq!(
        holder_drop.operation,
        Operation::Drop {
            owner: holder_owner
        }
    );
    assert_eq!(old_drop.operation, Operation::Drop { owner: old });
    assert_eq!(
        holder_drop.origin,
        Origin::Source(holder_fact.value_origin())
    );
    assert_eq!(old_drop.origin, Origin::Source(old_fact.value_origin()));
    let position = |instruction: &Instruction| {
        function.blocks[0]
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap()
    };
    assert!(position(exchange) < position(holder_drop));
    assert!(position(holder_drop) < position(old_drop));
    assert!(!function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootReplace { .. } | Operation::BorrowEnd { .. }
    )));
}

fn field_value_result(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("expected one real value result");
    };
    *value
}

fn field_value_definition(function: &Function, value: ValueId) -> &Instruction {
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Value(value)).unwrap().definition
    else {
        panic!("expected an original instruction result");
    };
    function.instruction(instruction).unwrap()
}

#[test]
fn field_replace_single_nullable_layout_keeps_nonclass_and_generic_boundaries() {
    for (declarations, owner) in [
        ("class Holder(val state: Rc<Int>?)", "Holder"),
        (
            "value class Inline(val id: Int)\nclass Holder(val state: Box<Inline>?)",
            "Holder",
        ),
        ("class Holder(val state: Int?)", "Holder"),
        ("class Holder(val state: String?)", "Holder"),
        (
            "value class Inline(val id: Int)\nclass Holder(val state: Inline?)",
            "Holder",
        ),
        (
            "enum class Choice { Empty }\nclass Holder(val state: Choice?)",
            "Holder",
        ),
        (
            "class Node<T>(val id: T)\nclass Holder(val state: Node<Int>?)",
            "Holder",
        ),
        (
            "class Node(val id: Int)\nclass Holder<T>(val state: Node?)",
            "Holder<Int>",
        ),
    ] {
        let source = format!("{declarations}\nfun test(own input: {owner}): Unit {{}}");
        let analysis = analyze_field(&source);
        let error = lower_field(&analysis)
            .err()
            .expect("unapproved nullable field recipe");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        assert_eq!(
            analysis
                .sources()
                .slice(error.span.expect("source-qualified field rejection")),
            Ok("state"),
            "{source}"
        );
    }
    // SPEC-0350 / ADR-0033: a generic class instantiated with a scalar or String
    // nullable field interns the inline nullable layout, so this recipe now lowers
    // (non-generic `Int?`/`String?` fields above keep the pointer-like storage gate).
    for owner in ["Holder<Int>", "Holder<String>"] {
        let source =
            format!("class Holder<T>(val state: T?)\nfun test(own input: {owner}): Unit {{}}");
        let program = lower_field(&analyze_field(&source))
            .unwrap_or_else(|error| panic!("inline nullable field layout: {error:?}\n{source}"));
        crate::llvm::render_verified_program(&program).expect("verified LLVM");
    }
}

#[test]
fn field_replace_single_nullable_layout_preserves_direct_parameter_and_name_independence() {
    for (declarations, owner) in [
        (
            "class Leaf(val id: Int)\nclass Envelope(val state: Leaf?)",
            "Envelope",
        ),
        (
            "class Leaf(val id: Int)\nclass Envelope<T>(val state: T?)",
            "Envelope<Leaf>",
        ),
        (
            "value class Inline(val id: Int)\nclass Envelope<T>(val state: T?)",
            "Envelope<Box<Inline>>",
        ),
        ("class Envelope<T>(val state: T?)", "Envelope<Rc<Int>>"),
    ] {
        let source = format!("{declarations}\nfun test(own input: {owner}): Unit {{}}");
        let analysis = analyze_field(&source);
        let program = lower_field(&analysis).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        assert_eq!(
            program
                .modules
                .iter()
                .flat_map(|module| &module.types)
                .filter(|ty| matches!(ty, SsaTypeKind::NullableHandle { .. }))
                .count(),
            1,
            "{source}"
        );
        render_verified_program(&program).expect("bounded nullable field layout verifies");
    }
}

#[test]
fn field_replace_single_nullable_recursive_class_layout_stays_unsupported() {
    for (source, start, end, parameter) in [
        (
            "class Node(val next: Node?)\nfun test(own input: Node): Unit {}",
            37,
            52,
            "own input: Node",
        ),
        (
            "class Left(val right: Right?)\nclass Right(val left: Left?)\nfun test(own input: Left): Unit {}",
            68,
            83,
            "own input: Left",
        ),
    ] {
        let analysis = analyze_field(source);
        let error = lower_field(&analysis)
            .err()
            .expect("recursive nullable layout remains closed");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        // Signature intern stores this exact parameter span in the pending owner.
        // Nullable re-entry rejects the already-active root, not the recursive field.
        let source_id = analysis
            .declaration_span(analysis.declaration("test", SymbolKind::Function))
            .source_id();
        let expected = analysis.sources().span(source_id, start, end).unwrap();
        assert_eq!(analysis.sources().slice(expected), Ok(parameter));
        assert_eq!(error.span, Some(expected), "{source}");
    }
}

#[test]
fn field_replace_single_unclosed_projections_stay_unsupported() {
    for source in [
        "class Inner(var value: Int)\nclass Outer(val inner: Inner)\nfun test(): Unit { val holder = Outer(Inner(1))\nval old = replace(&holder.inner.value, 2) }",
        "class Holder<T>(var value: T)\nfun test(): Unit { val holder = Holder(1)\nval old = replace(&holder.value, 2) }",
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nswap(&holder.a, &holder.b) }",
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nval old = replace(&holder.a, holder.b) }",
        "class Holder(var a: String, var b: String)\nfun test(): Unit { val holder = Holder(\"a\", \"b\")\nval old = replace(&holder.a, holder.b.clone()) }",
    ] {
        let analysis = analyze_field(source);
        assert_eq!(
            lower_field(&analysis).err().expect("unsupported").kind,
            LoweringErrorKind::UnsupportedNode,
            "{source}"
        );
    }
}

#[test]
fn field_replace_single_allows_direct_sibling_exchange() {
    let analysis = analyze_field(
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nval old = replace(&holder.a, replace(&holder.b, 3)) }",
    );
    let program = lower_field(&analysis).expect("direct sibling exchanges preserve their parent");
    assert_eq!(
        render_program(&program)
            .matches("heap.field_exchange")
            .count(),
        2
    );
    render_verified_program(&program).expect("sibling exchanges verify");
}
}
