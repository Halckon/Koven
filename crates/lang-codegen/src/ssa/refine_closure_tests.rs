//! SPEC-0364 closures in the refinement layer (ADR-0034): Borrow function parameters are
//! callable views; lambda literals are formed as function pointers or closures and lent.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Operation, Program},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

/// Lowers `source` (package `p`, entry `main`) and returns the program and the functions
/// the refinement layer lowered.
fn lower(source: &str) -> (Program, Vec<String>, crate::ssa::model::FunctionId) {
    let ((program, entry), routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("closures lower and verify")
    });
    (program, routed, entry)
}

fn run_counted(
    program: &Program,
    entry: crate::ssa::model::FunctionId,
    allocations: usize,
    stdout: &[u8],
) {
    let llvm =
        crate::llvm::render_verified_program_with_entry(program, entry).expect("verified LLVM");
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
    crate::native_tests::boxed_enum_tests::assert_success(&output, stdout);
}

#[test]
fn lambda_arguments_reach_callees_as_views() {
    let source = r#"class Data(val v: Int)
    fun apply(f: (Int) -> Int, x: Int): Int = f(x)
    fun twice(f: (Int) -> Int): Int = f(f(1))
    fun relay(f: (Int) -> Int): Int = apply(f, 10)
    fun main(): Unit {
        val k = 3
        if (apply({ n -> n * k }, 7) == 21 && twice({ n -> n + 1 }) == 3) { println("views") }
        if (relay({ n -> n + k }) == 13) { println("forward") }
        val d = Data(5)
        if (apply({ n -> n + d.v }, 1) == 6) { println("shared") }
        val e = Data(9)
        if (apply(move { n -> n + e.v }, 1) == 10) { println("owned") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in [
        "koven.p.main.d",
        "koven.p.apply.d",
        "koven.p.twice.d",
        "koven.p.relay.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    let module = &program.modules[0];
    let count = |predicate: fn(&Operation) -> bool| {
        module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|instruction| predicate(&instruction.operation))
            .count()
    };
    // Five lambda arguments become views; `relay` forwards its view by re-borrowing it.
    assert_eq!(
        count(|op| matches!(op, Operation::CallableViewOf { .. })),
        5
    );
    assert_eq!(
        count(|op| matches!(op, Operation::CallableInvokeView { .. })),
        3
    );
    assert_eq!(
        count(|op| matches!(op, Operation::FunctionAddress { .. })),
        1,
        "only the capture-free lambda is a bare function pointer"
    );
    // `d` and the moved `e` are each freed exactly once.
    run_counted(&program, entry, 2, b"views\nforward\nshared\nowned\n");
}

#[test]
fn a_factory_returns_a_move_closure_with_its_concrete_layout() {
    let source = r#"class Data(val v: Int)
    fun make(own s: String): () -> Boolean = move { s == "x" }
    fun adder(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun main(): Unit {
        val f = make("x")
        if (f()) { println("move") }
        val g = adder(Data(4))
        if (g(1) == 5) { println("data") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in ["koven.p.make.d", "koven.p.adder.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    let module = &program.modules[0];
    for name in ["koven.p.make.d", "koven.p.adder.d"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name.starts_with(name))
            .expect("factory");
        assert!(
            matches!(
                function.return_types.as_slice(),
                [ty] if module.concrete_closure(*ty).is_some()
            ),
            "{name} returns its concrete closure"
        );
    }
    // String literals do not allocate; the Data captured by `g` is freed exactly once, with
    // its closure.
    run_counted(&program, entry, 1, b"move\ndata\n");
}
