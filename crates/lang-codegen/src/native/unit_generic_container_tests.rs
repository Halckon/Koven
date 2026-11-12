//! SPEC-0275: direct generic container signatures through native owner delivery.
use super::*;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};
use lang_frontend::type_checking::{BuiltinType, IntrinsicTypeConstructor, UnitTypeKind};

const CONTAINERS: [(&str, &str); 3] = [
    ("Array", "arrayOf"),
    ("List", "listOf"),
    ("MutableList", "mutableListOf"),
];

mod generic_bodies {
//! SPEC-0276: body-only concrete identities reach real native ownership cleanup.
use super::*;
use crate::NativeObjectError;

const RESOURCE: &str = "class Resource(val name: String) { deinit() { println(this.name) } }";

#[test]
fn unit_generic_body_native_scalar_and_string_relay_lengths() {
    for (container, constructor) in CONTAINERS {
        for (element, explicit, inferred, allocations) in [
            ("Int", "1, 2", "3, 4", 2),
            (
                "String",
                "\"e-\" + \"one\", \"e-\" + \"two\"",
                "\"i-\" + \"one\", \"i-\" + \"two\"",
                6,
            ),
        ] {
            let analysis = relay_fixture(container, constructor, element, explicit, inferred);
            assert_body_native(
                &analysis,
                b"body-end\nafter-explicit\nbody-end\nafter-inferred\n",
                allocations,
                &format!("{container}<{element}> relay"),
            );
        }
    }
}

#[test]
fn unit_generic_body_native_resource_relay_drops_once_in_reverse() {
    for (container, constructor) in CONTAINERS {
        let analysis = relay_fixture(
            container,
            constructor,
            "Resource",
            "Resource(\"e-one\"), Resource(\"e-two\")",
            "Resource(\"i-one\"), Resource(\"i-two\")",
        );
        // Both reads precede cleanup; each callee drops its two resources in
        // reverse element order before the caller's returned marker.
        assert_body_native(
            &analysis,
            b"body-end\ne-two\ne-one\nafter-explicit\nbody-end\ni-two\ni-one\nafter-inferred\n",
            6,
            &format!("{container}<Resource> relay"),
        );
    }
}

#[test]
fn unit_generic_body_native_multiple_container_demands() {
    for (element, values, expected, allocations) in [
        ("Int", "1, 2", "multiple-end\nreturned\n", 2),
        (
            "String",
            "\"one\" + \"!\", \"two\" + \"!\"",
            "multiple-end\nreturned\n",
            4,
        ),
        (
            "Resource",
            "Resource(\"one\"), Resource(\"two\")",
            "multiple-end\ntwo\none\nreturned\n",
            4,
        ),
    ] {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                {resource}
                fun <T> multiple(own first: T, own second: T): Int {{
                    val a = arrayOf(first)
                    val l = listOf(second)
                    val m: MutableList<T> = mutableListOf()
                    println("multiple-end")
                    return a.size + l.size + m.size
                }}"#,
                resource = resource_declaration(element),
            ),
            &format!(
                r#"package q
                {import}
                fun entry(): Unit {{
                    if (p.multiple<{element}>({values}) != 2) {{ error("multiple demands") }}
                    println("returned")
                }}"#,
                import = resource_import(element),
            ),
        );
        assert_body_native(
            &analysis,
            expected.as_bytes(),
            allocations,
            &format!("multiple body demands for {element}"),
        );
    }
}

#[test]
fn unit_generic_body_native_deinit_only_closed_helper_seed() {
    for (container, constructor) in CONTAINERS {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                fun <T> helper(own x: T): Int {{
                    val xs = {constructor}(x)
                    println("helper")
                    return xs.size
                }}
                class Resource {{
                    deinit() {{
                        println("resource")
                        if (helper<Int>(1) != 1) {{ error("deinit helper result") }}
                        println("after-helper")
                    }}
                }}"#
            ),
            r#"package q
            import p.Resource
            fun entry(): Unit {
                val resource = Resource()
                println("entry-end")
            }"#,
        );
        assert_body_native(
            &analysis,
            b"entry-end\nresource\nhelper\nafter-helper\n",
            2,
            &format!("deinit-only {container}<Int>"),
        );
    }
}

