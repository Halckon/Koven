//! Shared normal-Unit routing observation; no lowering mode override.
use super::*;
use crate::ssa::model::{EntityId, Origin, TerminatorKind};

fn lower(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    entry_name: &str,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let (name_environment, mut type_environment) = standard_environments();
    for input in inputs {
        let _ = type_environment.authorize_range_source(sources, input.source_id());
        let _ = type_environment.authorize_range_extension_source(sources, input.source_id());
    }
    let (names, typed, owned) = analyze(sources, inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", entry_name);
    let (result, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(
            sources,
            inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    (result.map(|(program, _)| program), routed)
}

pub(super) fn routes_from_entry(
    source: &str,
    entry_name: &str,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    lower(&sources, &inputs, entry_name)
}

pub(super) fn routes_with_sibling(
    source: &str,
    sibling: &str,
    reverse: bool,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
    let (other_id, other) = parsed(
        &mut sources,
        "q/unused.ko",
        &format!("package q\n{sibling}"),
    );
    let mut inputs = [
        SourceUnitInput::new("root", "p/main.ko", id, &file),
        SourceUnitInput::new("root", "q/unused.ko", other_id, &other),
    ];
    if reverse {
        inputs.reverse();
    }
    lower(&sources, &inputs, "entry")
}

pub(super) fn route(source: &str) -> (Result<Program, LoweringError>, bool) {
    let (result, routed) = routes_from_entry(source, "f");
    let refined = routed.iter().any(|name| name.starts_with("koven.p.f.d"));
    (result, refined)
}

#[test]
fn cross_file_compatibility_loop_and_refined_resource_writeback_keep_distinct_routes() {
    let provider = r#"
class Guard(val label: String) { deinit() { println(label) } }
class Holder(var item: Guard)
fun change(inout slot: Guard): Unit { slot = Guard("drop:new") }
fun loop_messages(): Unit {
    for (s in arrayOf("loop:a", "loop:b")) { println(s) }
}
"#;
    let caller = r#"
import q.Guard
import q.Holder
import q.change
import q.loop_messages
fun entry(): Unit {
    val h = Holder(Guard("drop:old"))
    loop_messages()
    change(&h.item)
    println("after")
}
"#;

    let lower = |reverse| {
        let (result, mut routes) = routes_with_sibling(caller, provider, reverse);
        let program = result.expect("cross-file resource writeback must lower");
        crate::ssa::verify::verify_program(&program).expect("ownership and SSA verification");
        let module = &program.modules[0];
        let find = |prefix: &str| {
            module
                .functions
                .iter()
                .find(|function| function.name.starts_with(prefix))
                .unwrap_or_else(|| panic!("missing reachable function {prefix}"))
        };
        let entry = find("koven.p.entry.d");
        let change = find("koven.q.change.d");
        let loop_messages = find("koven.q.loop_messages.d");

        for callee in [change, loop_messages] {
            assert_eq!(
                entry
                    .instructions
                    .iter()
                    .filter(|instruction| matches!(
                        instruction.operation,
                        Operation::DirectCall { callee: id, .. } if id == callee.id
                    ))
                    .count(),
                1,
                "entry must reach {} through its planned call",
                callee.name
            );
        }
        assert!(routes.contains(&entry.name), "{routes:?}");
        assert!(routes.contains(&change.name), "{routes:?}");
        assert!(!routes.contains(&loop_messages.name), "{routes:?}");
        routes.sort();
        (render_program(&program), routes)
    };
    assert_eq!(lower(false), lower(true));
}

#[test]
fn nullable_float_equality_uses_both_routes_and_guards_payload_reads() {
    for (source, refined) in [
        ("fun f(a: Double?, b: Double): Boolean = a == b", true),
        (
            r#"fun f(a: Double?, b: Double): Unit {
    for (s in arrayOf("loop")) {
        if (b == a) { println(s) }
    }
}"#,
            false,
        ),
    ] {
        let (program, routed) = route(source);
        assert_eq!(routed, refined, "{source}");
        let llvm = crate::llvm::render_verified_program(&program.expect("comparison lowers"))
            .expect("comparison LLVM verifies");
        let (before, present) = llvm
            .split_once("\nnullable.compare.present:")
            .expect("the tag test has a dedicated present edge");
        let (present, _) = present
            .split_once("\nnullable.compare.join:")
            .expect("both comparison outcomes join");
        assert!(before.contains("label %nullable.compare.join, label %nullable.compare.present"));
        assert!(!before.contains("nullable.payload = extractvalue"));
        assert!(
            present.contains("nullable.payload = extractvalue")
                && present.contains("fcmp oeq double")
        );
    }
}

#[test]
fn compatibility_for_joins_plain_and_nullable_floats() {
    for (ty, literal) in [("Float", "1.0f"), ("Double", "1.0")] {
        let source = format!(
            r#"fun f(flag: Boolean, plain: {ty}, wrapped: {ty}?): Unit {{
    for (label in arrayOf("loop")) {{
        val picked: {ty}? = if (flag) plain else wrapped
        if (picked == plain) {{ println(label) }} else {{ println("null") }}
    }}
}}
fun main(): Unit {{
    f(true, {literal}, null)
    f(false, {literal}, null)
    f(false, {literal}, {literal})
}}"#
        );
        let (program, refined) = route(&source);
        assert!(!refined, "{ty} uses the production Compatibility route");
        let program = program.expect("both continuing branches reach an inline nullable join");
        crate::ssa::verify::verify_program(&program).expect("nullable float join SSA verifies");
        let run = crate::native_tests::emit_link_and_run("nullable-float-join.ko", &source, "main");
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"loop\nnull\nloop\n");
    }
}

