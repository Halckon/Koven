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
        for (expression, n, divisor) in [
            ("n+1", "2147483647", "1"),
            ("n-1", "-2147483648", "1"),
            ("n*2", "2147483647", "1"),
            ("n/divisor", "2", "0"),
            ("n%divisor", "2", "0"),
            ("n/divisor", "-2147483648", "-1"),
            ("n%divisor", "-2147483648", "-1"),
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
        }
    }
}

#[test]
fn public_std_range_scalar_counts_keep_other_binary_capability_gates() {
    for (declaration, expression) in [
        ("", "true && false"),
        ("", "true || false"),
        ("", "\"a\"+\"b\""),
        ("", "n == 2"),
        ("", "n < 3"),
    ] {
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
