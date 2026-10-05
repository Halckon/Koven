//! SPEC-0275: direct generic container signatures through native owner delivery.
use super::*;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};
use lang_frontend::type_checking::{BuiltinType, IntrinsicTypeConstructor, UnitTypeKind};

const CONTAINERS: [(&str, &str); 3] = [
    ("Array", "arrayOf"),
    ("List", "listOf"),
    ("MutableList", "mutableListOf"),
];

#[path = "unit_generic_body_tests.rs"]
mod generic_bodies;

#[path = "unit_generic_body_budget_tests.rs"]
mod generic_body_budgets;

#[test]
fn unit_generic_container_native_scalar_and_string_lengths_preserve_owners() {
    for (container, constructor) in CONTAINERS {
        for (element, values, expected_element, allocations) in [
            ("Int", "1, 2", "1", 1),
            ("String", "\"one\" + \"!\", \"two\" + \"!\"", "\"one!\"", 3),
        ] {
            let analysis = analyze_sources(
                &format!(
                    r#"package p
                    fun <T> sizeOf(items: {container}<T>): Int = ((items)).size
                    fun <T> pass(own items: {container}<T>): {container}<T> = items
                    fun check(item: {element}): Unit {{
                        if (item != {expected_element}) {{ error("source reuse") }}
                    }}"#
                ),
                &format!(
                    r#"package q
                    fun entry(): Unit {{
                        val empty: {container}<{element}> = {constructor}()
                        val source = {constructor}({values})
                        if (p.sizeOf<{element}>(empty) != 0) {{ error("explicit empty") }}
                        if (p.sizeOf(empty) != 0) {{ error("inferred empty") }}
                        if (p.sizeOf<{element}>(source) != 2) {{ error("explicit nonempty") }}
                        if (p.sizeOf(source) != 2) {{ error("inferred repeat") }}
                        p.check(source[0])
                        if (source.size != 2) {{ error("source after Borrow") }}
                        val emptyFirst = p.pass<{element}>(empty)
                        val emptyReturned = p.pass(emptyFirst)
                        if (emptyReturned.size != 0) {{ error("empty own return") }}
                        val first = p.pass<{element}>(source)
                        val returned = p.pass(first)
                        p.check(returned[0])
                        if (p.sizeOf(returned) != 2) {{ error("returned Borrow") }}
                        if (returned.size != 2) {{ error("returned owner") }}
                        println("values-ok")
                    }}"#
                ),
            );
            assert_native_and_counted(&analysis, b"values-ok\n", allocations);
        }
    }
}

#[test]
fn unit_generic_container_native_resource_borrow_and_pass_drop_once_in_reverse() {
    for (container, constructor) in CONTAINERS {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                class Resource(val name: String) {{ deinit() {{ println(this.name) }} }}
                fun <T> sizeOf(items: {container}<T>): Int {{
                    println("borrow")
                    return items.size
                }}
                fun <T> pass(own items: {container}<T>): {container}<T> {{
                    println("pass")
                    return items
                }}"#
            ),
            &format!(
                r#"package q
                import p.Resource
                fun entry(): Unit {{
                    val source = {constructor}(Resource("one"), Resource("two"))
                    if (p.sizeOf<Resource>(source) != 2) {{ error("explicit resource Borrow") }}
                    if (p.sizeOf(source) != 2) {{ error("repeat resource Borrow") }}
                    if (source.size != 2) {{ error("source after resource Borrow") }}
                    println("after-borrow")
                    val first = p.pass<Resource>(source)
                    val returned = p.pass(first)
                    if (returned.size != 2) {{ error("resource own return") }}
                    println("after-pass")
                    println("scope-end")
                }}"#
            ),
        );
        // Resource containers retain lexical cleanup even after their last read.
        // Two objects and one buffer must remain the same three allocations
        // throughout both Borrow calls and both owner-return transfers.
        assert_native_and_counted(
            &analysis,
            b"borrow\nborrow\nafter-borrow\npass\npass\nafter-pass\nscope-end\ntwo\none\n",
            3,
        );
    }
}