#[test]
fn compatibility_char_interpolation_uses_the_owned_accumulator() {
    let source = r#"fun f(value: Char): Unit {
    for (label in arrayOf("loop")) {
        println(label)
        println("<${value}${value}>")
    }
}
fun main(): Unit { f('é') }"#;
    let (program, refined) = route(source);
    assert!(!refined, "this ordinary loop must use Compatibility");
    let program = program.expect("Compatibility Char interpolation lowers");
    let appends = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.instructions)
        .filter(|i| matches!(i.operation, Operation::StringAppend { .. }))
        .count();
    assert_eq!(appends, 3);
    let run =
        crate::native_tests::emit_link_and_run("compat-char-interpolation.ko", source, "main");
    crate::native_tests::boxed_enum_tests::assert_success(&run, "loop\n<éé>\n".as_bytes());
}

#[test]
fn null_only_assertion_aborts_without_creating_a_payload_proof_in_either_route() {
    for (body, refined) in [
        ("missing()!!", true),
        (
            "for (label in arrayOf(\"loop\")) { println(label); missing()!! }",
            false,
        ),
    ] {
        let source = format!("fun missing(): Nothing? = null\nfun f(): Unit {{ {body} }}",);
        let (result, routed) = route(&source);
        assert_eq!(routed, refined, "{source}");
        let program = result.expect("ordinary SSA lowers the null-only assertion");
        crate::ssa::verify::verify_program(&program).expect("complete SSA verification");
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.starts_with("koven.p.f.d"))
            .expect("entry");
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(instruction.operation, Operation::DirectCall { .. })
                })
                .count(),
            1,
            "evaluate the original operand exactly once",
        );
        assert!(!function.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::NullableTake { .. })
        }));
        let aborts = function
            .blocks
            .iter()
            .filter_map(|block| {
                let terminator = block.terminator.as_ref().unwrap();
                assert!(!matches!(
                    terminator.kind,
                    TerminatorKind::NullableBranch { .. }
                ));
                terminator.abort.as_ref()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            aborts,
            [&crate::ssa::model::AbortReason::Text(
                "null assertion failed".to_owned()
            )],
        );
    }
}

#[test]
fn mixed_nullable_text_comparison_reads_presence_in_both_routes() {
    for (body, refined) in [
        ("return text() == missing()", true),
        (
            r#"for (label in arrayOf("loop")) { return missing() != text() }; return false"#,
            false,
        ),
    ] {
        let source = format!(
            "fun missing(): Nothing? = null\nfun text(): String? = \"\"\nfun f(): Boolean {{ {body} }}"
        );
        let (result, routed) = route(&source);
        assert_eq!(routed, refined, "{source}");
        let program = result.expect("both routes lower the comparison");
        crate::ssa::verify::verify_program(&program).expect("complete SSA verification");
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.starts_with("koven.p.f.d"))
            .expect("entry");
        let operations = function
            .instructions
            .iter()
            .map(|instruction| &instruction.operation)
            .collect::<Vec<_>>();
        let presence = operations
            .iter()
            .position(|operation| matches!(operation, Operation::NullableIsNull { .. }))
            .expect("read the nullable string tag");
        assert_eq!(
            operations[..presence]
                .iter()
                .filter(|operation| matches!(operation, Operation::DirectCall { .. }))
                .count(),
            2,
            "both original operands complete before the comparison",
        );
        assert!(!operations.iter().any(|operation| matches!(
            operation,
            Operation::StringEqual { .. } | Operation::NullableTake { .. }
        )));
    }
}

