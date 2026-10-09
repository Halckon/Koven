//! Public project take compiles real trusted std bodies and exercises the native descriptor ABI.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-range-project-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("src/app")).unwrap();
        fs::write(path.join("project.toml"), "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"range\"\nsource-roots = [\"src\"]\n").unwrap();
        Self(path)
    }
    fn build(&self, source: &str) -> std::process::Output {
        fs::write(self.0.join("src/app/Main.ko"), source).unwrap();
        Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .arg("build")
            .arg("--project")
            .arg(self.0.join("project.toml"))
            .arg("--entry")
            .arg("app.main")
            .arg("-o")
            .arg(self.0.join("program"))
            .output()
            .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn public_std_take_temporary_source_is_alive_through_immediate_borrow() {
    let project = Project::new();
    let built = project.build("package app\nimport koven.algorithms.take\nclass Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun read(view:View<Token>):Unit{for(item in view){println(item.text)};println(\"read-end\")}\nfun main():Unit{read(take(source(),1));println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        run.stdout,
        b"source\nfirst\nread-end\nsecond\nfirst\ndone\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn public_std_take_temporary_for_keeps_root_until_provider_finishes() {
    let project = Project::new();
    let built = project.build("package app\nimport koven.algorithms.take\nclass Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun main():Unit{for(item in take(source(),1)){println(item.text)};println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"source\nfirst\nsecond\nfirst\ndone\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn public_std_take_for_reads_string_move_only_and_resource_elements() {
    for (declaration, element, values, read, expected) in [
        (
            "",
            "String",
            "\"first\",\"second\",\"excluded\"",
            "println(item)",
            "first\nsecond\nafter\ndone\n",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\"),Item(\"excluded\")",
            "println(item.text)",
            "first\nsecond\nafter\ndone\n",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\"),Item(\"excluded\")",
            "println(item.text)",
            "first\nsecond\nafter\nexcluded\nsecond\nfirst\ndone\n",
        ),
    ] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take\n{declaration}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{}}\nfun main():Unit{{val source=listOf({values});borrow val part=take(source,2);read(part);println(\"after\");consume(source);println(\"done\")}}"));
        assert!(built.status.success(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_take_named_view_for_keeps_root_until_provider_finishes() {
    let project = Project::new();
    let built=project.build("package app\nimport koven.algorithms.take\nclass Token(val text:String){deinit(){println(this.text)}}\nfun consume(own source:List<Token>):Unit{}\nfun main():Unit{val source=listOf(Token(\"first\"),Token(\"excluded\"));{borrow val part=take(source,1);for(item in part){println(item.text)}};println(\"after\");consume(source);println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"first\nafter\nexcluded\nfirst\ndone\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn public_std_take_for_empty_break_continue_and_return_use_provider_cleanup() {
    for (values, count, body, expected) in [
        (
            "\"first\",\"second\"",
            0,
            "error(\"empty body\")",
            "after\nsecond\nfirst\ndone\n",
        ),
        (
            "\"first\",\"second\"",
            2,
            "println(item.text);break",
            "first\nafter\nsecond\nfirst\ndone\n",
        ),
        (
            "\"first\",\"second\"",
            2,
            "if(item.number==1){continue};println(item.text)",
            "second\nafter\nsecond\nfirst\ndone\n",
        ),
        (
            "\"first\",\"second\"",
            2,
            "println(item.text);return",
            "first\nafter\nsecond\nfirst\ndone\n",
        ),
        (
            "\"first\",\"second\"",
            2,
            "val a=Token(0,\"a\");val b=Token(0,\"b\");if(item.number==1){continue};touch(a);touch(b)",
            "b\na\nb\na\nafter\nsecond\nfirst\ndone\n",
        ),
    ] {
        let project = Project::new();
        let values = values
            .split(',')
            .enumerate()
            .map(|(index, value)| format!("Token({},{value})", index + 1))
            .collect::<Vec<_>>()
            .join(",");
        let built=project.build(&format!("package app\nimport koven.algorithms.take\nclass Token(val number:Int,val text:String){{deinit(){{println(this.text)}}}}\nfun touch(token:Token):Unit{{}}\nfun read(view:View<Token>):Unit{{for(item in view){{{body}}}}}\nfun consume(own source:List<Token>):Unit{{}}\nfun main():Unit{{val source=listOf({values});borrow val part=take(source,{count});read(part);println(\"after\");consume(source);println(\"done\")}}"));
        assert!(built.status.success(), "body {body}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "body {body}: {run:?}");
        assert_eq!(run.stdout, expected.as_bytes(), "body {body}: {run:?}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}
#[test]
fn public_std_take_returns_an_inline_range_and_reads_size_without_copying_elements() {
    for (element, value) in [("String", "\"kept\""), ("Item", "Item(7)")] {
        let project = Project::new();
        let output = project.build(&format!("package app\nimport koven.algorithms.take\nclass Item(val number: Int) {{}}\nfun check(source: View<{element}>, expected: Int): Unit {{ if (source.size != expected) {{ error(\"wrong range size\") }} }}\nfun main(): Unit {{ val source = listOf({value}); borrow val part = take(source, 2147483647); check(part, 1); println(\"done\") }}"));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{stderr}");
        assert!(output.stdout.is_empty());
        let executed = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(executed.status.code(), Some(0), "{executed:?}");
        assert_eq!(executed.stdout, b"done\n");
        assert!(executed.stderr.is_empty(), "{executed:?}");
    }
}

#[test]
fn public_std_take_clips_zero_empty_and_int_max_counts() {
    for (source, count, expected) in [
        ("listOf(\"a\",\"b\")", 0, 0),
        ("listOf(\"a\",\"b\")", 1, 1),
        ("listOf(\"a\",\"b\")", 2, 2),
        ("listOf(\"a\",\"b\")", 2147483647, 2),
        ("listOf<String>()", 2147483647, 0),
    ] {
        let project = Project::new();
        let built=project.build(&format!("package app\nimport koven.algorithms.take\nfun check(view:View<String>):Unit {{if(view.size!={expected}){{error(\"size\")}}}}\nfun main():Unit{{val source={source};borrow val part=take(source,{count});check(part);println(\"done\")}}"));
        assert!(built.status.success(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"done\n");
        assert!(run.stderr.is_empty());
    }
}

#[test]
fn public_std_take_resource_root_is_consumable_after_descriptor_ends() {
    let project = Project::new();
    let built=project.build("package app\nimport koven.algorithms.take\nclass Token(val name:String){deinit(){println(this.name)}}\nfun check(view:View<Token>):Unit{if(view.size!=1){error(\"size\")}}\nfun consume(own source:List<Token>):Unit{println(\"consume\")}\nfun main():Unit{val source=listOf(Token(\"resource\"));borrow val part=take(source,1);check(part);println(\"after\");consume(source);println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"after\nconsume\nresource\ndone\n");
    assert!(run.stderr.is_empty());
}

#[test]
fn public_std_take_negative_count_aborts_from_healthy_source() {
    let project = Project::new();
    let built=project.build("package app\nimport koven.algorithms.take\nfun check(view:View<String>):Unit{}\nfun main():Unit{val source=listOf(\"a\");borrow val part=take(source,-1);check(part);println(\"unreachable\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run.status.signal(), Some(6), "{run:?}");
    }
    assert!(run.stdout.is_empty(), "{run:?}");
}

#[test]
fn public_std_take_metadata_alias_and_borrow_return_keep_the_resource_root_alive() {
    let project = Project::new();
    let built=project.build("package app\nimport koven.algorithms.take\nclass Token(val name:String){deinit(){println(this.name)}}\nfun metadata(source:View<Token>):borrow View<Token> from source=source\nfun check(source:View<Token>):Unit{if(source.size!=1){error(\"size\")}}\nfun consume(own source:List<Token>):Unit{println(\"consume\")}\nfun main():Unit{val source=listOf(Token(\"resource\"));borrow val part=take(source,1);borrow val alias=part;borrow val returned=metadata(alias);check(returned);println(\"after\");consume(source);println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"after\nconsume\nresource\ndone\n");
    assert!(run.stderr.is_empty());
}

#[test]
fn public_std_take_temporary_for_cleans_all_exit_paths() {
    for (count, body, expected) in [
        (
            0,
            "error(\"empty\")",
            "source\nsecond\nfirst\nafter\ndone\n",
        ),
        (
            2,
            "println(item.text)",
            "source\nfirst\nsecond\nsecond\nfirst\nafter\ndone\n",
        ),
        (
            2,
            "println(item.text);break",
            "source\nfirst\nsecond\nfirst\nafter\ndone\n",
        ),
        (
            2,
            "if(item.number==1){continue};println(item.text)",
            "source\nsecond\nsecond\nfirst\nafter\ndone\n",
        ),
        (
            2,
            "println(item.text);return",
            "source\nfirst\nsecond\nfirst\ndone\n",
        ),
        (
            2,
            "val a=Token(0,\"a\");val b=Token(0,\"b\");if(item.number==1){continue};touch(a);touch(b)",
            "source\nb\na\nb\na\nsecond\nfirst\nafter\ndone\n",
        ),
    ] {
        let project = Project::new();
        let built=project.build(&format!("package app\nimport koven.algorithms.take\nclass Token(val number:Int,val text:String){{deinit(){{println(this.text)}}}}\nfun touch(token:Token):Unit{{}}\nfun source():List<Token>{{println(\"source\");return listOf(Token(1,\"first\"),Token(2,\"second\"))}}\nfun scan():Unit{{for(item in take(source(),{count})){{{body}}};println(\"after\")}}\nfun main():Unit{{scan();println(\"done\")}}"));
        assert!(built.status.success(), "body {body}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes(), "body {body}: {run:?}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_take_temporary_string_and_move_only_keep_both_consumers_alive() {
    for (declaration, element, values, read) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
        ),
    ] {
        for count in [0, 1, 2147483647] {
            let project = Project::new();
            let built=project.build(&format!("package app\nimport koven.algorithms.take\n{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun main():Unit{{read(take(source(),{count}));for(item in take(source(),{count})){{{read}}};println(\"done\")}}"));
            assert!(built.status.success(), "{element}/{count}: {built:?}");
            let run = Command::new(project.0.join("program")).output().unwrap();
            let items = match count {
                0 => "",
                1 => "first\n",
                _ => "first\nsecond\n",
            };
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(
                run.stdout,
                format!("source\n{items}source\n{items}done\n").as_bytes()
            );
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_take_temporary_sequential_descriptors_keep_distinct_roots() {
    let project = Project::new();
    let built=project.build("package app\nimport koven.algorithms.take\nclass Token(val text:String){deinit(){println(this.text)}}\nfun source(own text:String):List<Token>{println(\"source\");return listOf(Token(text))}\nfun read(view:View<Token>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{read(take(source(\"a\"),1));read(take(source(\"b\"),1));for(item in take(source(\"c\"),1)){println(item.text)};for(item in take(source(\"d\"),1)){println(item.text)};println(\"done\")}");
    assert!(built.status.success(), "{built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        run.stdout,
        b"source\na\na\nsource\nb\nb\nsource\nc\nc\nsource\nd\nd\ndone\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn public_std_take_immediate_named_source_remains_alive_until_consumer_ends() {
    for use_site in [
        "read(take(source,1))",
        "for(item in take(source,1)){println(item.text)}",
    ] {
        let project = Project::new();
        let built=project.build(&format!("package app\nimport koven.algorithms.take\nclass Token(val text:String){{deinit(){{println(this.text)}}}}\nfun read(view:View<Token>):Unit{{for(item in view){{println(item.text)}};println(\"read-end\")}}\nfun consume(own source:List<Token>):Unit{{}}\nfun main():Unit{{val source=listOf(Token(\"first\"),Token(\"second\"));{use_site};consume(source);println(\"done\")}}"));
        assert!(built.status.success(), "{use_site}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        let read_end = if use_site.starts_with("read") {
            "read-end\n"
        } else {
            ""
        };
        assert_eq!(
            run.stdout,
            format!("first\n{read_end}second\nfirst\ndone\n").as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_take_temporary_empty_source_finishes_without_elements() {
    for (declaration, element) in [
        ("", "String"),
        (
            "class Token(){deinit(){error(\"no element exists\")}}",
            "Token",
        ),
    ] {
        let project = Project::new();
        let built=project.build(&format!("package app\nimport koven.algorithms.take\n{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf<{element}>()}}\nfun read(view:View<{element}>):Unit{{if(view.size!=0){{error(\"size\")}};for(item in view){{error(\"empty body\")}}}}\nfun main():Unit{{read(take(source(),2147483647));for(item in take(source(),2147483647)){{error(\"empty body\")}};println(\"done\")}}"));
        assert!(built.status.success(), "{element}: {built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"source\nsource\ndone\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn public_std_take_view_source_composes_generic_elements_and_releases_root_once() {
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
        for count in [0, 1, 2147483647] {
            let project = Project::new();
            let built = project.build(&format!("package app\nimport koven.algorithms.take\n{declaration}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val source=listOf({values});borrow val parent=take(source,1);borrow val child=take(parent,{count});read(child);println(\"after\");consume(source);println(\"done\")}}"));
            assert!(built.status.success(), "{element}/{count}: {built:?}");
            let run = Command::new(project.0.join("program")).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            let visits = if count == 0 { "" } else { "first\n" };
            assert_eq!(
                run.stdout,
                format!("{visits}after\nconsume\n{drops}done\n").as_bytes()
            );
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_take_view_source_empty_and_negative_use_the_real_std_body() {
    for (values, count) in [("listOf<String>()", 2147483647), ("listOf(\"a\")", -1)] {
        let project = Project::new();
        let built = project.build(&format!("package app\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{{if(view.size!=0){{error(\"size\")}}}}\nfun main():Unit{{val source={values};borrow val parent=take(source,1);borrow val child=take(parent,{count});read(child);println(\"done\")}}"));
        assert!(built.status.success(), "{built:?}");
        let run = Command::new(project.0.join("program")).output().unwrap();
        if count < 0 {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(run.status.signal(), Some(6), "{run:?}");
            }
            assert!(run.stdout.is_empty(), "{run:?}");
        } else {
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, b"done\n");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_take_call_prefix_return_restores_named_and_temporary_roots() {
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
        for (prefix, operand, close, after, named) in [
            (
                "val root=source()",
                "take(root,1)",
                "",
                "consume(root)",
                true,
            ),
            ("", "take(source(),1)", "", "", false),
            (
                "val root=source();{borrow val parent=take(root,2)",
                "take(parent,1)",
                "}",
                "consume(root)",
                true,
            ),
            ("", "take(take(source(),2),1)", "", "", false),
        ] {
            let project = Project::new();
            let source = format!(
                "package app\nimport koven.algorithms.take\n{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun consume(own source:List<{element}>):Unit{{}}\nfun read(view:View<{element}>,number:Int):Unit{{for(item in view){{{read}}};println(\"called\")}}\nfun scan(stop:Boolean):Unit{{{prefix};read({operand},if(stop){{return}}else{{0}});{close}println(\"after\");{after}}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
            );
            let built = project.build(&source);
            assert!(built.status.success(), "{element}, {operand}: {built:?}");
            let run = Command::new(project.0.join("program")).output().unwrap();
            let normal = if named {
                format!("first\ncalled\nafter\n{drops}")
            } else {
                format!("first\ncalled\n{drops}after\n")
            };
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(
                run.stdout,
                format!("source\n{normal}source\n{drops}done\n").as_bytes(),
                "{run:?}"
            );
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_take_call_prefix_owned_arguments_remain_with_caller_until_commit() {
    for (declaration, element, values, sent, later, read, root_drops, sent_drop, later_drop) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "\"sent\".clone()",
            "\"later\".clone()",
            "println(item)",
            "",
            "",
            "",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "Item(\"sent\")",
            "Item(\"later\")",
            "println(item.text)",
            "",
            "",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "Item(\"sent\")",
            "Item(\"later\")",
            "println(item.text)",
            "second\nfirst\n",
            "sent\n",
            "later\n",
        ),
    ] {
        for (prefix, operand, close, after, named) in [
            (
                "val root=source()",
                "take(root,1)",
                "",
                "consume(root)",
                true,
            ),
            ("", "take(source(),1)", "", "", false),
            (
                "val root=source();{borrow val parent=take(root,2)",
                "take(parent,1)",
                "}",
                "consume(root)",
                true,
            ),
            ("", "take(take(source(),2),1)", "", "", false),
        ] {
            let project = Project::new();
            let source = format!(
                "package app\nimport koven.algorithms.take\n{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun sent():List<{element}>{{println(\"sent-source\");return listOf({sent})}}\nfun later():List<{element}>{{println(\"late-source\");return listOf({later})}}\nfun consume(own source:List<{element}>):Unit{{}}\nfun read(view:View<{element}>,own sent:List<{element}>,number:Int,own later:List<{element}>):Unit{{for(item in view){{{read}}};println(\"called\");consume(sent);consume(later)}}\nfun scan(stop:Boolean):Unit{{{prefix};read({operand},sent(),if(stop){{return}}else{{0}},later());{close}println(\"after\");{after}}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
            );
            let built = project.build(&source);
            assert!(built.status.success(), "{element}, {operand}: {built:?}");
            let run = Command::new(project.0.join("program")).output().unwrap();
            let normal = if named {
                format!("{sent_drop}{later_drop}after\n{root_drops}")
            } else {
                format!("{sent_drop}{later_drop}{root_drops}after\n")
            };
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, format!("source\nsent-source\nlate-source\nfirst\ncalled\n{normal}source\nsent-source\n{sent_drop}{root_drops}done\n").as_bytes(), "{run:?}");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}
