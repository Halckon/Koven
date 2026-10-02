#[path = "lower_frontend_return_tests.rs"]
mod return_control_tests;

use lang_frontend::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypeEnvironment, TypedFile, check_types, standard_environments},
};

use super::{
    lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
    model::Operation,
    render::render_program,
};
use crate::llvm::render_verified_program;

#[path = "call_lifetimes_tests.rs"]
mod call_lifetimes_tests;

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    types: TypeEnvironment,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

#[test]
fn boxed_enum_signature_only_storage_accepts_box_or_enum_first() {
    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        let analysis = analyze(&source);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        // Without any construction demand, the first parameter is the mapper's actual entry root.
        assert!(analysis.typed.constructions().is_empty());
        let lower = || {
            lower_scalar_file(
                &analysis.sources,
                &analysis.parsed,
                &analysis.names,
                &analysis.typed,
                &analysis.owned,
            )
            .expect("either recursive storage demand root must lower")
        };
        let program = lower();
        let rendered = render_program(&program);
        assert!(rendered.contains("heap_owner"), "{rendered}");
        assert!(rendered.contains("tagged_union"), "{rendered}");
        assert_eq!(rendered, render_program(&lower()));
        render_verified_program(&program).expect("signature-only recursive Box/enum LLVM verifies");
    }
}

#[test]
fn boxed_enum_mapper_preserves_nonrecursive_value_wrappers() {
    for wrapper in [
        "Box<Wrapped>",
        "Box<Wrapped>?",
        "Rc<Box<Wrapped>>",
        "Array<Box<Wrapped>>",
    ] {
        let analysis = analyze(&format!(
            "value class Wrapped(val value: Int)\nfun inspect(input: {wrapper}): Unit {{}}"
        ));
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        assert!(analysis.typed.constructions().is_empty());
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        render_verified_program(&program)
            .expect("existing wrapped Box payload must stay fully defined");
    }
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (environment, types) = standard_environments();
    let names =
        resolve_names(&sources, &parsed, &environment).expect("names must resolve internally");
    let typed =
        check_types(&sources, &parsed, &names, &types).expect("types must check internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership must check internally");
    Analysis {
        sources,
        parsed,
        names,
        types,
        typed,
        owned,
    }
}

#[test]
fn lowers_standard_dynamic_string_error_after_evaluating_its_borrow() {
    let analysis = analyze(
        "fun normal(): Unit {}\n\
         fun abortNow(message: String): Unit { error(message + \"!\") }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("standard error must lower to verified SSA Abort");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("abort @source(").count(), 1, "{ssa}");
    assert!(!ssa.contains("call @error"), "{ssa}");
    assert!(ssa.contains("string.literal"), "{ssa}");
    assert!(ssa.contains("string.concat"), "{ssa}");
    assert!(ssa.contains("borrow.shared"), "{ssa}");

    let unsupported = analyze("fun abortInterpolated(): Unit { error(\"${1}\") }");
    let error = match lower_scalar_file(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
    ) {
        Ok(_) => panic!("String interpolation remains outside this native slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn lowers_dynamic_string_println_calls_in_source_order_with_decoded_utf8() {
    let analysis = analyze(
        r#"fun borrowed(input: String): Unit { println(input) }
        fun output(): Unit {
            if (true) { println("") }
            if (true) { println("Hello") }
            if (true) { println("你好") }
            if (true) { println("\\\'\"\n\r\t\0\$") }
        }"#,
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("plain String println calls must lower");
    let bytes = program.modules[0]
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .filter_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        bytes,
        [
            b"".as_slice(),
            b"Hello".as_slice(),
            "你好".as_bytes(),
            b"\\\'\"\n\r\t\0$".as_slice(),
        ]
    );
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("print.string").count(), 5, "{ssa}");
    assert_eq!(ssa.matches("end_borrow").count(), 4, "{ssa}");
    assert!(!ssa.contains("print.literal"), "{ssa}");

    let unsupported = analyze("fun output(): Unit { println(\"${1}\") }");
    let error = match lower_scalar_file(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
    ) {
        Ok(_) => panic!("interpolated println remains outside this native slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);

    let nonliteral = analyze("fun output(input: String): Unit { println(input) }");
    let program = lower_scalar_file(
        &nonliteral.sources,
        &nonliteral.parsed,
        &nonliteral.names,
        &nonliteral.typed,
        &nonliteral.owned,
    )
    .expect("borrowed String println must reuse the entry loan");
    let ssa = render_program(&program);
    assert!(ssa.contains("print.string %l0"), "{ssa}");
    assert!(!ssa.contains("borrow.shared"), "{ssa}");
}

#[test]
fn lowers_string_binary_views_value_calls_and_returns_without_early_drop() {
    let analysis = analyze(
        "fun identity(own input: String): String = input\n\
         fun combine(left: String, own right: String): String {\n\
             val joined = left + \"!\"\n\
             val differs = joined != right\n\
             return identity(right)\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("String Borrow/Value/return lowering must produce verified SSA");
    let ssa = render_program(&program);
    assert!(ssa.contains("string.concat %l0"), "{ssa}");
    assert!(ssa.contains("string.equal"), "{ssa}");
    assert!(ssa.contains("not "), "{ssa}");
    assert!(ssa.contains("call @"), "{ssa}");

    let combine = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("combine"))
        .expect("combine function");
    let concat = combine
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::StringConcat { .. }))
        .expect("concat instruction");
    let first_drop = combine
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .expect("operand drop");
    assert!(concat < first_drop, "{:?}", combine.instructions);
}

#[test]
fn lowers_intrinsic_rc_construction_share_and_copyable_payload_read() {
    let analysis = analyze(
        "fun shared(): Int {\n\
             val first = Rc(40)\n\
             val second = first.share()\n\
             val copied = second.value\n\
             return copied + first.value\n\
         }",
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("intrinsic Rc facts must lower through verified SSA");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("shared.allocate").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.retain").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.payload_place").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 2, "{ssa}");
    assert!(ssa.contains("shared_owner \"Rc#t"), "{ssa}");

    let llvm = render_verified_program(&program).expect("Rc SSA must lower to verified LLVM");
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 1, "{llvm}");
    assert!(llvm.contains(".overflow = icmp eq"), "{llvm}");
    assert_eq!(llvm.matches("strong.next = sub").count(), 1, "{llvm}");
    assert!(!llvm.contains("atomicrmw"), "{llvm}");
    assert!(!llvm.contains("cmpxchg"), "{llvm}");
}

#[test]
fn lowers_move_only_rc_payload_borrow_call_without_consuming_owner() {
    let analysis = analyze(
        "class Resource {}\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun exercise(): Unit {\n\
             val owner = Rc(Resource())\n\
             val inspected = inspect(owner.value)\n\
             val retained = owner.share()\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("MoveOnly Rc payload Borrow must lower through verified SSA");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("shared.payload_place").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("borrow.shared").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("end_borrow").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.retain").count(), 1, "{ssa}");
    assert!(ssa.contains("call @f0(%l0)"), "{ssa}");

    let llvm =
        render_verified_program(&program).expect("Borrow call must lower to LLVM pointer ABI");
    assert!(
        llvm.contains("define internal void @f0.inspect(ptr %l0)"),
        "{llvm}"
    );
    assert!(llvm.contains("call void @f0.inspect(ptr"), "{llvm}");
    assert_eq!(llvm.matches("call ptr @malloc").count(), 2, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 2, "{llvm}");
}