#[test]
fn compatibility_nullable_text_projection_retires_children_before_dropping_the_root() {
    use crate::ssa::model::EntityId;

    let source = r#"class Inner(val text: String?)
class Middle(val inner: Inner)
class Outer(val middle: Middle)
fun make(): Outer = Outer(Middle(Inner(null)))
fun missing(): Nothing? = null
fun f(): Unit {
    for (label in arrayOf("same")) {
        if (make().middle.inner.text == missing()) { println(label) }
    }
}"#;
    let (result, refined) = route(source);
    assert!(!refined, "exercise the ordinary compatibility route");
    let program = result.expect("lower the nested class projection");
    crate::ssa::verify::verify_program(&program).expect("complete SSA verification");
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.f.d"))
        .unwrap();
    let instructions = &function.instructions;
    let projected = instructions
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::SharedHeapFieldLoan { base, .. }, [EntityId::Loan(child)]) => {
                    Some((*base, *child))
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    let [(first, middle), (parent, last)] = projected.as_slice() else {
        panic!("expected two child projections, got {projected:?}");
    };
    assert_eq!(middle, parent);

    let ordinal = |predicate: &dyn Fn(&Operation) -> bool| {
        instructions
            .iter()
            .position(|instruction| predicate(&instruction.operation))
            .expect("operation in the comparison")
    };
    let presence =
        ordinal(&|op| matches!(op, Operation::NullableLoanIsNull { source } if source == last));
    let last_end = ordinal(&|op| matches!(op, Operation::BorrowEnd { loan } if loan == last));
    let middle_end = ordinal(&|op| matches!(op, Operation::BorrowEnd { loan } if loan == middle));
    let first_end = ordinal(&|op| matches!(op, Operation::BorrowEnd { loan } if loan == first));
    let owner = instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::HeapPayloadPlace { owner } => Some(owner),
            _ => None,
        })
        .expect("use the original temporary root");
    let drop = ordinal(&|op| matches!(op, Operation::Drop { owner: value } if *value == owner));
    assert!(
        presence < last_end && last_end < middle_end && middle_end < first_end && first_end < drop
    );
}

#[test]
fn borrowed_call_result_supports_nullable_field_chain_presence() {
    let source = r#"class Inner(val text: String?) { deinit() {} }
class Outer(val inner: Inner) { deinit() {} }
fun view(borrow outer: Outer): borrow Outer from outer = outer
fun missing(): Nothing? = null
fun f(): Unit {
    val owner = Outer(Inner(null))
    borrow val alias = view(owner)
    if (alias.inner.text == missing()) { println("equal") }
}"#;
    let (result, routed) = routes_from_entry(source, "f");
    let program = result.unwrap_or_else(|err| panic!("{err:?}; routes: {routed:?}"));
    assert!(routed.iter().any(|name| name.starts_with("koven.p.f.d")));
    crate::ssa::verify::verify_program(&program).expect("complete SSA verification");
}

#[test]
fn lexical_field_reborrow_does_not_admit_unrelated_controls() {
    let source = r#"class Part(val value: Int) { deinit() {} }
class Holder(val part: Part) { deinit() {} }
fun observe(own holder: Holder, flag: Boolean): Unit {
    borrow val alias = holder.part
    borrow val child = alias
    if (flag) { println("unrelated") }
    if (child.value == 1) { println("read") }
}
fun f(): Unit { observe(Holder(Part(1)), true) }
"#;
    let (result, routed) = routes_from_entry(source, "f");
    assert!(
        matches!(
            result,
            Err(LoweringError {
                kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
                ..
            })
        ),
        "an ordinary field alias is not evidence for a returned-field observer"
    );
    assert!(routed.is_empty(), "reject in the existing preflight");
}

