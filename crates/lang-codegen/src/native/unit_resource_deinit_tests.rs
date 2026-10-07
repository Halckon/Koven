//! Resource bodies and lexical cleanup through both public compilation-unit entries.

use super::*;
use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

const LEAF: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun observe(value: String): Unit { println(value) }
"#;

#[test]
fn unit_resource_deinit_native_body_then_reverse_fields_and_lexical_owners() {
    for constants in [false, true] {
        let provider = format!(
            r#"package p
{LEAF}
class Guard(val first: Leaf, val second: Leaf) {{
    deinit() {{ observe(BANNER); println(this.first.name); println(this.second.name) }}
}}
"#
        );
        let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit {
    val earlier = Leaf("earlier")
    val guard = Guard(Leaf("first"), Leaf("second"))
    println("scope")
}
"#;
        let (run, llvm) = run_unit(&provider, consumer, constants);
        let expected = b"scope\nbody\nfirst\nsecond\nsecond\nfirst\nearlier\n";
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected);
        let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
        crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
    }
}

#[test]
fn unit_resource_deinit_native_transfer_return_and_temporary_drop_once() {
    for constants in [false, true] {
        let provider = format!(
            r#"package p
{LEAF}
fun make(): Leaf {{ val local = Leaf("returned"); return local }}
fun consume(own item: Leaf): Unit {{ println(BANNER) }}
fun inspect(item: Leaf): Unit {{ println("inspect") }}
fun branch(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    if (flag) {{ val inner = Leaf("inner"); println("return"); return }}
    println("fallthrough")
}}
"#
        );
        let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val source = p.make()
    val destination = source
    println("before")
    p.consume(destination)
    p.inspect(Leaf("temporary"))
    p.branch(true)
    p.branch(false)
    println("done")
}
"#;
        let (run, _) = run_unit(&provider, consumer, constants);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before\nbody\nreturned\ninspect\ntemporary\nreturn\ninner\nouter\nfallthrough\nouter\ndone\n");
    }
}

#[test]
fn unit_resource_deinit_native_loop_exits_preserve_outer_resource() {
    for constants in [false, true] {
        let provider = format!("package p\n{LEAF}");
        let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val outer = Leaf("outer")
    var i: Int = 0
    while (i < 3) {
        val inner = Leaf("inner")
        i = i + 1
        if (i == 1) { println("continue"); continue }
        if (i == 2) { println("break"); break }
    }
    println("after loop")
}
"#;
        let (run, _) = run_unit(&provider, consumer, constants);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            b"continue\ninner\nbreak\ninner\nafter loop\nouter\n"
        );
    }
}

#[test]
fn unit_resource_deinit_native_body_return_cleans_fields_but_abort_does_not() {
    for constants in [false, true] {
        for abort in [false, true] {
            let body_exit = if abort { "error(\"stop\")" } else { "return" };
            let provider = format!(
                r#"package p
{LEAF}
class Guard(val child: Leaf) {{ deinit() {{ observe(BANNER); {body_exit} }} }}
"#
            );
            let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit { val guard = Guard(Leaf("field")); println("scope") }
"#;
            let (run, _) = run_unit(&provider, consumer, constants);
            if abort {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(run.status.signal(), Some(6), "{run:?}");
                }
                assert!(!run.status.success(), "{run:?}");
                assert_eq!(run.stdout, b"scope\nbody\n");
            } else {
                assert!(run.status.success(), "{run:?}");
                assert_eq!(run.stdout, b"scope\nbody\nfield\n");
            }
        }
    }
}

#[test]
fn unit_resource_deinit_native_readonly_reborrow_clone_and_pending_field_chain() {
    for constants in [false, true] {
        let provider = format!(
            r#"package p
{LEAF}
fun inspect(resource: Guard): Unit {{ println("inspect") }}
fun show(text: String, own number: Int): Unit {{ println(text) }}
class Guard(val child: Leaf) {{
    deinit() {{
        inspect(this)
        show(this.child.name, if (true) {{ 1 }} else {{ 2 }})
        println(this.child.name.clone())
        val local = Leaf("local")
        println(BANNER)
        return
    }}
}}
"#
        );
        let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit { val guard = Guard(Leaf("field")); println("scope") }
"#;
        let (run, _) = run_unit(&provider, consumer, constants);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            b"scope\ninspect\nfield\nfield\nbody\nlocal\nfield\n"
        );
    }
}

#[test]
fn unit_destructuring_native_move_only_and_copyable_execution() {
    for constants in [false, true] {
        let provider = format!(
            r#"package p
{LEAF}
value class ResourcePair(val left: Leaf, val right: Leaf)
value class IntPair(val first: Int, val second: Int)
fun makeResources(): ResourcePair = ResourcePair(Leaf("left"), Leaf("right"))
fun makeInts(): IntPair = IntPair(10, 32)
"#
        );
        let consumer = r#"package q
import p.Leaf
import p.ResourcePair
import p.IntPair
fun entry(): Unit {
    val (x, y) = p.makeInts()
    if (x + y == 42) {
        println("42")
    }
    val (r1, r2) = p.makeResources()
    println("destructured")
}
"#;
        let (run, llvm) = run_unit(&provider, consumer, constants);
        let expected = b"42\ndestructured\nright\nleft\n";
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected);
        let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
        crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
    }
}

fn run_unit(provider: &str, consumer: &str, constants: bool) -> (std::process::Output, String) {
    let provider = if constants {
        provider.replacen(
            "package p",
            "package p\nconst val BANNER: String = \"body\"",
            1,
        )
    } else {
        provider.replace("BANNER", "\"body\"")
    };
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", &provider);
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
    let object = directory.join("resource.o");
    let executable = directory.join("resource");
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