#[test]
fn unit_generic_body_native_nullable_resource_and_empty_container() {
    for (container, constructor) in CONTAINERS {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                {RESOURCE}
                fun <T> nullable(own x: T): Int {{
                    val absent: T? = null
                    val present: T? = x
                    val empty: {container}<T> = {constructor}()
                    println("nullable-body")
                    return empty.size
                }}"#
            ),
            r#"package q
            import p.Resource
            fun entry(): Unit {
                if (p.nullable<Resource>(Resource("nullable-resource")) != 0) {
                    error("nullable empty body")
                }
                println("returned")
            }"#,
        );
        // Null and empty owners allocate nothing; wrapping the one real
        // Resource must preserve its pointer and its sole deinit obligation.
        assert_body_native(
            &analysis,
            b"nullable-body\nnullable-resource\nreturned\n",
            1,
            &format!("Nullable<Resource> with {container}<Resource>"),
        );
    }
}

/// SPEC-0350: the nongeneric `Int?` body is inside the refinement subset and now runs;
/// the generic `T?` body below still takes the existing path and keeps its rejection.
#[test]
fn unit_generic_body_native_nongeneric_nullable_int_control() {
    let analysis = analyze_sources(
        r#"package p
        fun nullable(own x: Int): Int {
            val absent: Int? = null
            val present: Int? = x
            return 0
        }"#,
        r#"package q
        fun entry(): Unit {
            val size = p.nullable(1)
            if (size == 0) { println("refined-nullable") }
        }"#,
    );
    // Copyable Int? allocates nothing, so the allocation-counting harness does not apply.
    run(&analysis, b"refined-nullable\n");
}

#[test]
fn unit_generic_body_native_nullable_int_keeps_control_rejection() {
    for (container, constructor, kind) in [
        ("Array", "arrayOf", IntrinsicTypeConstructor::Array),
        ("List", "listOf", IntrinsicTypeConstructor::List),
        (
            "MutableList",
            "mutableListOf",
            IntrinsicTypeConstructor::MutableList,
        ),
    ] {
        let analysis = analyze_sources(
            &format!(
                r#"package p
                fun <T> nullable(own x: T): Int {{
                    val absent: T? = null
                    val present: T? = x
                    val empty: {container}<T> = {constructor}()
                    return empty.size
                }}"#
            ),
            r#"package q
            fun entry(): Unit { val size = p.nullable<Int>(1) }"#,
        );
        let arena = analysis.typed.types().types();
        let int = arena.builtin(BuiltinType::Int).unwrap();
        assert!(
            arena
                .find(&UnitTypeKind::Intrinsic {
                    constructor: kind,
                    arguments: vec![int],
                })
                .is_some(),
            "{container}<Int> must exist before accepting the existing null rejection"
        );
        crate::ssa::with_per_family_lowering(|| {
            assert_nullable_int_rejection(&analysis);
        });
    }
}

fn relay_fixture(
    container: &str,
    constructor: &str,
    element: &str,
    explicit: &str,
    inferred: &str,
) -> UnitAnalysis {
    analyze_sources(
        &format!(
            r#"package p
            {resource}
            fun <T> inner(own first: T, own second: T): Int {{
                val empty: {container}<T> = {constructor}()
                val items = {constructor}(first, second)
                if (empty.size != 0) {{ error("empty body container") }}
                if (items.size != 2) {{ error("first header read") }}
                if (items.size != 2) {{ error("repeat preserves owner") }}
                println("body-end")
                return empty.size + items.size
            }}
            fun <T> middle(own first: T, own second: T): Int = inner(first, second)
            fun <T> relay(own first: T, own second: T): Int = middle(first, second)"#,
            resource = resource_declaration(element),
        ),
        &format!(
            r#"package q
            {import}
            fun entry(): Unit {{
                if (p.relay<{element}>({explicit}) != 2) {{ error("explicit relay") }}
                println("after-explicit")
                if (p.relay({inferred}) != 2) {{ error("inferred relay") }}
                println("after-inferred")
            }}"#,
            import = resource_import(element),
        ),
    )
}

fn resource_declaration(element: &str) -> &'static str {
    if element == "Resource" { RESOURCE } else { "" }
}

fn resource_import(element: &str) -> &'static str {
    if element == "Resource" {
        "import p.Resource"
    } else {
        ""
    }
}

fn assert_body_native(analysis: &UnitAnalysis, expected: &[u8], allocations: usize, label: &str) {
    eprintln!("body-only native fixture: {label}");
    let arena_len = analysis.typed.types().types().len();
    assert_native_and_counted(analysis, expected, allocations);
    assert_eq!(analysis.typed.types().types().len(), arena_len);
}