#[test]
fn borrowed_field_comparison_handles_return_in_null_only_operand() {
    let source = r#"class Section(val text: String?) { deinit() {} }
class Owner(val selected: Section) { deinit() {} }
fun branch(borrow owner: Owner): borrow Section from owner = owner.selected
fun missing(): Nothing? = null
fun inspect(own owner: Owner, early: Boolean): Unit {
    borrow val alias = branch(owner)
    if (alias.text == (if (early) { return } else { missing() })) { println("equal") }
}
fun f(): Unit { inspect(Owner(Section(null)), true) }
"#;
    let (result, routed) = routes_from_entry(source, "f");
    let program = result.unwrap_or_else(|err| panic!("{err:?}; routes: {routed:?}"));
    assert!(
        routed
            .iter()
            .any(|name| name.starts_with("koven.p.inspect.d"))
    );
    crate::ssa::verify::verify_program(&program).expect("complete SSA verification");

    let unrelated = source.replace(
        "{ return }",
        r#"{ if (early) { println("unrelated") }; return }"#,
    );
    let (result, routed) = routes_from_entry(&unrelated, "f");
    assert!(matches!(
        result,
        Err(LoweringError {
            kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
            ..
        })
    ));
    assert!(
        routed.is_empty(),
        "an unrelated preceding if still requires its own proof"
    );
}

#[test]
fn borrowed_field_comparison_keeps_result_across_array_for() {
    let source = r#"class Inner(val text: String?) { deinit() {} }
class Outer(val inner: Inner) { deinit() {} }
fun view(borrow outer: Outer): borrow Outer from outer = outer
fun missing(): Nothing? = null
fun f(): Unit {
    val owner = Outer(Inner(null))
    borrow val alias = view(owner)
    for (label in arrayOf("first", "second")) {
        if (alias.inner.text == missing()) { println(label) }
    }
}"#;
    let named = source.replace(
        r#"    for (label in arrayOf("first", "second"))"#,
        "    val labels = arrayOf(\"first\", \"second\")\n    for (label in labels)",
    );
    let nested = source.replace(
        r#"    for (label in arrayOf("first", "second")) {
        if (alias.inner.text == missing()) { println(label) }
    }"#,
        r#"    for (outer in arrayOf("outer")) {
        for (label in arrayOf("first", "second")) {
            if (alias.inner.text == missing()) { println(label) }
        }
    }"#,
    );
    for source in [source, &named, &nested] {
        let (result, routed) = routes_from_entry(source, "f");
        let program = result.unwrap_or_else(|err| panic!("{err:?}; routes: {routed:?}"));
        assert!(routed.iter().any(|name| name.starts_with("koven.p.f.d")));
        crate::ssa::verify::verify_program(&program).expect("complete SSA verification");
    }
    let unrelated = source.replace(
        "    for (label",
        "    for (other in arrayOf(\"other\")) { println(other) }\n    for (label",
    );
    let (result, routed) = routes_from_entry(&unrelated, "f");
    assert!(matches!(
        result,
        Err(LoweringError {
            kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
            ..
        })
    ));
    assert!(
        routed.is_empty(),
        "one array observer cannot admit a sibling loop"
    );
}

#[test]
fn object_call_break_retires_the_argument_and_receiver_without_dispatch() {
    for body in [
        "loop { Gate.cond(7, break) }",
        "loop { while (Gate.cond(7, break)) {} }",
    ] {
        let text = format!(
            "package p\nobject Gate {{ fun cond(value: Int, stop: Boolean): Boolean = stop }}\nfun entry(): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/main.ko", &text);
        for input in [
            SourceUnitInput::new("root", "p/main.ko", source, &file),
            SourceUnitInput::memory_document(source, &file),
        ] {
            let (program, routed) = lower(&sources, &[input], "entry");
            let program = program.expect("object call prefix must lower");
            crate::ssa::verify::verify_program(&program).unwrap();
            let entry = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.starts_with("koven.p.entry.d"))
                .unwrap();
            assert!(routed.contains(&entry.name));
            assert!(
                entry.instructions.iter().all(|instruction| !matches!(
                    instruction.operation,
                    Operation::DirectCall { .. }
                )),
                "break prevents dispatch"
            );
            let loan_at = |text| {
                let mut matches = entry.instructions.iter().filter(|instruction| {
                    matches!(instruction.operation, Operation::BorrowBegin { .. })
                        && matches!(instruction.origin, Origin::Source(span)
                            if sources.slice(span) == Ok(text))
                });
                let instruction = matches.next().expect("actual prefix loan");
                assert!(matches.next().is_none());
                let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                    panic!("the prefix borrows exactly once at {text}");
                };
                *loan
            };
            let receiver = loan_at("Gate");
            let argument = loan_at("7");
            assert_ne!(receiver, argument);
            // Straight-line SSA folding may absorb the break branch. Its
            // cleanup instructions retain their source origin and block order.
            let ends = entry
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(instruction.operation, Operation::BorrowEnd { .. })
                        && matches!(instruction.origin, Origin::Source(span)
                            if sources.slice(span) == Ok("break"))
                })
                .collect::<Vec<_>>();
            let [first, second] = ends.as_slice() else {
                panic!("the break ends both loans exactly once");
            };
            assert_eq!(first.block, second.block);
            assert_eq!(first.operation, Operation::BorrowEnd { loan: argument });
            assert_eq!(second.operation, Operation::BorrowEnd { loan: receiver });
            let exit = &entry.blocks[first.block.index()];
            assert!(
                exit.instructions
                    .iter()
                    .position(|id| *id == first.id)
                    .unwrap()
                    < exit
                        .instructions
                        .iter()
                        .position(|id| *id == second.id)
                        .unwrap(),
                "the break ends the argument loan before the receiver loan"
            );
        }
    }
}

