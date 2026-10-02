//! Cross-file resource exchange through both public native unit entries.

use super::*;
use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

const PROVIDER: &str = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun consume(own guard: Guard): Unit { println("consume old") }
"#;

#[test]
fn unit_resource_exchange_native_commit_preserves_lexical_owners_and_unique_frees() {
    let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit {
    var first = Guard("old body", Leaf("old first"), Leaf("old second"))
    var second = Guard("second body", Leaf("second first"), Leaf("second second"))
    val old = replace(&first, p.replacement())
    println("after replace")
    p.consume(old)
    swap(&first, &second)
    println("after swap")
}
"#;
    let expected = b"evaluate new\nafter replace\nconsume old\nold body\nold second\nold first\nafter swap\nnew body\nnew second\nnew first\nsecond body\nsecond second\nsecond first\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(PROVIDER, consumer, constants);
        crate::native_tests::boxed_enum_tests::assert_success(&run, expected);
        let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&ir, 9);
        crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
    }
}

#[test]
fn unit_resource_exchange_native_returned_and_borrowed_old_owner_drop_once() {
    let provider = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
fun takeOld(): Leaf {
    var target = Leaf("new root")
    return replace(&target, Leaf("replacement root"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
"#;
    let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val returned = p.takeOld()
    println("received old")
    var target = Leaf("borrowed old")
    p.inspect(replace(&target, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"replacement root\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nnew root\n",
        );
    }
}

#[test]
fn unit_resource_exchange_native_pending_exit_keeps_old_root_initialized() {
    for exit in ["return", "break", "continue"] {
        let provider = format!(
            r#"package p
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
fun exercise(own flag: Boolean): Unit {{
    var target = Leaf("old")
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&target, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.exercise(true); p.exercise(false); println(\"done\") }";
        // The base unit entry retains its existing argument-control-transfer boundary.
        let (run, _) = run_unit(&provider, consumer, true);
        let expected = if exit == "return" {
            "old\ncommitted\nold\nafter loop\nnew\ndone\n"
        } else {
            "after loop\nold\ncommitted\nold\nafter loop\nnew\ndone\n"
        };
        crate::native_tests::boxed_enum_tests::assert_success(&run, expected.as_bytes());
    }
}

#[test]
fn unit_resource_exchange_native_abort_does_not_run_deinit() {
    let provider = "package p\nclass Leaf { deinit() { println(\"unexpected deinit\") } }";
    let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    var target = Leaf()
    println("before abort")
    val old = replace(&target, error("stop"))
    println("unexpected commit")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
        }
    }
}

pub(super) fn run_unit(
    provider: &str,
    consumer: &str,
    constants: bool,
) -> (std::process::Output, String) {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    let directory = TestDirectory::create();
    let object = directory.join("resource-exchange.o");
    let executable = directory.join("resource-exchange");
    let mut first_ir = None;
    let mut first_object = None;
    if constants {
        let typed = result
            .validate_constants()
            .expect("constant-capable resource types");
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .expect("constant-capable resource ownership");
        for order in [inputs, [inputs[1], inputs[0]]] {
            let (program, function) =
                crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                )
                .expect("constant unit resource SSA");
            let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                .expect("constant unit resource LLVM");
            crate::emit_native_constant_unit_object(
                &sources,
                &order,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .expect("public constant unit object");
            assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
        }
    } else {
        let typed = result.validate().expect("resource types");
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .expect("resource ownership");
        for order in [inputs, [inputs[1], inputs[0]]] {
            let (program, function) = lower_scalar_unit_with_entry(
                &sources,
                &order,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
            )
            .expect("unit resource SSA");
            let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                .expect("unit resource LLVM");
            emit_native_unit_object(
                &sources,
                &order,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .expect("public unit object");
            assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
        }
    }
    assert_no_sibling_temporary(&directory.0);
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    (run, first_ir.expect("lowered unit LLVM"))
}

fn assert_stable_output(
    ir: String,
    object: &Path,
    first_ir: &mut Option<String>,
    first_object: &mut Option<Vec<u8>>,
) {
    if let Some(first) = first_ir {
        assert_eq!(first, &ir);
    } else {
        *first_ir = Some(ir);
    }
    let bytes = fs::read(object).unwrap();
    if let Some(first) = first_object {
        assert_eq!(first, &bytes);
    } else {
        *first_object = Some(bytes);
    }
}
