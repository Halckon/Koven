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
        "class Holder(var first: String, val second: String)\nfun inspect(text: String): String { println(text)\nreturn \"next\" }\nfun entry(): Unit {\nval holder = Holder(\"first\", \"second\")\nval old = replace(&holder.first, inspect(holder.second))\n}",
        "class Holder(val first: String, var second: String)\nfun inspect(text: String, own other: String): Unit { println(text)\nprintln(other) }\nfun entry(): Unit {\nval holder = Holder(\"first\", \"second\")\ninspect(holder.first, replace(&holder.second, \"next\"))\n}",
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