#[test]
fn lowers_repeated_class_and_box_root_borrows_without_rc_special_cases() {
    let analysis = analyze(
        "class Resource {}\n\
         value class Token(val item: Int)\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun inspectBox(resource: Box<Token>): Unit {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val first = inspect(resource)\n\
             val second = inspect(resource)\n\
             val boxed = Box(Token(1))\n\
             val third = inspectBox(boxed)\n\
             val fourth = inspectBox(boxed)\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("class and Box roots must use the generic Borrow call path");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("root_place").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("borrow.shared").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("end_borrow").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("call @f0").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("call @f1").count(), 2, "{ssa}");
    assert!(!ssa.contains("shared.payload_place"), "{ssa}");
}

#[test]
fn lowers_nullable_class_parameter_smart_cast_to_non_null_borrow_view() {
    let analysis = analyze(
        "class Node()\n\
         fun observe(node: Node): Unit {}\n\
         fun inspect(own node: Node?): Unit {\n\
             if (node != null) { observe(node) }\n\
         }",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("nullable smart-cast Borrow must lower");
    let ssa = render_program(&program);
    assert!(ssa.contains("nullable_handle<"), "{ssa}");
    assert!(ssa.contains("nullable.branch"), "{ssa}");
    assert!(ssa.contains("view %l"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable smart cast LLVM");
    assert!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert!(llvm.contains("phi ptr"), "{llvm}");
}

#[test]
fn lowers_nullable_rc_operations_through_the_proven_non_null_view() {
    let analysis = analyze(
        "fun inspect(own owner: Rc<Int>?): Unit {\n\
             if (owner != null) {\n\
                 val retained = owner.share()\n\
                 val copied = owner.value\n\
             }\n\
         }",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("nullable Rc operations must use the proven non-null view");
    let ssa = render_program(&program);
    assert!(ssa.contains("shared.retain %l"), "{ssa}");
    assert!(ssa.contains("shared.payload_place %l"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable Rc view LLVM");
    assert!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert!(llvm.contains(".next = add i64"), "{llvm}");
}

#[test]
fn nullable_when_read_views_preserve_owned_subject_for_reuse() {
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\n\
             fun observe(node: {inner}): Unit {{}}\n\
             fun inspect(own node: {inner}?): Unit {{\n\
                 when (node) {{ null -> {{}}; else -> {{ observe(node) }} }}\n\
                 when (node) {{ null -> {{}}; else -> {{ observe(node) }} }}\n\
             }}"
        ));
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .expect("read-only nullable when must preserve the owner for a later when");
        let ssa = render_program(&program);
        assert_eq!(ssa.matches("nullable.branch").count(), 2, "{ssa}");
        assert!(
            !ssa.contains("nullable.take"),
            "a view must not consume the owner: {ssa}"
        );
        assert!(
            !ssa.contains("shared.retain"),
            "read views must not share: {ssa}"
        );
        assert!(
            !ssa.contains("heap.allocate"),
            "proofs must not allocate: {ssa}"
        );
        let llvm = render_verified_program(&program)
            .expect("both nullable branches must pass LLVM lowering");
        assert!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert!(
            !llvm.contains("%koven.enum"),
            "pointer nullable needs no tag: {llvm}"
        );
    }
}

#[test]
fn rejects_inline_nullable_lowering_without_panicking() {
    let analysis = analyze(
        "value class Token(val item: Int)\n\
         fun inspect(own token: Token?): Unit {\n\
             if (token != null) {}\n\
         }",
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Ok(_) => panic!("inline nullable ABI remains outside SPEC-0196"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn rejects_borrowed_nullable_when_and_non_null_assertion_without_panicking() {
    for source in [
        "class Node {}\nfun inspect(inout node: Node?): Unit { when (node) { null -> {}; else -> {} } }",
        "class Node {}\nclass Holder(val item: Node?)\nfun inspect(holder: Holder): Unit { when (holder.item) { null -> {}; else -> {} } }",
        "class Node {}\nfun inspect(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }",
        "value class Token(val item: Int)\nfun inspect(own token: Token?): Unit { when (token) { null -> {}; else -> {} } }",
        "class Node {}\n\
         fun inspect(node: Node?): Unit {\n\
             when (node) { null -> {}; else -> {} }\n\
         }",
        "fun inspect(node: Int?): Int = node!!",
    ] {
        let analysis = analyze(source);
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(analysis.owned.diagnostics().is_empty());
        let error = match lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        ) {
            Ok(_) => panic!("unimplemented nullable control form must not silently lower"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[test]
fn declarative_type_roots_do_not_enter_the_scalar_instance_graph() {
    let analysis = analyze(
        "public value class Pair<A, B>(val first: A, val second: B)\n\
         class Holder(val item: Int)\n\
         interface Marker\n\
         enum class Outcome<T, E> { Ok(item: T), Err(failure: E) }\n\
         fun entry(): Unit {}",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());

    let lower = || {
        lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .expect("declarative type roots have no module initialization action")
    };
    let program = lower();
    let repeated = lower();
    let ssa = render_program(&program);
    assert_eq!(ssa, render_program(&repeated));
    assert_eq!(ssa.matches("func \"").count(), 1, "{ssa}");
    assert!(ssa.contains("func \"entry\""), "{ssa}");
    for declaration in ["Pair", "Holder", "Marker", "Outcome"] {
        assert!(!ssa.contains(declaration), "{ssa}");
    }

    let llvm = render_verified_program(&program).expect("declarative roots must reach LLVM");
    assert_eq!(llvm.matches("define internal").count(), 1, "{llvm}");
    for declaration in ["Pair", "Holder", "Marker", "Outcome"] {
        assert!(!llvm.contains(declaration), "{llvm}");
    }

    {
        let rejected = analyze("val state = 1\nfun entry(): Unit {}");
        assert!(rejected.names.diagnostics().is_empty());
        assert!(rejected.typed.diagnostics().is_empty());
        let error = match lower_scalar_file(
            &rejected.sources,
            &rejected.parsed,
            &rejected.names,
            &rejected.typed,
            &rejected.owned,
        ) {
            Ok(_) => panic!("runtime-bearing roots remain outside this lowering slice"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert!(error.span.is_some());
    }

    let nominal_use = analyze(
        "value class Wrapped(val item: Int)\n\
         class Holder(val item: Int)\n\
         class Resource {}\n\
         enum class Flag { On, Off }\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         enum class Owned<T> { Some(item: T), None }\n\
         fun wrapped(): Wrapped = Wrapped(1)\n\
         fun holder(): Holder = Holder(2)\n\
         fun boxed(): Box<Wrapped> = Box(Wrapped(3))\n\
         fun flag(): Flag = Flag.On\n\
         fun some(): Maybe<Int> = Maybe.Some(4)\n\
         fun enumProjected(): Int { val maybe: Maybe<Int> = Maybe.Some(8) val result: Int = when (maybe) {\n\
             is Maybe.Some<Int> -> maybe.item\n\
             is Maybe.None<Int> -> 0\n\
         } return result }\n\
         fun projected(): Int { val wrapped = Wrapped(5) return wrapped.item }\n\
         fun destructured(): Int { val (item) = Wrapped(6) return item }\n\
         fun classProjected(): Int { val holder = Holder(7) return holder.item }\n\
         fun dropEnum(): Unit { val event: Owned<Resource> = Owned.Some(Resource()) }",
    );
    assert!(
        nominal_use.typed.diagnostics().is_empty(),
        "{:?}",
        nominal_use.typed.diagnostics()
    );
    assert!(nominal_use.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &nominal_use.sources,
        &nominal_use.parsed,
        &nominal_use.names,
        &nominal_use.typed,
        &nominal_use.owned,
    )
    .expect("value/class/Box construction facts must lower to verified SSA");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("aggregate.construct").count(), 11, "{ssa}");
    assert_eq!(ssa.matches("heap.allocate").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("tagged.construct").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("aggregate.project").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("aggregate.copy_explode").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("heap.payload_place").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("tagged.payload_place").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("tagged.discriminant").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 2, "{ssa}");
    assert!(ssa.contains("Wrapped#t"), "{ssa}");
    assert!(ssa.contains("Holder#t"), "{ssa}");
    assert!(ssa.contains("Box#t"), "{ssa}");
    assert!(ssa.contains("Flag#t"), "{ssa}");
    assert!(ssa.contains("Maybe#t"), "{ssa}");
    assert!(ssa.contains("Owned#t"), "{ssa}");

    let llvm = render_verified_program(&program).expect("nominal SSA must lower to LLVM");
    assert_eq!(llvm.matches("call ptr @malloc").count(), 4, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 2, "{llvm}");
    assert!(llvm.contains("%koven.enum.t"), "{llvm}");
    assert!(!llvm.contains("retain"), "{llvm}");
    assert!(!llvm.contains("clone"), "{llvm}");
}

#[test]
fn lowers_real_scalar_expression_functions_through_verified_ssa() {
    let analysis = analyze(
        "fun add(left: Int, right: Int): Int = left + right\n\
         fun negate(input: Int): Int = -input\n\
         fun ordered(left: Int, right: Int): Boolean = !(left >= right)\n\
         fun constantOrder(): Boolean = 1 < 2\n\
         fun invoke(first: Int, second: Int): Int = add(right = second, left = first)",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("closed scalar subset must lower");
    let first = render_program(&program);
    let second = render_program(&program);
    assert_eq!(first, second);
    assert!(first.contains("func \"add\""));
    assert!(first.contains("func \"negate\""));
    assert!(first.contains("func \"ordered\""));
    assert!(first.contains("func \"constantOrder\""));
    assert!(first.contains("func \"invoke\""));
    assert!(first.contains("checked.add"));
    assert!(first.contains("checked.sub"));
    assert!(first.contains("cmp.ge"));
    assert!(first.contains("not %v"));
    assert!(first.contains("call @f0("));
    assert_eq!(first.matches("abort @source").count(), 2);
}

#[test]
fn lowers_integer_literal_boundaries_without_runtime_overflow_checks() {
    let analysis = analyze(
        "fun byteMin(): Byte = -128\n\
         fun byteMax(): Byte = 127\n\
         fun ubyteMax(): UByte = 255u\n\
         fun shortMin(): Short = -32768\n\
         fun shortMax(): Short = 32767\n\
         fun ushortMax(): UShort = 65535u\n\
         fun intMin(): Int = -2147483648\n\
         fun intMax(): Int = 2147483647\n\
         fun uintMax(): UInt = 4294967295u\n\
         fun longMin(): Long = -9223372036854775808L\n\
         fun longMax(): Long = 9223372036854775807L\n\
         fun ulongMax(): ULong = 18446744073709551615uL",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("integer literal boundaries must lower through verified SSA");
    let ssa = render_program(&program);
    for constant in [
        "const -128",
        "const 127",
        "const 255",
        "const -32768",
        "const 32767",
        "const 65535",
        "const -2147483648",
        "const 2147483647",
        "const 4294967295",
        "const -9223372036854775808",
        "const 9223372036854775807",
        "const 18446744073709551615",
    ] {
        assert!(ssa.contains(constant), "missing {constant} in {ssa}");
    }
    assert!(!ssa.contains("checked.sub"));
    assert!(!ssa.contains("abort @source"));

    let llvm = render_verified_program(&program).expect("literal boundary SSA must lower to LLVM");
    for (function, instruction) in [
        ("byteMin", "ret i8 -128"),
        ("byteMax", "ret i8 127"),
        ("ubyteMax", "ret i8 -1"),
        ("shortMin", "ret i16 -32768"),
        ("shortMax", "ret i16 32767"),
        ("ushortMax", "ret i16 -1"),
        ("intMin", "ret i32 -2147483648"),
        ("intMax", "ret i32 2147483647"),
        ("uintMax", "ret i32 -1"),
        ("longMin", "ret i64 -9223372036854775808"),
        ("longMax", "ret i64 9223372036854775807"),
        ("ulongMax", "ret i64 -1"),
    ] {
        let body = llvm_function_body(&llvm, function);
        assert!(
            body.contains(instruction),
            "missing {instruction} in {body}"
        );
    }
    assert!(!llvm.contains("llvm.ssub.with.overflow"));
    assert!(!llvm.contains("llvm.trap"));

    let invalid = analyze(
        "fun byteTooLow(): Byte = -129\n\
         fun ulongTooHigh(): ULong = 18446744073709551616uL",
    );
    assert_eq!(invalid.typed.diagnostics().len(), 2);
    assert!(
        invalid
            .typed
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0090")
    );
    let error = lower_scalar_file(
        &invalid.sources,
        &invalid.parsed,
        &invalid.names,
        &invalid.typed,
        &invalid.owned,
    )
    .err()
    .expect("out-of-range literals must be rejected before SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);
}

#[test]
fn lowers_reachable_scalar_generic_instances_once_and_keeps_recursive_identity() {
    let analysis = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("reachable concrete scalar generic instances must lower");
    let rendered = render_program(&program);
    let repeated = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("repeated instance planning must lower");
    assert_eq!(rendered, render_program(&repeated));
    assert_eq!(rendered.matches("func \"identity<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"identity<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"recurse<Int>\"").count(), 1);
    assert!(!rendered.contains("unused<"));

    let module = &program.modules[0];
    let recursive = module
        .functions
        .iter()
        .find(|function| function.name == "recurse<Int>")
        .expect("recursive generic instance must exist");
    let recursive_body = rendered
        .split("func \"recurse<Int>\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("recursive instance body must render");
    assert!(recursive_body.contains(&format!("call @f{}(", recursive.id.index())));
}

#[test]
fn generic_overloads_with_the_same_type_arguments_keep_distinct_targets() {
    let analysis = analyze(
        "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("generic overload instances must lower independently");
    let module = &program.modules[0];
    let choices = module
        .functions
        .iter()
        .filter(|function| function.name == "choose<Int>")
        .map(|function| function.id)
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 2);
    let target_of = |name: &str| {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("caller must exist");
        function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee, .. } => Some(callee),
                _ => None,
            })
            .expect("caller must contain one direct call")
    };
    let one = target_of("one");
    let two = target_of("two");
    assert_ne!(one, two);
    assert!(choices.contains(&one));
    assert!(choices.contains(&two));
    let llvm = render_verified_program(&program).expect("distinct overload instances must lower");
    assert_eq!(
        llvm.matches("define internal i32 @\"f0.choose<Int>\"")
            .count(),
        1
    );
    assert_eq!(
        llvm.matches("define internal i32 @\"f1.choose<Int>\"")
            .count(),
        1
    );
    assert!(llvm.contains("call i32 @\"f0.choose<Int>\""));
    assert!(llvm.contains("call i32 @\"f1.choose<Int>\""));
}

#[test]
fn lowers_verified_frontend_ssa_to_deterministic_llvm_ir() {
    let analysis = analyze(
        "fun byte(input: Byte): Byte = input\n\
         fun ubyte(input: UByte): UByte = input\n\
         fun short(input: Short): Short = input\n\
         fun ushort(input: UShort): UShort = input\n\
         fun int(input: Int): Int = input\n\
         fun uint(input: UInt): UInt = input\n\
         fun long(input: Long): Long = input\n\
         fun ulong(input: ULong): ULong = input\n\
         fun boolean(input: Boolean): Boolean = !input\n\
         fun addSigned(left: Int, right: Int): Int = left + right\n\
         fun addUnsigned(left: UInt, right: UInt): UInt = left + right\n\
         fun subtractUnsigned(left: UInt, right: UInt): UInt = left - right\n\
         fun multiplyUnsigned(left: UInt, right: UInt): UInt = left * right\n\
         fun subtract(left: Int, right: Int): Int = left - right\n\
         fun multiply(left: Int, right: Int): Int = left * right\n\
         fun divide(left: Int, right: Int): Int = left / right\n\
         fun remainder(left: Int, right: Int): Int = left % right\n\
         fun divideUnsigned(left: UInt, right: UInt): UInt = left / right\n\
         fun remainderUnsigned(left: UInt, right: UInt): UInt = left % right\n\
         fun choose(flag: Boolean, left: Int, right: Int): Int =\n\
             if (flag) { left } else { right }\n\
         fun increment(limit: Int): Int {\n\
             var current: Int = 0\n\
             while (current < limit) { current += 1 }\n\
             return current\n\
         }\n\
         fun invoke(left: Int, right: Int): Int = addSigned(left, right)\n\
         fun unit(flag: Boolean): Unit { if (flag) { 1 } }",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("frontend scalar matrix must produce verified SSA");
    let first =
        render_verified_program(&program).expect("verified SSA must lower to verified LLVM");
    let second = render_verified_program(&program).expect("repeated LLVM lowering must succeed");
    assert_eq!(first, second);
    assert!(first.contains(&format!(
        "target triple = \"{}\"",
        crate::llvm::native_target_triple().unwrap()
    )));
    assert!(first.contains("define internal i8 @f0.byte(ptr %l0)"));
    assert!(first.contains("define internal i16 @f2.short(ptr %l0)"));
    assert!(first.contains("define internal i32 @f4.int(ptr %l0)"));
    assert!(first.contains("define internal i64 @f6.long(ptr %l0)"));
    assert!(first.contains("define internal i1 @f8.boolean(ptr %l0)"));
    assert!(first.contains("llvm.sadd.with.overflow.i32"));
    assert!(first.contains("llvm.uadd.with.overflow.i32"));
    assert!(first.contains("llvm.usub.with.overflow.i32"));
    assert!(first.contains("llvm.umul.with.overflow.i32"));
    assert!(first.contains("llvm.ssub.with.overflow.i32"));
    assert!(first.contains("llvm.smul.with.overflow.i32"));
    assert!(first.contains(" sdiv i32 "));
    assert!(first.contains(" udiv i32 "));
    assert!(first.contains(" srem i32 "));
    assert!(first.contains(" urem i32 "));
    assert!(first.contains("phi i32"));
    assert!(first.contains("br i1"));
    assert!(first.contains("call i32 @f9.addSigned"));
    assert!(first.contains("call void @abort()"));
    assert!(first.contains("unreachable"));
    assert!(!first.contains("invoke "));
    assert!(!first.contains("landingpad"));

    for function in ["divide", "remainder"] {
        let body = llvm_function_body(&first, function);
        assert!(body.contains(".zero = icmp eq i32"), "{body}");
        assert!(body.contains(".min = icmp eq i32"), "{body}");
        assert!(body.contains(", -2147483648"), "{body}");
        assert!(body.contains(".minus_one = icmp eq i32"), "{body}");
        assert!(body.contains(", -1"), "{body}");
        assert!(body.contains(".signed_overflow = and i1"), "{body}");
        assert!(body.contains(" = or i1 "), "{body}");
        assert!(body.contains(" = select i1 "), "{body}");
        assert!(body.contains("i32 1"), "{body}");
    }
    for function in ["divideUnsigned", "remainderUnsigned"] {
        let body = llvm_function_body(&first, function);
        assert!(body.contains(" = icmp eq i32 "), "{body}");
        assert!(body.contains(", 0"), "{body}");
        assert!(body.contains(" = select i1 "), "{body}");
        assert!(body.contains("i32 1"), "{body}");
        assert!(!body.contains(".signed_overflow"), "{body}");
    }
}

#[test]
fn rejects_mixed_analysis_chains_before_constructing_ssa() {
    let first = analyze("fun value(input: Int): Int = input");
    let second = analyze("fun value(input: Int): Int = input");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &second.owned,
    )
    .err()
    .expect("foreign ownership analysis must be rejected");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedSource);

    let repeated_typed = check_types(&first.sources, &first.parsed, &first.names, &first.types)
        .expect("repeated type analysis must succeed");
    let repeated_owned =
        check_ownership(&first.sources, &first.parsed, &first.names, &repeated_typed)
            .expect("ownership analysis for repeated typed product must succeed");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &repeated_owned,
    )
    .err()
    .expect("ownership from another typed product must not match the original typed chain");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
}

#[test]
fn lowers_straight_line_blocks_locals_assignments_and_returns() {
    let analysis = analyze(
        "fun compute(input: Int): Int {\n\
             val doubled: Int = input + input\n\
             var total: Int = doubled\n\
             { total += 1 }\n\
             return total\n\
         }\n\
         fun observe(input: Int): Unit {\n\
             val local: Int = input\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("straight-line scalar blocks must lower");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"compute\""));
    assert!(rendered.contains("func \"observe\""));
    assert_eq!(rendered.matches("checked.add").count(), 2);
    assert_eq!(rendered.matches("abort @source").count(), 2);
}

#[test]
fn lowers_if_short_circuit_and_branch_local_updates_as_cfg() {
    let analysis = analyze(
        "fun rhs(): Boolean = true\n\
         fun choose(flag: Boolean, left: Int, right: Int): Int =\n\
             if (flag) { left } else { right }\n\
         fun conjunction(left: Boolean): Boolean = left && rhs()\n\
         fun disjunction(left: Boolean): Boolean = left || rhs()\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = total + 1 } else { total = total + 2 }\n\
             return total\n\
         }\n\
         fun same(flag: Boolean, input: Int, replacement: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = replacement } else { total = replacement }\n\
             return total\n\
         }\n\
         fun discard(flag: Boolean): Unit {\n\
             if (flag) { 1 }\n\
             when { flag -> 2 }\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("if and short-circuit expressions must lower through verified CFG");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"choose\""));
    assert!(rendered.contains("func \"conjunction\""));
    assert!(rendered.contains("func \"disjunction\""));
    assert!(rendered.contains("func \"update\""));
    assert!(rendered.contains("func \"same\""));
    assert!(rendered.contains("func \"discard\""));
    assert!(rendered.matches("cond %v").count() >= 4);
    assert!(rendered.matches("branch bb").count() >= 8);
    assert!(rendered.contains("call @f0()"));
    assert!(rendered.contains("bb3(%v"));
    let conjunction = rendered
        .split("func \"conjunction\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("conjunction function must render");
    assert!(
        conjunction.find("cond %v").expect("short-circuit branch")
            < conjunction.find("call @f0()").expect("right-hand call")
    );
    let same = rendered
        .split("func \"same\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("same function must render");
    assert_eq!(same.matches("return %v").count(), 1, "{same}");
}

#[test]
fn carries_move_only_string_bindings_across_if_and_short_circuit_edges() {
    let analysis = analyze(
        "fun keep(own joined: String, flag: Boolean): String {\n\
             if (flag) { println(joined) } else { println(joined) }\n\
             return joined\n\
         }\n\
         fun compare(own joined: String, flag: Boolean): String {\n\
             val matches: Boolean = flag && joined == \"hello\"\n\
             if (matches) { println(joined) }\n\
             return joined\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("live MoveOnly bindings must be delivered explicitly across every CFG edge");
    let rendered = render_program(&program);
    for name in ["keep", "compare"] {
        let body = rendered
            .split(&format!("func \"{name}\""))
            .nth(1)
            .and_then(|body| body.split("\n\n  func").next())
            .expect("function must render");
        assert!(body.matches("bb").count() >= 4, "{body}");
        assert!(body.contains("branch bb"), "{body}");
        assert!(body.contains("return %v"), "{body}");
    }
}

#[test]
fn comma_when_drops_owner_only_on_the_match_that_skips_consumption() {
    let analysis = analyze(
        "class Node {}\nfun probe(own node: Node): Boolean = false\nfun select(flag: Boolean, own node: Node): Int = when { flag, probe(node) -> 0; else -> 1 }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("each matched edge must discharge its own owner before the shared body");
    let rendered = render_program(&program);
    assert_eq!(rendered.matches("drop ").count(), 2, "{rendered}");
}

#[test]
fn lowers_boolean_when_subjectless_chains_and_diverging_entries() {
    let analysis = analyze(
        "fun select(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             false -> 2\n\
         }\n\
         fun predicate(left: Boolean, right: Boolean): Int = when {\n\
             left && right -> 1\n\
             left -> 2\n\
             else -> 3\n\
         }\n\
         fun grouped(left: Boolean, right: Boolean): Int = when {\n\
             left, right -> 7\n\
             else -> 8\n\
         }\n\
         fun early(flag: Boolean): Int = when (flag) {\n\
             true -> return 4\n\
             false -> 5\n\
         }\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             when { flag -> total = 6 }\n\
             return total\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("Boolean when forms must lower through verified CFG");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"select\""));
    assert!(rendered.contains("func \"predicate\""));
    assert!(rendered.contains("func \"grouped\""));
    assert!(rendered.contains("func \"early\""));
    assert!(rendered.contains("func \"update\""));
    assert!(rendered.matches("cond %v").count() >= 6);
    assert!(rendered.matches("return %v").count() >= 4);
    assert!(rendered.contains("bb3(%v"));
}

#[test]
fn lowers_while_loop_break_continue_and_nested_loop_targets() {
    let analysis = analyze(
        "fun increment(limit: Int): Int {\n\
             var current: Int = 0\n\
             while (current < limit) { current += 1 }\n\
             return current\n\
         }\n\
         fun exits(flag: Boolean): Int {\n\
             var result: Int = 0\n\
             loop {\n\
                 if (flag) { { result = 1 } break } else { continue }\n\
             }\n\
             return result\n\
         }\n\
         fun nested(outer: Boolean, inner: Boolean): Int {\n\
             var total: Int = 0\n\
             loop {\n\
                 while (inner) { break }\n\
                 if (outer) { { total = 2 } break } else { continue }\n\
             }\n\
             return total\n\
         }\n\
         fun spin(): Unit { loop { continue } }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("while/loop and nearest break/continue targets must lower through verified SSA");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"increment\""));
    assert!(rendered.contains("func \"exits\""));
    assert!(rendered.contains("func \"nested\""));
    assert!(rendered.contains("func \"spin\""));
    assert!(rendered.matches("branch bb1(").count() >= 6);
    assert!(rendered.matches("bb1(%v").count() >= 3);
    let spin = rendered
        .split("func \"spin\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("spin function must render");
    assert!(!spin.contains("return"));
}

#[test]
fn diagnostics_and_unsupported_bodies_fail_without_partial_programs() {
    let diagnostic = analyze("fun broken(input: Int): Int = missing");
    let error = lower_scalar_file(
        &diagnostic.sources,
        &diagnostic.parsed,
        &diagnostic.names,
        &diagnostic.typed,
        &diagnostic.owned,
    )
    .err()
    .expect("frontend diagnostics must gate lowering");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let numeric_when = analyze(
        "fun numeric(input: Int): Int = when (input) {\n\
             1 -> 2\n\
             else -> 3\n\
         }",
    );
    assert!(numeric_when.typed.diagnostics().is_empty());
    let error = lower_scalar_file(
        &numeric_when.sources,
        &numeric_when.parsed,
        &numeric_when.names,
        &numeric_when.typed,
        &numeric_when.owned,
    )
    .err()
    .expect("non-Boolean when remains outside the scalar control-flow slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let elvis = analyze("fun choose(input: Int?): Int = input ?: 0");
    assert!(elvis.typed.diagnostics().is_empty());
    assert!(elvis.owned.diagnostics().is_empty());
    assert!(elvis.owned.deferred().is_empty());
    let error = lower_scalar_file(
        &elvis.sources,
        &elvis.parsed,
        &elvis.names,
        &elvis.typed,
        &elvis.owned,
    )
    .err()
    .expect("Elvis has valid frontend facts but remains outside native lowering");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let invalid_jump = analyze("fun invalid(): Unit { break }");
    assert_eq!(invalid_jump.typed.diagnostics().len(), 1);
    assert_eq!(
        invalid_jump.typed.diagnostics()[0].code().to_string(),
        "L0142"
    );
    let error = lower_scalar_file(
        &invalid_jump.sources,
        &invalid_jump.parsed,
        &invalid_jump.names,
        &invalid_jump.typed,
        &invalid_jump.owned,
    )
    .err()
    .expect("Phase 2 jump diagnostics must gate SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let for_loop = analyze("fun iterate(): Unit { for (item in listOf(1)) {} }");
    assert!(for_loop.typed.diagnostics().is_empty());
    assert!(for_loop.owned.diagnostics().is_empty());
    assert_eq!(for_loop.owned.iterations().len(), 1);
    let program = lower_scalar_file(
        &for_loop.sources,
        &for_loop.parsed,
        &for_loop.names,
        &for_loop.typed,
        &for_loop.owned,
    )
    .expect("for loops are now lowered to valid SSA");
    super::verify::verify_program(&program).expect("for loop SSA must verify");

    let string_instance = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun text(own input: String): String = identity(input)",
    );
    assert!(string_instance.typed.diagnostics().is_empty());
    assert!(string_instance.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &string_instance.sources,
        &string_instance.parsed,
        &string_instance.names,
        &string_instance.typed,
        &string_instance.owned,
    )
    .expect("String is now a first-class concrete generic instance");
    assert!(render_program(&program).contains("identity<String>"));
}

fn llvm_function_body<'a>(llvm: &'a str, source_name: &str) -> &'a str {
    let marker = format!(".{source_name}(");
    llvm.split("define ")
        .skip(1)
        .find(|definition| {
            definition
                .split_once('{')
                .is_some_and(|(header, _)| header.contains(&marker))
        })
        .and_then(|definition| definition.split("\n}").next())
        .expect("LLVM function definition")
}

#[test]
fn nullable_when_consumes_inner_without_duplicate_owner() {
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\nfun consume(own node: {inner}): Unit {{}}\nfun inspect(own node: {inner}?): Unit {{ when (node) {{ null -> {{}}; else -> {{ consume(node) }} }} }}"
        ));
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .expect("consume the proven inner");
        let ssa = render_program(&program);
        assert_eq!(ssa.matches("nullable.take").count(), 1, "{ssa}");
        render_verified_program(&program).expect("unique extracted owner verifies in LLVM");
    }
}

#[test]
fn non_null_assertion_owned_pointer_uses_proven_take_and_direct_abort() {
    // The assertion transfers the original nullable owner, without allocating or retaining it.
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\nfun inspect(own node: {inner}?): {inner} = node!!"
        ));
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        assert_eq!(analysis.owned.non_null_assertions().len(), 1);
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .expect("owned pointer assertion must lower from its ownership plan");
        let ssa = render_program(&program);
        assert_eq!(ssa.matches("nullable.take").count(), 1, "{ssa}");
        assert_eq!(ssa.matches("abort @source(").count(), 1, "{ssa}");
        let llvm = render_verified_program(&program).expect("assertion SSA verifies before LLVM");
        assert!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert!(!ssa.contains("rc.retain"), "{ssa}");
    }
}

#[test]
fn non_null_assertion_preserves_pending_value_arguments_across_its_branch() {
    // The first owner has been evaluated, but only the eventual call consumes it.
    let analysis = analyze(
        "class Node()\nfun take(own first: Node, own second: Node): Node = first\nfun inspect(own first: Node, own second: Node?): Node = take(first, second!!)",
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("both owners must reach the call in the proven branch");
    render_verified_program(&program).expect("pending owner uses the current block parameter");
}

#[test]
fn non_null_assertion_temporary_group_and_borrowed_call_results_keep_one_owner() {
    for body in [
        "fun inspect(): Node = make()!!",
        "fun inspect(own node: Node?): Node = ((node))!!",
        "fun inspect(own node: Node?): Unit = observe(Node(), node!!)",
        "fun inspect(own node: Node?, flag: Boolean): Node = (if (flag) { node } else { make() })!!",
        "fun inspect(own probe: Node?, own node: Node?): Node = (if (probe == null) { node } else { make() })!!",
    ] {
        let analysis = analyze(&format!(
            "class Node()\nfun make(): Node? {{ val result: Node? = Node()\nreturn result }}\nfun observe(first: Node, second: Node): Unit {{}}\n{body}"
        ));
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.owned.diagnostics()
        );
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap_or_else(|error| panic!("{body}: {error:?}"));
        render_verified_program(&program).expect("operand and call-result owners remain unique");
    }
}

#[test]
fn non_null_assertion_later_transfer_cleans_pending_value_owner() {
    for body in [
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { return Node() } else { second })!!)",
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node { loop { val result = take(first, (if (flag) { break } else { second })!!)\nbreak }\nreturn Node() }",
        "fun inspect(flag: Boolean): Node { loop { val first = Node()\nval second: Node? = Node()\nval result = take(first, (if (flag) { continue } else { second })!!)\nbreak }\nreturn Node() }",
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { loop { break }\nsecond } else { second })!!)",
    ] {
        let analysis = analyze(&format!(
            "class Node()\nfun take(own first: Node, own second: Node): Node = first\n{body}"
        ));
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.owned.diagnostics()
        );
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap_or_else(|error| panic!("{body}: {error:?}"));
        render_verified_program(&program)
            .expect("only escaping control transfers clean pending owners");
    }
}

