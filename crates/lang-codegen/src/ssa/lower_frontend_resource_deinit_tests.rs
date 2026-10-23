//! Single-source Unit resource destructors are real readonly callable bodies.
//! Legacy rejection, direct-invoked resource closure and raw DropPoint observations
//! retain their original pipeline.

use lang_frontend::name_resolution::SymbolKind;

use super::{
    LoweringErrorKind, analyze as analyze_legacy, lower_scalar_file, render_program,
    render_verified_program,
};
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        EntityId, EntityType, Function, FunctionId, LoanKind, Module, Operation, Origin, Program,
        SsaTypeKind,
    },
};

struct ResourceUnit {
    source: UnitSource,
    program: Program,
    entry: FunctionId,
}

impl ResourceUnit {
    fn module(&self) -> &Module {
        self.program
            .module(self.entry.module())
            .expect("entry module")
    }

    fn entry(&self) -> &Function {
        self.module().function(self.entry).expect("returned entry")
    }

    fn function(&self, name: &str) -> &Function {
        let declaration = self.source.declaration(name, SymbolKind::Function);
        let origin = Origin::Source(self.source.declaration_span(declaration));
        let functions = self
            .module()
            .functions
            .iter()
            .filter(|function| function.origin == origin)
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("expected one concrete function for original declaration {name}");
        };
        *function
    }

    fn deinit(&self, name: &str) -> &Function {
        let declaration = self.source.declaration(name, SymbolKind::Classifier);
        let span = self.source.declaration_span(declaration);
        let module = self.module();
        let functions = module
            .functions
            .iter()
            .filter(|function| {
                let Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }) = function.receiver()
                else {
                    return false;
                };
                let Origin::Source(origin) = &function.origin else {
                    return false;
                };
                module.deinit(target) == Some(function.id())
                    && origin.source_id() == span.source_id()
                    && span.start() <= origin.start()
                    && origin.end() <= span.end()
            })
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("expected one registered deinit within original class {name}");
        };
        *function
    }
}

fn lower_resource(source: &str, entry_name: &str) -> ResourceUnit {
    let source = UnitSource::analyze("lowering.ko", source).expect("resource Unit frontend");
    source.assert_frontend_clean();
    let declaration = source.declaration(entry_name, SymbolKind::Function);
    let (program, entry) = source
        .lower_entry(declaration)
        .expect("resource deinit body must lower through its real Unit capability");
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        Origin::Source(source.declaration_span(declaration))
    );
    ResourceUnit {
        source,
        program,
        entry,
    }
}

