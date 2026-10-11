//! Normal source programs only: checked count arithmetic keeps range roots live.
use super::*;

const IMPORTS: &str = "package app\nimport koven.algorithms.take\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n";

fn programs(consumer: &str) -> [(Program, FunctionId); 3] {
    [
        single_with_entry(consumer),
        unit_with_provider_order(&format!("{IMPORTS}{consumer}"), "", false),
        unit_with_provider_order(&format!("{IMPORTS}{consumer}"), "", true),
    ]
}

#[test]
fn range_scalar_counts_single_and_unit_transport_live_parent_child_roots() {
    let consumer = r#"
class Item(val text:String){deinit(){println(this.text)}}
fun source():List<Item>{println("source");return listOf(Item("a"),Item("b"),Item("c"),Item("d"))}
fun count(n:Int):Int{println("count");return n}
fun read(view:View<Item>):Unit{for(item in view){println(item.text)}}
fun consume(own root:List<Item>):Unit{println("consume")}
fun scan(n:Int):Unit{
    val root=source()
    borrow val parent=take(root,count(n)-1)
    borrow val child=parent.drop(count(n)/2-1)
    borrow val sibling=dropLast(parent,count(n)%3)
    read(child.take(count(n)*1-2))
    read(take(sibling,count(n)+0))
    read(root.dropLast(root.size-1))
    read(drop(child,child.size-1))
    read(parent)
    consume(root)
    println("done")
}
fun main():Unit{scan(4)}
"#;
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let module = &program.modules[0];
        let function = module
            .functions
            .iter()
            .find(|function| {
                function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        matches!(instruction.operation, Operation::CheckedArithmetic { .. })
                    })
                    .count()
                    >= 8
            })
            .expect("dynamic consumer must retain its checked arithmetic");
        let mut transports_parent_and_child = false;
        for block in &function.blocks {
            if !block.instructions.iter().any(|id| {
                matches!(
                    function.instructions[id.index()].operation,
                    Operation::CheckedArithmetic { .. }
                )
            }) {
                continue;
            }
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref().unwrap().kind
            else {
                panic!("checked branch")
            };
            assert!(matches!(
                function.blocks[when_true.target.index()]
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                TerminatorKind::Abort
            ));
            let parameters = &function.blocks[when_false.target.index()].parameters;
            let descriptors = parameters.iter().filter(|entity| matches!(
                function.entity(**entity).unwrap().ty,
                EntityType::Value(ty) if matches!(module.type_kind(ty), Some(SsaTypeKind::RangeView { .. }))
            )).count();
            let loans = parameters
                .iter()
                .filter(|entity| matches!(entity, EntityId::Loan(_)))
                .count();
            transports_parent_and_child |= descriptors >= 2 && loans >= 2;
        }
        assert!(
            transports_parent_and_child,
            "checked success edges must carry live descriptors and source loans"
        );
        let run = run_unmodified(&program, entry, &format!("live-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"source\ncount\ncount\ncount\ncount\nb\nc\ncount\na\nb\na\nc\na\nb\nc\nconsume\nd\nc\nb\na\ndone\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn run_unmodified(program: &Program, entry: FunctionId, label: &str) -> std::process::Output {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!(
        "koven-range-scalar-counts-{}-{label}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let ir = directory.join("program.ll");
    let executable = directory.join("program");
    let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    fs::remove_dir_all(directory).unwrap();
    run
}

#[cfg(unix)]
#[test]
fn range_scalar_counts_normal_source_arithmetic_and_negative_counts_abort() {
    use std::os::unix::process::ExitStatusExt;
    for (case, expression, n) in [
        ("add-overflow", "n+1", "2147483647"),
        ("subtract-overflow", "n-1", "-2147483648"),
        ("multiply-overflow", "n*2", "2147483647"),
        ("divide-zero", "1/n", "0"),
        ("remainder-zero", "1%n", "0"),
        ("divide-overflow", "n/-1", "-2147483648"),
        ("remainder-minimum", "n%-1", "-2147483648"),
        ("negative-count", "n-2", "1"),
        ("minimum-expression", "(-2147483647-1)", "0"),
    ] {
        let consumer = format!(
            r#"
class Item(val text:String){{deinit(){{println(this.text)}}}}
fun read(view:View<Item>):Unit{{println("unreachable")}}
fun scan(n:Int):Unit{{
    val root=listOf(Item("dropped"))
    borrow val parent=take(root,1)
    borrow val child=parent.dropLast(0)
    read(drop(child,{expression}))
    read(parent)
}}
fun main():Unit{{scan({n})}}
"#
        );
        for (index, (program, entry)) in programs(&consumer).into_iter().enumerate() {
            let run = run_unmodified(&program, entry, &format!("{case}-{index}"));
            assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
            assert!(
                run.stdout.is_empty(),
                "consumer and cleanup must not run: {run:?}"
            );
        }
    }
}

#[test]
fn range_scalar_counts_keep_non_int_binary_and_wider_cfg_gates() {
    for (statement, fragment) in [
        ("val other=1L+2L", "1L+2L"),
        ("val other=\"a\"+\"b\"", "\"a\"+\"b\""),
        ("val other=n==1", "n==1"),
        ("val other=n shl 1", "n shl 1"),
        ("val other=true && false", "true && false"),
        ("if(true){read(part)}", "if(true){read(part)}"),
        ("while(false){read(part)}", "while(false){read(part)}"),
        ("return", "return"),
    ] {
        let consumer = format!(
            "fun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val part=take(root,1);{statement};read(part)}}\nfun main():Unit{{scan(1)}}"
        );
        assert_source_rejected(&consumer, fragment);
    }
}

#[test]
fn range_scalar_counts_do_not_widen_ordinary_borrow_or_metadata_alias_bindings() {
    for binding in [
        "val text=\"kept\";borrow val alias=text;println(alias)",
        "borrow val alias=part;read(alias)",
    ] {
        let consumer = format!(
            "fun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val part=take(root,1);{binding};read(take(part,n-1));read(part)}}\nfun main():Unit{{scan(1)}}"
        );
        assert_source_rejected(&consumer, "n-1");
    }
}

fn assert_source_rejected(consumer: &str, fragment: &str) {
    use lang_frontend::{
        name_resolution::resolve_names, ownership_checking::check_ownership,
        type_checking::check_types,
    };
    let mut sources = SourceMap::new();
    let text = format!(
        "package app\n{}\n{consumer}",
        include_str!("../../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(
        &mut sources,
        "range-scalar-frontier.ko",
        &text,
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "main")
        .unwrap()
        .id();
    let error = crate::ssa::lower_frontend::orchestrate::lower_scalar_file_with_entry(
        &sources, &parsed, &names, &typed, &owned, entry,
    )
    .err()
    .expect("the capability gate must reject this source");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
    let inputs = [SourceUnitInput::new(
        "app",
        "app/frontier.ko",
        source,
        &parsed,
    )];
    let (names, typed, owned) =
        crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let error = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        crate::ssa::unit_lower_test_support::declaration(&names, "app", "main"),
    )
    .err()
    .expect("the unit capability gate must reject this source");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
}

#[test]
fn range_scalar_counts_pending_temporary_source_and_descriptor_evaluate_once() {
    let consumer = r#"
class Item(val text:String){deinit(){println(this.text)}}
fun source():List<Item>{println("source");return listOf(Item("a"),Item("b"))}
fun count(n:Int):Int{println("count");return n}
fun read(view:View<Item>,n:Int):Unit{println("read");for(item in view){println(item.text)}}
fun scan(n:Int):Unit{
    val root=listOf(Item("root"))
    borrow val parent=take(root,1)
    read(take(source(),count(n)-1),count(n)+0)
    read(source().dropLast(count(n)-1),count(n)*1)
    read(parent,count(n)%2)
    println("done")
}
fun main():Unit{scan(2)}
"#;
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let run = run_unmodified(&program, entry, &format!("pending-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"source\ncount\ncount\nread\na\nb\na\nsource\ncount\ncount\nread\na\nb\na\ncount\nread\nroot\ndone\nroot\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}
