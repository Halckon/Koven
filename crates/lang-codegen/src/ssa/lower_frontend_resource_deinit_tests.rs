//! Single-file resource destructors are real readonly callable bodies.

use super::{
    LoweringErrorKind, analyze, lower_scalar_file, render_program, render_verified_program,
};
use crate::ssa::model::{EntityType, LoanKind, Operation};

fn lower_resource(source: &str) -> crate::ssa::model::Program {
    let analysis = analyze(source);
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
    lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("resource deinit body must lower")
}

#[test]
fn single_resource_deinit_body_is_lowered_and_reads_live_fields() {
    let program = lower_resource(
        r#"fun record(value: Int): Unit {}
        class Resource(val value: Int) {
            deinit() { record(this.value) }
        }
        fun run(): Unit { val resource = Resource(7) }"#,
    );
    let module = &program.modules[0];
    let destructor = module
        .functions
        .iter()
        .find(|function| function.name.starts_with("__deinit."))
        .expect("resource must have a destructor body");
    assert!(matches!(
        destructor.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    assert!(destructor.return_types.is_empty());
    let record = module
        .functions
        .iter()
        .find(|function| function.name == "record")
        .unwrap();
    assert!(
        destructor
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation,
        Operation::DirectCall { callee, .. } if callee == record.id))
    );
    assert!(
        destructor.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::SharedHeapFieldLoan { .. }
        ))
    );
    render_verified_program(&program).expect("resource deinit LLVM verifies");
}

#[test]
fn single_resource_deinit_handles_nested_class_fields_and_body_generic_call_roots() {
    let source = r#"fun <T : Copyable> sink(own value: T): Unit {}
        fun <T : Copyable> record(own value: T): Unit { sink(value) }
        class Inner(val code: Int) { deinit() { record(this.code) } }
        class Outer(val inner: Inner, val label: String) {
            deinit() { record(this.inner.code); println(label) }
        }
        fun run(): Unit { val resource = Outer(Inner(7), "outer") }"#;
    let program = lower_resource(source);
    let module = &program.modules[0];
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|function| function.name.starts_with("__deinit."))
            .count(),
        2
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "record<Int>")
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.name == "sink<Int>")
    );
    assert_eq!(
        render_program(&program),
        render_program(&lower_resource(source))
    );
    render_verified_program(&program)
        .expect("nested resource fields and generic body calls verify");
}

#[test]
fn single_resource_deinit_can_forward_this_and_borrow_string_fields() {
    let program = lower_resource(
        r#"
        fun inspect(resource: Resource): Unit {}
        class Resource(val text: String) {
            deinit() { inspect(this); println(this.text); println(text.clone()) }
        }
        fun run(): Unit { val resource = Resource("live") }
    "#,
    );
    render_verified_program(&program).expect("readonly this and field borrows verify");
}

#[test]
fn single_resource_deinit_body_return_and_abort_are_explicit() {
    for body in ["println(text); return", "error(text)"] {
        let program = lower_resource(&format!(
            "class Resource(val text: String) {{ deinit() {{ {body} }} }}\nfun run(): Unit {{ val resource = Resource(\"live\") }}"
        ));
        render_verified_program(&program).expect("destructor body control flow verifies");
    }
}

#[test]
fn single_resource_deinit_rejects_unsupported_resource_wrappers() {
    for source in [
        "class Resource { deinit() {} }\nfun inspect(value: Resource?): Unit {}",
        "class Resource<T>(val value: T) { deinit() {} }\nfun run(): Unit { val resource = Resource(1) }",
        "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }",
        "class Resource { deinit() {} }\nclass Holder<T>(val value: T)\nfun run(): Unit { val wrapped = Holder(Resource()) }",
    ] {
        let analysis = analyze(source);
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{source}: {:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{source}: {:?}",
            analysis.owned.diagnostics()
        );
        let failure = match lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        ) {
            Ok(_) => panic!("unsupported resource representation must fail: {source}"),
            Err(error) => error,
        };
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        assert!(failure.span.is_some());
    }
}