// Only the unsupported direct-invoked resource closure retains this legacy path.
fn lower_resource_legacy(source: &str) -> crate::ssa::model::Program {
    let analysis = analyze_legacy(source);
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
    let unit = lower_resource(
        r#"fun record(value: Int): Unit {}
        class Resource(val value: Int) {
            deinit() { record(this.value) }
        }
        fun run(): Unit { val resource = Resource(7) }"#,
        "run",
    );
    let destructor = unit.deinit("Resource");
    assert!(matches!(
        destructor.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    assert!(destructor.return_types.is_empty());
    let record = unit.function("record");
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
    render_verified_program(&unit.program).expect("resource deinit LLVM verifies");
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
    let unit = lower_resource(source, "run");
    let module = unit.module();
    let inner = unit.deinit("Inner");
    let outer = unit.deinit("Outer");
    assert_ne!(inner.id(), outer.id());
    assert_eq!(module.deinits.len(), 2);
    let record = unit.function("record");
    let sink = unit.function("sink");
    for function in [record, sink] {
        let block = function.block(function.entry_block().unwrap()).unwrap();
        let [parameter] = block.parameters.as_slice() else {
            panic!("the original generic helper has exactly one Int argument");
        };
        let EntityType::Value(ty) = function.entity(*parameter).unwrap().ty else {
            panic!("the original generic helper owns its Copyable Int argument");
        };
        assert_eq!(
            module.type_kind(ty),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
    }
    for (caller, callee) in [(inner, record), (outer, record), (record, sink)] {
        assert!(caller.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::DirectCall { callee: target, .. } if target == callee.id()
        )));
    }
    assert_eq!(
        render_program(&unit.program),
        render_program(&lower_resource(source, "run").program)
    );
    render_verified_program(&unit.program)
        .expect("nested resource fields and generic body calls verify");
}

#[test]
fn single_resource_deinit_can_forward_this_and_borrow_string_fields() {
    let unit = lower_resource(
        r#"
        fun inspect(resource: Resource): Unit {}
        class Resource(val text: String) {
            deinit() { inspect(this); println(this.text); println(text.clone()) }
        }
        fun run(): Unit { val resource = Resource("live") }
    "#,
        "run",
    );
    render_verified_program(&unit.program).expect("readonly this and field borrows verify");
}

#[test]
fn single_resource_deinit_body_return_and_abort_are_explicit() {
    // The complete mixed Return/Abort whole remains CI-only.
    for body in ["println(text); return", "error(text)"] {
        let unit = lower_resource(
            &format!(
                "class Resource(val text: String) {{ deinit() {{ {body} }} }}\nfun run(): Unit {{ val resource = Resource(\"live\") }}"
            ),
            "run",
        );
        render_verified_program(&unit.program).expect("destructor body control flow verifies");
    }
}

#[test]
fn single_resource_deinit_rejects_unsupported_resource_wrappers() {
    for source in [
        "class Resource { deinit() {} }\nfun inspect(value: Resource?): Unit {}",
        "class Resource<T>(val value: T) { deinit() {} }\nfun run(): Unit { val resource = Resource(1) }",
        "class Resource { deinit() {} }\nclass Holder<T>(val value: T)\nfun run(): Unit { val wrapped = Holder(Resource()) }",
    ] {
        let analysis = analyze_legacy(source);
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
fn single_resource_deinit_inside_owned_closure_uses_body_and_environment_cleanup() {
    // Legacy-only: direct callable invocation does not select resource lambdas
    // in the Unit planner. Keep the complete original body/environment oracle.
    let program = lower_resource_legacy(
        "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }",
    );
    let module = &program.modules[0];
    let thunk = module
        .functions
        .iter()
        .find(|function| function.name.ends_with(".thunk"))
        .expect("one selected owned closure thunk");
    assert_eq!(
        thunk
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        1
    );
    let body_drops = thunk
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some(thunk.entity(EntityId::Value(owner)).unwrap().ty)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [EntityType::Value(resource)] = body_drops.as_slice() else {
        panic!("the callback's local Resource must have one drop")
    };
    assert!(matches!(
        module.type_kind(*resource),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("run");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::CallableInvoke { .. }))
            .count(),
        1
    );
    let entry_drops = entry
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some(entry.entity(EntityId::Value(owner)).unwrap().ty)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [EntityType::Value(closure)] = entry_drops.as_slice() else {
        panic!("the closure must independently drop its owned String environment once")
    };
    assert!(matches!(
        module.type_kind(*closure),
        Some(SsaTypeKind::ConcreteClosure { .. })
    ));
    render_verified_program(&program)
        .expect("body Resource and owned environment cleanup verify through LLVM");
}

#[test]
fn single_resource_deinit_supports_direct_sequence_elements_and_resource_value_fields() {
    for (entry, source) in [
        (
            "run",
            "class Resource { deinit() {} }\nvalue class Wrapped(val resource: Resource)\nfun run(): Unit { val wrapped = Wrapped(Resource()) }",
        ),
        (
            "inspect",
            "class Resource { deinit() {} }\nfun inspect(value: Array<Resource>): Unit {}",
        ),
        (
            "inspect",
            "class Resource { deinit() {} }\nfun inspect(value: List<Resource>): Unit {}",
        ),
        (
            "inspect",
            "class Resource { deinit() {} }\nfun inspect(value: MutableList<Resource>): Unit {}",
        ),
    ] {
        let unit = lower_resource(source, entry);
        render_verified_program(&unit.program).expect("direct resource storage verifies");
    }
}

#[test]
fn single_resource_deinit_registers_body_local_resource_cleanup_on_return() {
    for ending in ["", "return"] {
        let unit = lower_resource(
            &format!(
                r#"
            class Local(val name: String) {{ deinit() {{ println(name) }} }}
            class Owner {{ deinit() {{ val local = Local("nested"); println("body"); {ending} }} }}
            fun run(): Unit {{ val owner = Owner() }}
        "#
            ),
            "run",
        );
        let destructor = unit.deinit("Owner");
        assert!(
            destructor.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::HeapAllocate { .. })
            }),
            "body construction must be lowered"
        );
        assert!(
            destructor
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        );
        render_verified_program(&unit.program)
            .expect("body-local resource and return cleanup verify");
    }
}