fn assert_nullable_int_rejection(analysis: &UnitAnalysis) {
    let arena = analysis.typed.types().types();
    let int = arena.builtin(BuiltinType::Int).unwrap();
    assert!(
        arena.find(&UnitTypeKind::Nullable(int)).is_some(),
        "existing inline rejection cannot substitute for a missing canonical Int?"
    );
    let span = analysis
        .provider
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| analysis.sources.slice(node.span()) == Ok("null"))
        .expect("the body has the actual null literal")
        .1
        .span();
    assert_eq!(span.source_id(), analysis.provider_source);
    let error = reject_preserving_outputs(analysis);
    // Frozen from the generic body on the existing lowering path: its type plan omits
    // inline nullable storage, and this null literal has no SSA type mapping.
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidModel);
    assert_eq!(error.span(), Some(span));
    assert_eq!(
        error.to_string(),
        "native object InvalidModel: frontend lowering failed with MissingFact"
    );
}

fn reject_preserving_outputs(analysis: &UnitAnalysis) -> NativeObjectError {
    let snapshot = |directory: &Path| {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let arena_len = analysis.typed.types().types().len();
    let mut previous = None;
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
        .expect_err("inline nullable remains outside the native representation");
        assert_eq!(analysis.typed.types().types().len(), arena_len);
        assert_eq!(
            snapshot(&directory.0),
            before,
            "all names and bytes survive"
        );
        assert_eq!(object.exists(), existing_target);
        assert_no_sibling_temporary(&directory.0);
        if let Some(previous) = &previous {
            assert_eq!(&error, previous);
        }
        previous = Some(error);
    }
    previous.unwrap()
}
}

mod generic_body_budgets {
//! SPEC-0276: production cache bounds do not replace entry-local planner limits.
use super::*;

const GROW: &str = "package p\n\
    fun <T> grow(): Unit {\n\
        val values: List<T> = listOf()\n\
        grow<List<T>>()\n\
    }";

#[test]
fn unit_generic_body_native_production_limit_rejects_growth_preserving_outputs() {
    let analysis = analyze_sources(GROW, "package q\nfun entry(): Unit { p.grow<Int>() }");
    let arena = analysis.typed.types().types();
    let int = arena.builtin(BuiltinType::Int).unwrap();
    assert!(
        arena
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![int],
            })
            .is_some(),
        "body-only cache must prepare the actual growth prefix"
    );
    let span = analysis
        .provider
        .ast()
        .items()
        .get(
            analysis.names.names().index().declarations()
                [analysis.declaration("p", "grow").index()]
            .root(),
        )
        .unwrap()
        .span();
    assert_eq!(span.source_id(), analysis.provider_source);
    let arena_len = arena.len();
    let snapshot = |directory: &Path| {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    for existing_target in [false, true] {
        let directory = TestDirectory::create();
        let object = directory.join("growth.o");
        fs::write(directory.join("neighbor.bin"), b"neighbor\0unchanged").unwrap();
        if existing_target {
            fs::write(&object, b"old-object\0bytes").unwrap();
        }
        let before = snapshot(&directory.0);
        // This entry is never linked or executed: its actual graph must be
        // rejected by the existing Phase 4 specialization budget before emit.
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
        .expect_err("the production planner rejects the selected growing instance graph");
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        assert_eq!(error.span(), Some(span));
        assert_eq!(
            error.to_string(),
            "native object UnsupportedSource: frontend lowering failed with InstanceLimitExceeded"
        );
        assert_eq!(arena.len(), arena_len);
        assert_eq!(
            snapshot(&directory.0),
            before,
            "all names and bytes survive"
        );
        assert_eq!(object.exists(), existing_target);
        assert_no_sibling_temporary(&directory.0);
    }
}

#[test]
fn unit_generic_body_native_unselected_growth_seeds_preserve_normal_entry() {
    for closed_seed in [false, true] {
        let provider = if closed_seed {
            format!("{GROW}\nfun unused(): Unit {{ grow<Int>() }}")
        } else {
            GROW.to_owned()
        };
        let analysis = analyze_sources(
            &provider,
            "package q\nfun entry(): Unit { println(\"normal-growth\") }",
        );
        let arena = analysis.typed.types().types();
        let int = arena.builtin(BuiltinType::Int).unwrap();
        assert_eq!(
            arena
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![int],
                })
                .is_some(),
            closed_seed,
            "only the real closed unused() call seeds the bounded growth cache"
        );
        let calls = analysis
            .typed
            .types()
            .calls()
            .iter()
            .filter(|call| {
                call.target()
                    == lang_frontend::type_checking::UnitCallTarget::Declaration(
                        analysis.declaration("p", "grow"),
                    )
                    && call.instance().type_arguments() == [int]
            })
            .count();
        assert_eq!(calls, usize::from(closed_seed));
        let arena_len = arena.len();
        // The selected normal entry only prints. The recursive source remains
        // present, including the real closed seed in the second control.
        run(&analysis, b"normal-growth\n");
        assert_eq!(arena.len(), arena_len);
    }
}
}

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