#[test]
fn single_resource_deinit_supports_direct_sequence_elements_and_resource_value_fields() {
    for source in [
        "class Resource { deinit() {} }\nvalue class Wrapped(val resource: Resource)\nfun run(): Unit { val wrapped = Wrapped(Resource()) }",
        "class Resource { deinit() {} }\nfun inspect(value: Array<Resource>): Unit {}",
        "class Resource { deinit() {} }\nfun inspect(value: List<Resource>): Unit {}",
        "class Resource { deinit() {} }\nfun inspect(value: MutableList<Resource>): Unit {}",
    ] {
        let program = lower_resource(source);
        render_verified_program(&program).expect("direct resource storage verifies");
    }
}

#[test]
fn single_resource_deinit_registers_body_local_resource_cleanup_on_return() {
    for ending in ["", "return"] {
        let program = lower_resource(&format!(
            r#"
            class Local(val name: String) {{ deinit() {{ println(name) }} }}
            class Owner {{ deinit() {{ val local = Local("nested"); println("body"); {ending} }} }}
            fun run(): Unit {{ val owner = Owner() }}
        "#
        ));
        let module = &program.modules[0];
        let destructor = module
            .functions
            .iter()
            .find(|function| {
                function.name.starts_with("__deinit.")
                    && function.instructions.iter().any(|instruction| {
                        matches!(instruction.operation, Operation::HeapAllocate { .. })
                    })
            })
            .expect("body construction must be lowered");
        assert!(
            destructor
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        );
        render_verified_program(&program).expect("body-local resource and return cleanup verify");
    }
}

#[test]
fn single_resource_deinit_rejects_member_calls_and_nested_call_borrows() {
    for source in [
        "class Resource { fun observe(): Unit {}\n deinit() { this.observe() } }\nfun run(): Unit { val resource = Resource() }",
        "fun inspect(text: String): Unit {}\nclass Inner(val text: String)\nclass Outer(val inner: Inner) { deinit() { inspect(inner.text) } }\nfun run(): Unit { val outer = Outer(Inner(\"nested\")) }",
    ] {
        let analysis = analyze(source);
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{source}: {:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{source}: {:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{source}: {:?}",
            analysis.owned.diagnostics()
        );
        let failure = match lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        ) {
            Ok(_) => panic!("unsupported destructor body must be rejected"),
            Err(error) => error,
        };
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
        assert!(failure.span.is_some());
    }
}

#[test]
fn single_resource_deinit_rejects_partial_move_at_the_conditional_owner_origin() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    let analysis = analyze(
        r#"
        class Resource { deinit() { println("released") } }
        fun consume(own value: Resource): Unit {}
        fun run(flag: Boolean): Unit {
            val resource = Resource()
            if (flag) { consume(resource) }
            println("after branch")
        }
    "#,
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let resource = analysis
        .names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "resource")
        .expect("named resource binding")
        .id();
    let facts = analysis
        .owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(resource))
        .collect::<Vec<_>>();
    assert_eq!(
        facts.len(),
        1,
        "resource must remain live on the non-consuming path"
    );
    assert!(
        facts[0].condition().is_some(),
        "scope cleanup needs the saved branch choice"
    );
    assert!(
        matches!(facts[0].point(), DropPoint::AfterStatement(_)),
        "moving cleanup to the branch exit would violate lexical resource lifetime"
    );
    let failure = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Ok(_) => panic!("conditional owner transport is outside this native slice"),
        Err(error) => error,
    };
    assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(failure.span, Some(facts[0].value_origin()));
}

#[test]
fn single_resource_deinit_local_move_chain_transports_only_current_owner() {
    use crate::ssa::model::TerminatorKind;
    let program = lower_resource(
        r#"
        class Resource { deinit() {} }
        fun work(own stop: Boolean): Unit {
            val original = Resource()
            val moved = (original)
            val current = ((moved))
            if (stop) { return }
        }
    "#,
    );
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name == "work")
        .expect("work function");
    let branch = function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .find_map(|terminator| match &terminator.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true, when_false)),
            _ => None,
        })
        .expect("conditional return");
    for edge in [branch.0, branch.1] {
        assert_eq!(
            edge.arguments.len(),
            1,
            "only the current resource crosses each edge"
        );
    }
    render_verified_program(&program).expect("moved resource cleanup verifies on both exits");
}