#[test]
fn unit_generic_container_native_direct_argument_accepts_concrete_nested_list() {
    for (container, constructor) in CONTAINERS {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                fun <T> sizeOf(items: {container}<T>): Int = items.size
                fun <T> pass(own items: {container}<T>): {container}<T> = items"#
            ),
            &format!(
                r#"package q
                fun entry(): Unit {{
                    val source = {constructor}(listOf(1), listOf(2, 3))
                    if (p.sizeOf<List<Int>>(source) != 2) {{ error("explicit concrete nested") }}
                    if (p.sizeOf(source) != 2) {{ error("inferred concrete nested") }}
                    if (source.size != 2) {{ error("nested source reuse") }}
                    val first = p.pass<List<Int>>(source)
                    val returned = p.pass(first)
                    if (p.sizeOf(returned) != 2) {{ error("nested returned Borrow") }}
                    if (returned.size != 2) {{ error("nested own return") }}
                    println("nested-ok")
                }}"#
            ),
        );
        // Only the direct T is replaced with already-concrete List<Int>.
        // The outer buffer and both inner buffers each need unique cleanup.
        assert_native_and_counted(&analysis, b"nested-ok\n", 3);
    }
}

#[test]
fn unit_generic_container_native_rejections_preserve_outputs_and_canonical_arena() {
    let snapshot = |directory: &Path| {
        fs::read_dir(directory)
            .expect("read rejection output directory")
            .map(|entry| {
                let entry = entry.expect("rejection directory entry");
                (
                    entry.file_name(),
                    fs::read(entry.path()).expect("read every preserved file"),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let provider = "package p\nfun <T> nestedSize(items: List<List<T>>): Int = items.size";
    let consumer = "package q\nfun entry(): Unit {\n\
         val values = listOf(listOf(1))\n\
         val size = p.nestedSize<Int>(values)\n\
     }";
    let kind = NativeObjectErrorKind::UnsupportedSource;
    let detail = "UnsupportedNode";
    let span_text = "items: List<List<T>>";
    let analysis = analyze_sources(provider, consumer);
    let arena = analysis.typed.types().types();
    let int = arena.builtin(BuiltinType::Int).unwrap();
    let list_int = UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::List,
        arguments: vec![int],
    };
    let inner = arena
        .find(&list_int)
        .expect("real call publishes List<Int>");
    assert!(
        arena
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![inner],
            })
            .is_some(),
        "recursive rejection must not depend on an absent canonical"
    );
    let span = analysis
        .typed
        .types()
        .signatures()
        .declaration(analysis.declaration("p", "nestedSize"))
        .and_then(|signature| signature.callable())
        .unwrap()
        .parameters()[0]
        .span();
    assert_eq!(span.source_id(), analysis.provider_source);
    assert_eq!(
        &analysis.sources.source_text(span.source_id()).unwrap()[span.start()..span.end()],
        span_text
    );
    let arena_len = arena.len();
    for existing_target in [false, true] {
        let directory = TestDirectory::create();
        let object = directory.join("rejected.o");
        fs::write(directory.join("neighbor.bin"), b"neighbor\0unchanged").unwrap();
        if existing_target {
            fs::write(&object, b"old-object\0bytes").unwrap();
        }
        let before = snapshot(&directory.0);
        let error = emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect_err("unsupported generic boundary rejects before object publication");
        assert_eq!(error.kind(), kind);
        assert_eq!(error.span(), Some(span));
        assert_eq!(
            error.to_string(),
            format!("native object {kind:?}: frontend lowering failed with {detail}")
        );
        assert_eq!(
            arena.len(),
            arena_len,
            "backend cannot extend the typed arena"
        );
        assert_eq!(
            snapshot(&directory.0),
            before,
            "all names and bytes survive"
        );
        assert_eq!(object.exists(), existing_target);
        assert_no_sibling_temporary(&directory.0);
    }
}

fn assert_native_and_counted(analysis: &UnitAnalysis, expected: &[u8], allocations: usize) {
    run(analysis, expected);
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("direct generic container signatures lower with exact ownership");
    for module in &program.modules {
        for function in &module.functions {
            for instruction in &function.instructions {
                assert!(
                    !matches!(
                        instruction.operation,
                        Operation::StringClone { .. } | Operation::SharedRetain { .. }
                    ),
                    "generic container delivery must not insert clone or retain"
                );
            }
        }
    }
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("generic container native program passes SSA verification");
    // The existing harness rejects untracked and repeated frees and checks
    // every live allocation, so equal totals cannot hide descendant leaks.
    let counted = run_counted_allocations(&llvm, allocations);
    assert_success(&counted, expected);
}