#[test]
fn singleton_call_exit_drops_the_completed_temporary_before_the_scope() {
    let source = r#"
class Marker(val label: String) { deinit() { println(label) } }
fun prefix(): Marker { println("prefix"); return Marker("drop-prefix") }
fun suffix(): Int { println("suffix"); return 0 }
object Gate {
    fun cond(MODEvalue: Marker, stop: Boolean, last: Int): Boolean {
        println("dispatch")
        return stop
    }
}
fun returning(): Unit {
    val frame = Marker("drop-return-frame")
    Gate.cond(prefix(), return, suffix())
}
fun main(): Unit {
    loop {
        val frame = Marker("drop-break-frame")
        while (Gate.cond(prefix(), break, suffix())) { println("body") }
    }
    var again = true
    while (again) {
        again = false
        val frame = Marker("drop-continue-frame")
        Gate.cond(prefix(), continue, suffix())
    }
    returning()
    Gate.cond(prefix(), false, suffix())
}"#;
    for mode in ["own ", ""] {
        let source = source.replace("MODE", mode);
        let run = crate::native_tests::emit_link_and_run("object-call-exits.ko", &source, "main");
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"prefix\ndrop-prefix\ndrop-break-frame\nprefix\ndrop-prefix\ndrop-continue-frame\nprefix\ndrop-prefix\ndrop-return-frame\nprefix\nsuffix\ndispatch\ndrop-prefix\n",
        );
    }
}

#[test]
fn singleton_mixed_abort_keeps_the_existing_receiver_boundary() {
    for body in [
        r#"Gate.choose(7, if (stop) error("stopped") else 0)"#,
        r#"loop { Gate.choose(7, if (stop) error("stopped") else break) }"#,
    ] {
        let source = format!(
            "object Gate {{ fun choose(first: Int, second: Int): Unit {{}} }}\n\
             fun f(stop: Boolean): Unit {{ {body} }}"
        );
        let (result, routed) = route(&source);
        assert!(!routed, "{body}");
        assert!(
            matches!(
                result,
                Err(LoweringError {
                    kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
                    ..
                })
            ),
            "{body}"
        );
    }
}

#[test]
fn singleton_exit_ignores_an_abort_outside_the_evaluated_prefix() {
    for body in [
        r#"loop { Gate.choose(7, break, error("unreached")) }"#,
        r#"loop { Gate.choose(7, if (flag) { break; error("unreached") } else break, 0) }"#,
    ] {
        let source = format!(
            "object Gate {{ fun choose(first: Int, second: Int, last: Int): Unit {{}} }}\n\
             fun f(flag: Boolean): Unit {{ {body} }}"
        );
        let (program, routed) = route(&source);
        assert!(routed, "{body}");
        crate::ssa::verify::verify_program(&program.expect("the ordinary break must lower"))
            .expect("the singleton exit's SSA verifies");
    }
}

#[test]
fn singleton_argument_return_keeps_the_projected_borrow_result_boundary() {
    let (result, routed) = routes_from_entry(
        r#"
class Packet(val text: String)
object Gate { fun inspect(prefix: Int, stop: Int): Unit {} }
fun choose(source: Packet, stop: Boolean): borrow String from source {
    Gate.inspect(7, if (stop) return source.text else 0)
    return source.text
}
fun entry(): Unit {
    val source = Packet("held".clone())
    borrow val value = choose(source, true)
    println(value)
}
"#,
        "entry",
    );
    assert!(
        !routed
            .iter()
            .any(|name| name.starts_with("koven.p.choose.d"))
    );
    assert!(matches!(
        result,
        Err(LoweringError {
            kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
            ..
        })
    ));
}
