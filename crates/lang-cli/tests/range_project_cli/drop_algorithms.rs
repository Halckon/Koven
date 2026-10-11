//! Public std count algorithms use real Koven bodies and the checked range ABI.
use super::*;

#[test]
fn public_std_drop_algorithms_clip_list_view_and_receiver_counts() {
    for algorithm in ["drop", "dropLast"] {
        for receiver in [false, true] {
            let call = |source: &str, count: i32| {
                if receiver {
                    format!("{source}.trim({count})")
                } else {
                    format!("trim({source},{count})")
                }
            };
            let mut checks = String::new();
            let mut expected = String::new();
            for source in ["source", "parent", "empty", "emptyView"] {
                let size = if source.starts_with("empty") { 0 } else { 3 };
                for count in [0, 1, 3, 4, i32::MAX] {
                    let clipped = count.min(size) as usize;
                    let values = if source.starts_with("empty") {
                        &[][..]
                    } else {
                        &["a", "b", "c"][..]
                    };
                    let kept = if algorithm == "drop" {
                        &values[clipped..]
                    } else {
                        &values[..values.len() - clipped]
                    };
                    for value in kept {
                        expected.push_str(value);
                        expected.push('\n');
                    }
                    checks.push_str(&format!(
                        "read({},{});",
                        call(source, count),
                        size - count.min(size)
                    ));
                }
            }
            let project = Project::new();
            let built = project.build(&format!("package app\nimport koven.algorithms.{algorithm} as trim\nimport koven.algorithms.take\nfun read(view:View<String>,size:Int):Unit{{if(view.size!=size){{error(\"size\")}};for(item in view){{println(item)}}}}\nfun main():Unit{{val source=listOf(\"a\",\"b\",\"c\");borrow val parent=take(source,3);val empty=listOf<String>();borrow val emptyView=take(source,0);{checks};println(\"done\")}}"));
            assert!(
                built.status.success(),
                "{algorithm}, receiver={receiver}: {built:?}"
            );
            let run = Command::new(project.0.join("program")).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            expected.push_str("done\n");
            assert_eq!(run.stdout, expected.as_bytes());
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_drop_algorithms_nonzero_offset_preserves_elements_and_root_once() {
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"a\".clone(),\"b\".clone(),\"c\".clone(),\"d\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\"),Item(\"d\")",
            "println(item.text)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\"),Item(\"d\")",
            "println(item.text)",
            "d\nc\nb\na\n",
        ),
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\nimport koven.algorithms.take\n{declaration}\nfun source():List<{element}>{{return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own root:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val root=source();borrow val parent=drop(root,1);borrow val child=parent.dropLast(1);borrow val sibling=dropLast(parent,0);borrow val empty=parent.drop(2147483647);read(child);read(sibling);read(empty);read(take(child,1));println(\"after\");consume(root);read(source().drop(1).dropLast(1));println(\"done\")}}"));
        assert!(built.status.success(), "{element}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(
            run.stdout,
            format!("b\nc\nb\nc\nd\nb\nafter\nconsume\n{drops}b\nc\n{drops}done\n").as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[cfg(unix)]
#[test]
fn public_std_drop_algorithms_negative_counts_abort_even_for_empty_views() {
    use std::os::unix::process::ExitStatusExt;
    for algorithm in ["drop", "dropLast"] {
        for values in ["\"a\"", ""] {
            for count in ["-1", "-2147483648"] {
                for operand in [
                    format!("{algorithm}(source,{count})"),
                    format!("source.{algorithm}({count})"),
                    format!("{algorithm}(parent,{count})"),
                    format!("parent.{algorithm}({count})"),
                    format!("{algorithm}(emptyView,{count})"),
                    format!("emptyView.{algorithm}({count})"),
                ] {
                    let project = Project::new();
                    let built = project.build(&format!("package app\nimport koven.algorithms.{algorithm}\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{{println(\"unreachable\")}}\nfun main():Unit{{val source=listOf<String>({values});borrow val parent=take(source,1);borrow val emptyView=take(source,0);read({operand})}}"));
                    assert!(built.status.success(), "{operand}: {built:?}");
                    let run = Command::new(project.0.join("program")).output().unwrap();
                    assert_eq!(run.status.signal(), Some(6), "{run:?}");
                    assert!(run.stdout.is_empty(), "{run:?}");
                }
            }
        }
    }
}

#[test]
fn public_std_drop_algorithms_keep_empty_child_and_sibling_roots_protected() {
    for pending in ["parent", "child", "sibling", "empty"] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\nfun read(view:View<String>):Unit{{if(view.size==0){{println(\"empty\")}}}}\nfun consume(own root:List<String>):Unit{{}}\nfun main():Unit{{val root=listOf(\"a\",\"b\");borrow val parent=drop(root,1);borrow val child=parent.dropLast(0);borrow val sibling=dropLast(parent,0);borrow val empty=parent.drop(2147483647);consume(root);read({pending})}}"));
        assert!(!built.status.success(), "{pending}: {built:?}");
        assert!(
            String::from_utf8_lossy(&built.stderr).contains("L0135"),
            "{pending}: {built:?}"
        );
        assert!(!project.0.join("program").exists());
    }
}

#[test]
fn public_std_drop_algorithms_do_not_authorize_same_named_user_sources() {
    for declaration in [
        "fun <T> drop(source:List<T>,count:Int):View<T> from source=rangeView(source,count,source.size)",
        "borrow fun <T> List<T>.dropLast(count:Int):View<T> from this=rangeView(this,0,this.size-count)",
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\n{declaration}\nfun main():Unit{{}}"));
        assert!(!built.status.success(), "{built:?}");
        assert!(
            String::from_utf8_lossy(&built.stderr).contains("L0164"),
            "{built:?}"
        );
        assert!(!project.0.join("program").exists());
    }
}

#[test]
fn public_std_drop_algorithms_keep_existing_inline_binary_count_capability_gate() {
    for algorithm in ["take", "drop", "dropLast"] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.{algorithm}\nfun read(view:View<String>):Unit{{}}\nfun main():Unit{{val source=listOf(\"a\");borrow val parent={algorithm}(source,0);read({algorithm}(source,(-2147483647-1)))}}"));
        assert!(!built.status.success(), "{built:?}");
        assert!(
            String::from_utf8_lossy(&built.stderr).contains(
                "native object UnsupportedSource: frontend lowering failed with UnsupportedNode"
            ),
            "{built:?}"
        );
        assert!(!project.0.join("program").exists());
    }
}
