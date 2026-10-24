//! Public range count arithmetic consumes typed Int facts and checked control flow.
use super::*;

#[test]
fn public_std_range_scalar_counts_preserve_live_descriptors() {
    for algorithm in ["take", "drop", "dropLast"] {
        for receiver in [false, true] {
            for source in ["root", "parent"] {
                let mut checks = String::new();
                let mut expected = String::new();
                let values = if source == "root" {
                    &["a", "b", "c", "d"][..]
                } else {
                    &["b", "c", "d"][..]
                };
                for (index, (expression, count)) in [
                    ("n-1", 1_usize),
                    ("(n+1)-2", 1),
                    ("n*2", 4),
                    ("n/2", 1),
                    ("n%2", 0),
                    ("parent.size-1", 2),
                    ("localCount", 1),
                ]
                .into_iter()
                .enumerate()
                {
                    let clipped = count.min(values.len());
                    let kept = match algorithm {
                        "take" => &values[..clipped],
                        "drop" => &values[clipped..],
                        _ => &values[..values.len() - clipped],
                    };
                    for value in kept {
                        expected.push_str(value);
                        expected.push('\n');
                    }
                    let call = if receiver {
                        format!("{source}.trim({expression})")
                    } else {
                        format!("trim({source},{expression})")
                    };
                    checks.push_str(&format!(
                        "borrow val child{index}={call};read(child{index},{});",
                        kept.len()
                    ));
                }
                // Reuse the original descriptor after every checked arithmetic edge.
                expected.push_str("b\nc\nd\nconsumed\n");
                let project = Project::new();
                let built = project.build(&format!("package app\nimport koven.algorithms.{algorithm} as trim\nimport koven.algorithms.drop\nfun read(view:View<String>,expected:Int):Unit{{if(view.size!=expected){{error(\"size\")}};for(item in view){{println(item)}}}}\nfun consume(own root:List<String>):Unit{{println(\"consumed\")}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\",\"b\",\"c\",\"d\");borrow val parent=drop(root,1);val localCount=n-1;{checks}read(parent,3);consume(root)}}\nfun main():Unit{{scan(2)}}"));
                assert!(
                    built.status.success(),
                    "{algorithm}/{source}/{receiver}: {built:?}"
                );
                let run = Command::new(project.0.join("program")).output().unwrap();
                assert_eq!(run.status.code(), Some(0), "{run:?}");
                assert_eq!(run.stdout, expected.as_bytes());
                assert!(run.stderr.is_empty(), "{run:?}");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn public_std_range_scalar_count_abort_precedes_consumer_without_unwind() {
    use std::os::unix::process::ExitStatusExt;
    for algorithm in ["take", "drop", "dropLast"] {
        for (expression, n, divisor, message) in [
            ("n+1", "2147483647", "1", "integer overflow in +"),
            ("n-1", "-2147483648", "1", "integer overflow in -"),
            ("n*2", "2147483647", "1", "integer overflow in *"),
            ("n/divisor", "2", "0", "division by zero"),
            ("n%divisor", "2", "0", "division by zero"),
            ("n/divisor", "-2147483648", "-1", "integer overflow in /"),
            ("n%divisor", "-2147483648", "-1", "integer overflow in %"),
        ] {
            let project = Project::new();
            let built = project.build(&format!("package app\nimport koven.algorithms.{algorithm} as trim\nimport koven.algorithms.take\nclass Item(val text:String){{deinit(){{println(\"unwind\")}}}}\nfun read(view:View<Item>):Unit{{println(\"consumer\")}}\nfun scan(n:Int,divisor:Int):Unit{{val root=listOf(Item(\"a\"));borrow val parent=take(root,1);println(\"before\");read(parent.trim({expression}));println(\"after\")}}\nfun main():Unit{{scan({n},{divisor})}}"));
            assert!(
                built.status.success(),
                "{algorithm}/{expression}: {built:?}"
            );
            let run = Command::new(project.0.join("program")).output().unwrap();
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
            assert_eq!(run.stdout, b"before\n", "{run:?}");
            // SPEC-0355: the checked-count failure names its operator before SIGABRT.
            let report = String::from_utf8_lossy(&run.stderr);
            assert!(
                report.starts_with(&format!("abort: {message}\n  at ")),
                "{algorithm}/{expression}: {report:?}"
            );
        }
    }
}

#[test]
fn public_std_range_scalar_counts_keep_other_binary_capability_gates() {
    for (declaration, expression) in [("", "\"a\"+\"b\""), ("", "true == false"), ("", "1L == 2L")]
    {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take\n{declaration}\nfun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val parent=take(root,1);val other={expression};read(parent)}}\nfun main():Unit{{scan(2)}}"));
        assert!(!built.status.success(), "{expression}: {built:?}");
        assert!(
            String::from_utf8_lossy(&built.stderr).contains(
                "native object UnsupportedSource: frontend lowering failed with UnsupportedNode"
            ),
            "{expression}: {built:?}"
        );
        assert!(!project.0.join("program").exists());
    }
}

#[test]
fn public_std_range_scalar_counts_allow_literal_boolean_short_circuit() {
    for (expression, expected) in [("true && false", "false"), ("true || false", "true")] {
        let project = Project::new();
        // Preserve the two former negative expressions as local initializers.
        let built = project.build(&format!(
            r#"package app
import koven.algorithms.take
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun show(value:Boolean):Unit{{if(value){{println("true")}}else{{println("false")}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{
    val root=listOf("kept")
    {{borrow val parent=take(root,1);val other={expression};show(other);read(parent)}}
    consume(root)
}}
fun main():Unit{{scan(2)}}"#
        ));
        assert!(built.status.success(), "{expression}: {built:?}");
        assert!(built.stdout.is_empty(), "{built:?}");
        assert!(built.stderr.is_empty(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(
            run.stdout,
            format!("{expected}\nkept\nconsume\n").as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn check_int_comparison_at_binding_positions(expression: &str) {
    let comparison = format!("val other={expression};show(other)");
    for (position, body, expected) in [
        (
            "before",
            format!("{comparison};{{borrow val parent=take(root,1);read(parent)}}"),
            "true\nkept\nconsume\nfalse\nkept\nconsume\n",
        ),
        (
            "live",
            format!("{{borrow val parent=take(root,1);read(parent);{comparison};read(parent)}}"),
            "kept\ntrue\nkept\nconsume\nkept\nfalse\nkept\nconsume\n",
        ),
        (
            "after",
            format!("{{borrow val parent=take(root,1);read(parent)}};{comparison}"),
            "kept\ntrue\nconsume\nkept\nfalse\nconsume\n",
        ),
    ] {
        let project = Project::new();
        // show has no borrow binding; its branch does not exercise scan's if gate.
        // The local Boolean preserves the exact initializer form of the old negative.
        let built = project.build(&format!(
            r#"package app
import koven.algorithms.take
fun show(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{val root=listOf("kept","excluded");{body};consume(root)}}
fun main():Unit{{scan(2);scan(4)}}"#
        ));
        assert!(built.status.success(), "{expression}/{position}: {built:?}");
        assert!(built.stdout.is_empty(), "{built:?}");
        assert!(built.stderr.is_empty(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes(), "{expression}/{position}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_range_scalar_counts_allow_int_equality_at_all_binding_positions() {
    check_int_comparison_at_binding_positions("n == 2");
}

#[test]
fn public_std_range_scalar_counts_allow_int_less_than_at_all_binding_positions() {
    check_int_comparison_at_binding_positions("n < 3");
}

#[test]
fn public_std_range_scalar_counts_select_scoped_if_before_resource_consume() {
    for condition in ["mark(n) == 2", "mark(n) < 3"] {
        let project = Project::new();
        let built = project.build(&format!(
            r#"package app
import koven.algorithms.take
class Item(val text:String){{deinit(){{println("drop");println(this.text)}}}}
fun mark(n:Int):Int{{println("condition");return n}}
fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
fun consume(own root:List<Item>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{
    val root=listOf(Item("kept"),Item("excluded"))
    {{
        borrow val parent=take(root,1)
        if({condition}){{println("then");read(parent)}}else{{println("else");read(parent)}}
        read(parent)
    }}
    consume(root)
}}
fun main():Unit{{scan(2);scan(4)}}"#
        ));
        assert!(built.status.success(), "{condition}: {built:?}");
        assert!(built.stdout.is_empty(), "{built:?}");
        assert!(built.stderr.is_empty(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(
            run.stdout,
            b"condition\nthen\nkept\nkept\nconsume\ndrop\nexcluded\ndrop\nkept\ncondition\nelse\nkept\nkept\nconsume\ndrop\nexcluded\ndrop\nkept\n",
            "{condition}: {run:?}"
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}