#[test]
fn non_null_assertion_does_not_register_unreachable_inline_types() {
    let analysis =
        analyze("fun inspect(): Int { return 0\nval unused: Int? = 1\nval asserted = unused!! }");
    assert!(analysis.typed.diagnostics().is_empty());
    assert_eq!(analysis.typed.non_null_assertions().len(), 1);
    assert!(analysis.owned.non_null_assertions().is_empty());
    lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("unreachable typed facts cannot enable or reject runtime layouts");
}

#[path = "constant_lowering_tests.rs"]
mod constant_lowering_tests;

#[test]
fn rejects_produced_guarded_cleanup_before_lowering_bindings() {
    let analysis = analyze(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val f: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })\nval used = f() }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    assert!(
        analysis
            .owned
            .drops()
            .iter()
            .any(|fact| fact.condition().is_some()),
        "frontend must produce real guarded cleanup"
    );
    let first = analysis
        .owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                ..
            } => analysis.owned.cleanup_conditions().owner_snapshot(*owner),
            _ => None,
        })
        .expect("frontend must publish the saved owner choices");
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("saved choices require an explicit SSA carrier"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(first.origin()));
}

#[test]
fn rejects_conditional_capture_instance_before_narrow_closure_lowering() {
    use lang_frontend::ownership_checking::{CleanupCondition, IterationCleanupAction};

    let analysis = analyze(
        "fun run(flag: Boolean) { val text = \"x\"\nif (flag) { val action: move () -> Unit = move { println(text) }\nval used = action() } }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let input = analysis
        .owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture { input, .. }
                if !matches!(
                    analysis.owned.cleanup_conditions().get(input.condition()),
                    Some(CleanupCondition::Always)
                ) =>
            {
                Some(*input)
            }
            _ => None,
        })
        .expect("the branch must publish a guarded capture instance");
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("the narrow native bridge cannot select a dynamic capture source"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(input.origin()));
}

