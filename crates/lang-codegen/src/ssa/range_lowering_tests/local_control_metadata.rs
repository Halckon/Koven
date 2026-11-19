//! Native branch and scope-boundary evidence for borrowed RangeView metadata.
use super::ownership::{carrier_scope_layout, run_carrier_layouts};

#[test]
fn range_carrier_gate_metadata_alias_crosses_normal_if() {
    for (case, branch, expected) in [
        ("one-arm", "if(flag){read(child)}",
            b"first\njoin\nfirst\ndrop\nsecond\ndrop\nfirst\njoin\nfirst\ndrop\nsecond\ndrop\nfirst\n".as_slice()),
        ("two-arm", "if(flag){read(child)}else{println(\"else\")}",
            b"first\njoin\nfirst\ndrop\nsecond\ndrop\nfirst\nelse\njoin\nfirst\ndrop\nsecond\ndrop\nfirst\n".as_slice()),
    ] {
        carrier_scope_layout(
            case,
            branch,
            &format!("borrow val parent=take(root,1)\nborrow val child=(parent)\n{branch}\nprintln(\"join\");read(child)"),
            expected,
        );
    }
}

#[test]
fn range_carrier_gate_nested_alias_ends_before_the_parent() {
    let branch = "if(flag){read(child)}else{println(\"else\")}";
    let body = format!(
        "borrow val parent=take(root,1)\n{{borrow val child=(parent);{branch};println(\"joined\");read(child)}}\nprintln(\"outer\");read(parent)"
    );
    carrier_scope_layout(
        "nested-alias",
        branch,
        &body,
        b"first\njoined\nfirst\nouter\nfirst\ndrop\nsecond\ndrop\nfirst\nelse\njoined\nfirst\nouter\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_nested_alias_borrow_ends_at_the_inner_boundary() {
    use super::{Pipeline, lower};
    use crate::ssa::model::Operation;

    let source = r#"
fun read(view:View<String>):Unit{}
fun scan(flag:Boolean):Unit {
    val root=listOf("first","second")
    borrow val parent=take(root,1)
    {
        borrow val child=(parent)
        if(flag){read(child)}
        read((child))
    }
    read(parent)
}
fun main():Unit{scan(true);scan(false)}
"#;
    let lowered = lower(source, "borrow val child", Pipeline::Unit);
    let (program, _) = lowered.result.expect("nested alias lowers");
    let marker = lowered.expected_span;
    let (function, child_end) = program.modules[0]
        .functions
        .iter()
        .find_map(|function| {
            function.instructions.iter().find_map(|instruction| {
                let origin = instruction.origin.span();
                (matches!(instruction.operation, Operation::BorrowEnd { .. })
                    && origin.source_id() == marker.source_id()
                    && origin.start() == marker.start())
                .then_some((function, instruction))
            })
        })
        .expect("scan function ends the child alias loan");
    let merge = &function.blocks[child_end.block.index()];
    let instructions: Vec<_> = merge
        .instructions
        .iter()
        .map(|id| &function.instructions[id.index()])
        .collect();
    let position = |predicate: &dyn Fn(&crate::ssa::model::Instruction) -> bool| {
        instructions
            .iter()
            .position(|instruction| predicate(instruction))
            .expect("scope operations share the continuing block")
    };
    let read_call = |source| {
        position(&|instruction| {
            matches!(instruction.operation, Operation::DirectCall { .. })
                && lowered.sources.slice(instruction.origin.span()).ok() == Some(source)
        })
    };
    let end = position(&|instruction| instruction.id == child_end.id);
    let parent_end =
        position(&|instruction| matches!(instruction.operation, Operation::RangeEnd { .. }));
    assert!(read_call("read((child))") < end);
    assert!(end < read_call("read(parent)"));
    assert!(read_call("read(parent)") < parent_end);
}

#[test]
fn range_carrier_gate_alias_inside_if_arm_keeps_the_outer_parent() {
    let inner = "if(inner){println(\"hit\");read(child)}else{println(\"inner-else\")}";
    let arm = format!("borrow val child=(parent);{inner};println(\"join\");read(child)");
    for (case, depth) in [
        ("direct-arm", 0),
        ("arm-block", 1),
        ("two-arm-blocks", 2),
        ("four-arm-blocks", 4),
    ] {
        let arm = format!("{}{arm}{}", "{".repeat(depth), "}".repeat(depth));
        let source = format!(
            r#"
class Item(val text:String){{deinit(){{println("drop");println(this.text)}}}}
fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
fun scan(flag:Boolean,inner:Boolean):Unit{{
    val root=listOf(Item("first"),Item("second"))
    borrow val parent=take(root,1)
    if(flag){{{arm}}}else{{println("skip")}}
    println("outer");read(parent)
}}
fun main():Unit{{scan(true,true);scan(true,false);scan(false,true)}}
"#
        );
        run_carrier_layouts(
            case,
            &source,
            inner,
            b"hit\nfirst\njoin\nfirst\nouter\nfirst\ndrop\nsecond\ndrop\nfirst\ninner-else\njoin\nfirst\nouter\nfirst\ndrop\nsecond\ndrop\nfirst\nskip\nouter\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }
}

#[test]
fn range_carrier_gate_deep_arm_checks_intermediate_statements() {
    use super::{Pipeline, lower};
    use crate::ssa::LoweringErrorKind;

    let outer =
        "if(flag){{val unproved=0;{borrow val child=(parent);if(flag){read(child)};read(child)}}}";
    let source = format!(
        r#"
fun read(view:View<String>):Unit{{}}
fun scan(flag:Boolean):Unit{{
    val root=listOf("first")
    borrow val parent=take(root,1)
    {outer}
    read(parent)
}}
fun main():Unit{{scan(true)}}
"#
    );
    let lowered = lower(&source, outer, Pipeline::Unit);
    let Err(error) = lowered.result else {
        panic!("an unchecked intermediate declaration must keep the arm rejected");
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn range_carrier_gate_if_chain_keeps_live_parent_on_each_edge() {
    let inner = "if(inner){println(\"hit\");read(child)}else{println(\"inner-else\")}";
    let arm = format!("borrow val child=(parent);{inner};println(\"join\");read(child)");
    for depth in [0, 2] {
        let middle = format!(
            "if(middle){{{}{arm}{}}}else{{println(\"skip-middle\");read(parent)}}",
            "{".repeat(depth),
            "}".repeat(depth),
        );
        let outer = format!(
            "if(outer){{{}{middle}{}}}else{{println(\"skip-outer\");read(parent)}}",
            "{".repeat(depth),
            "}".repeat(depth),
        );
        let source = format!(
            r#"
class Item(val text:String){{deinit(){{println(this.text)}}}}
fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
fun scan(outer:Boolean,middle:Boolean,inner:Boolean):Unit{{
    val root=listOf(Item("first"),Item("second"))
    borrow val parent=take(root,1)
    {outer}
    println("outer");read(parent)
}}
fun main():Unit{{scan(true,true,true);scan(true,true,false);scan(true,false,true);scan(false,true,true)}}
"#
        );
        run_carrier_layouts(
            &format!("if-chain-{depth}"),
            &source,
            inner,
            b"hit\nfirst\njoin\nfirst\nouter\nfirst\nsecond\nfirst\ninner-else\njoin\nfirst\nouter\nfirst\nsecond\nfirst\nskip-middle\nfirst\nouter\nfirst\nsecond\nfirst\nskip-outer\nfirst\nouter\nfirst\nsecond\nfirst\n",
        );
    }
}

#[test]
fn range_carrier_gate_parent_created_in_if_arm_ends_before_outer_merge() {
    let inner = "if(inner){println(\"hit\");read(child)}else{println(\"inner-else\")}";
    let arm = format!(
        "borrow val parent=take(root,count());if(middle){{borrow val child=(parent);{inner};println(\"join\");read(child)}}else{{println(\"skip-middle\");read(parent)}};println(\"parent\");read(parent)"
    );
    for depth in [0, 1] {
        let source = format!(
            r#"
class Item(val text:String){{deinit(){{println(this.text)}}}}
fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
fun count():Int{{println("make-view");return 1}}
fun scan(outer:Boolean,middle:Boolean,inner:Boolean):Unit{{
    val root=listOf(Item("first"),Item("second"))
    if(outer){{{}{arm}{}}}else{{println("skip-outer")}}
    println("merged")
}}
fun main():Unit{{scan(true,true,true);scan(true,true,false);scan(true,false,true);scan(false,true,true)}}
"#,
            "{".repeat(depth),
            "}".repeat(depth),
        );
        run_carrier_layouts(
            &format!("arm-parent-{depth}"),
            &source,
            inner,
            b"make-view\nhit\nfirst\njoin\nfirst\nparent\nfirst\nmerged\nsecond\nfirst\nmake-view\ninner-else\njoin\nfirst\nparent\nfirst\nmerged\nsecond\nfirst\nmake-view\nskip-middle\nfirst\nparent\nfirst\nmerged\nsecond\nfirst\nskip-outer\nmerged\nsecond\nfirst\n",
        );
    }
}

#[test]
fn range_carrier_gate_arm_parent_cleanup_stays_on_the_taken_edge() {
    use super::{Pipeline, lower};
    use crate::ssa::model::{Operation, TerminatorKind};

    let source = r#"
fun read(view:View<String>):Unit{}
fun skip():Unit{}
fun merged():Unit{}
fun scan(outer:Boolean,inner:Boolean):Unit {
    val root=listOf("first")
    if(outer) {
        borrow val parent=take(root,1)
        {
            borrow val child=(parent)
            if(inner){read(child)}
            read((child))
        }
        read(parent)
    } else { skip() }
    merged()
}
fun main():Unit{scan(true,true);scan(false,false)}
"#;
    let lowered = lower(source, "borrow val child", Pipeline::Unit);
    let (program, _) = lowered.result.expect("branch-local parent lowers");
    let (function, parent_end) = program.modules[0]
        .functions
        .iter()
        .find_map(|function| {
            function.instructions.iter().find_map(|instruction| {
                matches!(instruction.operation, Operation::RangeEnd { .. })
                    .then_some((function, instruction))
            })
        })
        .expect("scan ends the branch-local descriptor");
    let source_is = |instruction: &crate::ssa::model::Instruction, text: &str| {
        lowered.sources.slice(instruction.origin.span()).ok() == Some(text)
    };
    let call = |text| {
        function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { .. })
                    && source_is(instruction, text)
            })
            .unwrap_or_else(|| panic!("expected source call {text} in scan"))
    };
    let child_end = function
        .instructions
        .iter()
        .find(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { .. })
                && instruction.origin.span().start() == lowered.expected_span.start()
        })
        .expect("scan ends the metadata alias");
    let end_block = &function.blocks[parent_end.block.index()];
    let position = |id| {
        end_block
            .instructions
            .iter()
            .position(|instruction| *instruction == id)
            .expect("cleanup and reads share the taken arm's continuing block")
    };
    assert!(position(call("read((child))").id) < position(child_end.id));
    assert!(position(child_end.id) < position(call("read(parent)").id));
    assert!(position(call("read(parent)").id) < position(parent_end.id));

    let skip = call("skip()").block;
    let merge = call("merged()").block;
    assert_ne!(end_block.id, skip);
    for predecessor in [end_block.id, skip] {
        let terminator = function.blocks[predecessor.index()]
            .terminator
            .as_ref()
            .unwrap();
        assert!(matches!(&terminator.kind, TerminatorKind::Branch(edge) if edge.target == merge));
    }
    assert!(function.blocks.iter().any(|block| {
        block.terminator.as_ref().is_some_and(|term| {
            matches!(&term.kind, TerminatorKind::Conditional { when_false, .. } if when_false.target == skip)
        })
    }));
}

#[test]
fn range_carrier_gate_short_circuit_precedes_creation_in_the_taken_arm() {
    let source = r#"
class Item(val text:String){deinit(){println(this.text)}}
fun read(view:View<Item>):Unit{for(item in view){println(item.text)}}
fun rhs(result:Boolean):Boolean{println("rhs");return result}
fun count():Int{println("make-view");return 1}
fun scan(left:Boolean,right:Boolean):Unit {
    val root=listOf(Item("item"))
    if(left && rhs(right)) {
        borrow val parent=take(root,count())
        {
            borrow val child=(parent)
            if(right){read(child)}
            read((child))
        }
        read(parent)
    } else { println("skip") }
    println("merged")
}
fun main():Unit{scan(false,true);scan(true,false);scan(true,true)}
"#;
    run_carrier_layouts(
        "short-condition-arm-parent",
        source,
        "left && rhs(right)",
        b"skip\nmerged\nitem\nrhs\nskip\nmerged\nitem\nrhs\nmake-view\nitem\nitem\nitem\nmerged\nitem\n",
    );
}
