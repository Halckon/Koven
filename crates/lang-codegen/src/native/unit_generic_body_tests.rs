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
        fun entry(): Unit { val size = p.nullable(1) }"#,
    );
    assert_nullable_int_rejection(&analysis);
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
        assert_nullable_int_rejection(&analysis);
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
    // Frozen from the real nongeneric control: the type plan omits unsupported
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