#[test]
fn rejects_loop_carried_last_capture_loan_before_lowering_bindings() {
    let analysis = analyze(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val xs = listOf(1)
                { f = ({ read(xs) }) }
                { g = ({ read(xs) }) }
                break
            }
            val first = f()
            val second = g()
        }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    assert!(analysis.owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        lang_frontend::ownership_checking::IterationCleanupAction::TestLastCaptureLoan { .. }
    )));
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("last capture loan needs an executable runtime selector"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        analysis.sources.slice(error.span.unwrap()).unwrap(),
        "xs",
        "native must reject the unavailable runtime last-loan choice"
    );
}

#[test]
fn bitwise_inv_retains_the_existing_single_file_integer_index_boundary() {
    for expression in ["values[0]", "values[0].inv()"] {
        let analysis = analyze(&format!("fun entry(values: List<Int>): Int = {expression}"));
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        let error = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .err()
        .expect("integer index is an existing single-file lowering boundary");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[test]
fn bitwise_inv_does_not_reinterpret_unsupported_single_file_source_members() {
    for method in ["ordinary", "inv"] {
        let analysis = analyze(&format!(
            "class Sample {{ fun {method}(): Int = 7 }}\nfun entry(): Int = Sample().{method}()"
        ));
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        assert!(analysis.typed.integer_operations().is_empty());
        let error = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .err()
        .expect("source member call retains single-file lowering boundary");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[path = "root_primitive_lower_tests.rs"]
mod root_primitive_tests;

#[path = "field_replace_lower_tests.rs"]
mod field_replace_tests;
