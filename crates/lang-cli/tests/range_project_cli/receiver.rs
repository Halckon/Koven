//! Public CLI coverage for the trusted std receiver wrappers, through real source loading.
use super::*;

#[test]
fn public_std_receiver_take_clips_counts_and_preserves_top_level_calls() {
    let project = Project::new();
    let built = project.build("package app\nimport koven.algorithms.take\nfun check(view:View<String>,size:Int):Unit{if(view.size!=size){error(\"size\")}}\nfun main():Unit{val source=listOf(\"first\",\"second\");borrow val empty=source.take(0);check(empty,0);borrow val all=(source).take(2147483647);check(all,2);borrow val zero=all.take(0);check(zero,0);borrow val first=all.take(1);check(first,1);for(item in take(first,2147483647)){println(item)};val no=listOf<String>();borrow val none=no.take(2147483647);check(none,0)}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"first\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn public_std_receiver_take_temporary_chain_preserves_string_move_only_and_resource() {
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "second\nfirst\n",
        ),
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take as prefix\n{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}};println(\"read-end\")}}\nfun main():Unit{{read(source().prefix(2147483647).prefix(1));println(\"done\")}}"));
        assert!(built.status.success(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(
            run.stdout,
            format!("source\nfirst\nread-end\n{drops}done\n").as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[cfg(unix)]
#[test]
fn public_std_receiver_take_negative_count_aborts_for_list_view_and_empty_source() {
    use std::os::unix::process::ExitStatusExt;
    for (values, operand) in [
        ("\"first\"", "source.take(-1)"),
        ("", "source.take(-1)"),
        ("\"first\"", "source.take(1).take(-1)"),
        ("", "source.take(0).take(-1)"),
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{{println(\"unreachable\")}}\nfun main():Unit{{val source:List<String> = listOf<String>({values});read({operand})}}"));
        assert!(
            built.status.success(),
            "values={values:?}, operand={operand}: {built:?}"
        );
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.signal(), Some(6), "{run:?}");
        assert!(run.stdout.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_receiver_take_named_root_immediate_borrow_matches_top_level() {
    for (label, operation, read) in [
        ("top-level unused", "take(source,1)", "println(\"read\")"),
        ("receiver unused", "source.take(1)", "println(\"read\")"),
        (
            "receiver used",
            "source.take(1)",
            "if(view.size!=1){error(\"size\")};println(\"read\")",
        ),
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{{{read}}}\nfun main():Unit{{val source=listOf(\"first\");read({operation})}}"));
        assert!(built.status.success(), "{label}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
        assert_eq!(run.stdout, b"read\n");
    }
}