#[test]
fn single_resource_deinit_rejects_member_calls_and_nested_call_borrows() {
    for source in [
        "class Resource { fun observe(): Unit {}\n deinit() { this.observe() } }\nfun run(): Unit { val resource = Resource() }",
        "fun inspect(text: String): Unit {}\nclass Inner(val text: String)\nclass Outer(val inner: Inner) { deinit() { inspect(inner.text) } }\nfun run(): Unit { val outer = Outer(Inner(\"nested\")) }",
    ] {
        let analysis = analyze_legacy(source);
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
    let analysis = analyze_legacy(
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
    let unit = lower_resource(
        r#"
        class Resource { deinit() {} }
        fun work(own stop: Boolean): Unit {
            val original = Resource()
            val moved = (original)
            val current = ((moved))
            if (stop) { return }
        }
    "#,
        "work",
    );
    let function = unit.entry();
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [stop @ EntityId::Value(_)] = entry_block.parameters.as_slice() else {
        panic!("work retains its one original Boolean Value parameter");
    };
    assert_eq!(
        unit.module()
            .type_kind(function.entity(*stop).unwrap().ty.semantic_type()),
        Some(&SsaTypeKind::Boolean)
    );
    let allocations = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
        .collect::<Vec<_>>();
    let [allocation] = allocations.as_slice() else {
        panic!("the move chain retains exactly one resource construction");
    };
    let [resource @ EntityId::Value(_)] = allocation.results.as_slice() else {
        panic!("resource construction produces one actual owner");
    };
    assert_eq!(
        unit.source
            .sources()
            .slice(allocation.origin.span())
            .unwrap(),
        "Resource()"
    );
    let resource_type = function.entity(*resource).unwrap().ty.semantic_type();
    assert_eq!(
        unit.module().deinit(resource_type),
        Some(unit.deinit("Resource").id())
    );
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
        // Unit also carries the original Copyable stop binding. Its extra slot
        // must not hide a duplicated or stale resource alias from the move chain.
        assert_eq!(
            edge.arguments.len(),
            2,
            "stop and the current resource only"
        );
        assert_eq!(
            edge.arguments
                .iter()
                .filter(|argument| *argument == stop)
                .count(),
            1
        );
        let owners = edge
            .arguments
            .iter()
            .filter(|argument| *argument != stop)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            owners.len(),
            1,
            "only the current resource crosses each edge"
        );
        assert_eq!(owners, [*resource]);
    }
    render_verified_program(&unit.program).expect("moved resource cleanup verifies on both exits");
}

#[test]
fn single_resource_deinit_control_body_nested_scope_consumes_published_drops() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    let source = r#"
        class Resource { deinit() {} }
        fun work(outer: Boolean, inner: Boolean): Unit {
            if (outer) {
                if (inner) {
                    val first = Resource()
                    val second = Resource()
                    println("inner")
                }
                println("outer")
            }
            println("done")
        }
    "#;
    // Legacy observation only: keep the raw DropPoint/order oracle intact.
    // The independent Unit lowering below receives no facts from this analysis.
    let analysis = analyze_legacy(source);
    let resource_drops = analysis
        .owned
        .drops()
        .iter()
        .filter_map(|fact| {
            let DropTarget::Named(symbol) = fact.target() else {
                return None;
            };
            let name = analysis
                .names
                .symbols()
                .iter()
                .find(|entry| entry.id() == symbol)?
                .name();
            matches!(name, "first" | "second").then_some((name, fact.point()))
        })
        .collect::<Vec<_>>();
    assert_eq!(resource_drops.len(), 2);
    assert_eq!(resource_drops[0].0, "second");
    assert_eq!(resource_drops[1].0, "first");
    assert_eq!(resource_drops[0].1, resource_drops[1].1);
    assert!(matches!(resource_drops[0].1, DropPoint::AfterStatement(_)));
    let unit = lower_resource(source, "work");
    render_verified_program(&unit.program)
        .expect("nested normal scope exits consume frontend drops");
}

#[test]
fn single_resource_deinit_control_body_tail_owner_survives_other_local_cleanup() {
    let unit = lower_resource(
        r#"
        class Resource { deinit() {} }
        fun choose(own flag: Boolean): Resource {
            val result = if (flag) {
                val spare = Resource()
                val chosen = Resource()
                chosen
            } else { Resource() }
            return result
        }
    "#,
        "choose",
    );
    let function = unit.entry();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "only the spare is dropped; the branch tail is delivered to the caller"
    );
    render_verified_program(&unit.program).expect("MoveOnly tail delivery precedes scope cleanup");
}

#[test]
fn single_resource_deinit_control_body_when_scopes_consume_local_resources() {
    for control in [
        "when { flag -> { val first = Resource(); val second = Resource(); println(\"branch\") }; else -> {} }",
        "when (flag) { true -> { val first = Resource(); val second = Resource(); println(\"branch\") }; false -> {} }",
    ] {
        let unit = lower_resource(
            &format!(
                "class Resource {{ deinit() {{}} }}\nfun work(flag: Boolean): Unit {{ {control}; println(\"done\") }}"
            ),
            "work",
        );
        render_verified_program(&unit.program).expect("when body consumes its scope cleanup facts");
    }
}

#[test]
fn single_resource_deinit_control_body_return_uses_only_transfer_cleanup() {
    let unit = lower_resource(
        r#"
        class Resource { deinit() {} }
        fun work(outer: Boolean, inner: Boolean): Unit {
            if (outer) {
                if (inner) {
                    val first = Resource()
                    val second = Resource()
                    return
                }
            }
        }
    "#,
        "work",
    );
    let function = unit.entry();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "return consumes both resources without replaying normal scope cleanup"
    );
    render_verified_program(&unit.program)
        .expect("diverging branches retain their transfer cleanup");
}
