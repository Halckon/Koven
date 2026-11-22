//! Single-source Unit resource destructors are real readonly callable bodies.
//! Source-specific Resource admission and backend-boundary contracts use ordinary Unit.

use crate::test_support::diagnostics::assert_text_eq;
#[path = "lower_frontend_resource_deinit_tests/direct_closure.rs"]
pub(in crate::ssa) mod direct_closure;
#[allow(unused_imports)]
pub(in crate::ssa) use direct_closure::field_chain;

#[path = "lower_frontend_resource_deinit_tests/nested_scope_drops.rs"]
mod nested_scope_drops;

use direct_closure::field_chain::assert_nested_this_field_chain;
use lang_frontend::name_resolution::SymbolKind;

use super::{LoweringErrorKind, render_program, render_verified_program};
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
        function
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
        function
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
    assert_nested_this_field_chain(&unit, outer, "this.inner.code", &[0, 0], record, true);
    let allocations = unit
        .entry()
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
        .collect::<Vec<_>>();
    let [inner_allocation, outer_allocation] = allocations.as_slice() else {
        panic!("the original source allocates exactly Inner and Outer");
    };
    let [inner_owner @ EntityId::Value(_)] = inner_allocation.results.as_slice() else {
        panic!("Inner construction produces the actual nested owner");
    };
    let [EntityId::Value(outer_owner)] = outer_allocation.results.as_slice() else {
        panic!("Outer construction produces the sole scope-cleanup owner");
    };
    assert_eq!(
        unit.source.sources().slice(inner_allocation.origin.span()),
        Ok("Inner(7)")
    );
    assert_eq!(
        unit.module().deinit(
            unit.entry()
                .entity(*inner_owner)
                .unwrap()
                .ty
                .semantic_type()
        ),
        Some(inner.id())
    );
    assert_eq!(
        unit.module().deinit(
            unit.entry()
                .entity(EntityId::Value(*outer_owner))
                .unwrap()
                .ty
                .semantic_type()
        ),
        Some(outer.id())
    );
    let Operation::HeapAllocate { payload, .. } = outer_allocation.operation else {
        unreachable!()
    };
    let payloads = unit
        .entry()
        .instructions
        .iter()
        .filter(|instruction| instruction.results == [EntityId::Value(payload)])
        .collect::<Vec<_>>();
    let [payload] = payloads.as_slice() else {
        panic!("Outer has one real payload definition")
    };
    assert!(
        matches!(&payload.operation, Operation::AggregateConstruct { fields, .. }
        if fields.len() == 2 && EntityId::Value(fields[0]) == *inner_owner)
    );
    assert_eq!(
        unit.entry()
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::Drop { owner } => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [*outer_owner]
    );
    assert_text_eq!(
        render_program(&unit.program),
        render_program(&lower_resource(source, "run").program)
    );
    render_verified_program(&unit.program)
        .expect("nested resource fields and generic body calls verify");
}

#[test]
fn single_resource_deinit_grouped_nested_copy_and_borrow_keep_distinct_extents() {
    let source = r#"fun take(own code: Int): Unit {}
        fun observe(code: Int): Unit {}
        fun inspect(owner: Outer): Unit {}
        class Leaf(val code: Int)
        class Middle(val leaf: Leaf)
        class Outer(val middle: Middle) {
            deinit() {
                take(((this.middle).leaf).code)
                observe(((middle).leaf).code)
                inspect(this)
            }
        }
        fun run(): Unit { val outer = Outer(Middle(Leaf(9))) }"#;
    let unit = lower_resource(source, "run");
    let body = unit.deinit("Outer");
    assert_nested_this_field_chain(
        &unit,
        body,
        "((this.middle).leaf).code",
        &[0, 0, 0],
        unit.function("take"),
        true,
    );
    assert_nested_this_field_chain(
        &unit,
        body,
        "((middle).leaf).code",
        &[0, 0, 0],
        unit.function("observe"),
        false,
    );
    let receiver = body.block(body.entry_block().unwrap()).unwrap().parameters[0];
    let inspect = unit.function("inspect");
    let last_call = body
        .instructions
        .iter()
        .rev()
        .find(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .unwrap();
    assert!(matches!(&last_call.operation,
        Operation::DirectCall { callee, receiver: None, arguments }
            if *callee == inspect.id() && arguments.as_slice() == [receiver]));
    assert_text_eq!(
        render_program(&unit.program),
        render_program(&lower_resource(source, "run").program)
    );
    render_verified_program(&unit.program)
        .expect("grouped readonly paths preserve copy and call loan extents");
}

#[test]
fn single_resource_deinit_nested_copy_keeps_generic_receivers_unsupported() {
    for (source, fragment) in [
        (
            "fun take(own code: Int): Unit {}\nclass Inner(val code: Int)\nclass Outer<T>(val inner: Inner, val tag: T) { deinit() { take(this.inner.code) } }\nfun run(): Unit { val outer = Outer(Inner(1), 2) }",
            "Outer(Inner(1), 2)",
        ),
        (
            "fun take(own code: Int): Unit {}\nclass Inner<T>(val code: T)\nclass Outer(val inner: Inner<Int>) { deinit() { take(this.inner.code) } }\nfun run(): Unit { val outer = Outer(Inner(1)) }",
            "this.inner.code",
        ),
    ] {
        let unit = UnitSource::analyze("unsupported.ko", source)
            .expect("generic receiver source is frontend-clean");
        unit.assert_frontend_clean();
        let error = crate::ssa::with_per_family_lowering(|| {
            unit.lower_entry(unit.declaration("run", SymbolKind::Function))
        })
        .err()
        .expect("the readonly copy path must not open generic class receivers");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        let span = error
            .span
            .expect("unsupported generic receiver retains its source origin");
        assert_eq!(unit.sources().source_text(span.source_id()), Ok(source));
        assert_eq!(unit.sources().slice(span), Ok(fragment));
        let start = source.find(fragment).unwrap();
        assert_eq!((span.start(), span.end()), (start, start + fragment.len()));
    }
}

#[test]
fn single_resource_deinit_nested_copy_rejects_move_only_and_exclusive_access() {
    for (body, code, fragment) in [
        ("consume(this.inner.payload)", "L0132", "payload"),
        ("update(&this.inner.code)", "L0134", "&"),
    ] {
        let source = format!(
            "class Payload {{}}\nclass Inner(val payload: Payload, var code: Int)\nfun consume(own value: Payload): Unit {{}}\nfun update(inout value: Int): Unit {{}}\nclass Outer(val inner: Inner) {{ deinit() {{ {body} }} }}\nfun run(): Unit {{ val outer = Outer(Inner(Payload(), 1)) }}"
        );
        let Err(rejection) = UnitSource::analyze("readonly-boundary.ko", &source) else {
            panic!("readonly this must not authorize a MoveOnly read or exclusive child");
        };
        assert_eq!(
            rejection.report.stages,
            [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
        );
        let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
            panic!("the actual Unit ownership boundary must reject exactly this access");
        };
        assert_eq!(*stage, "ownership");
        assert_eq!(diagnostic.code().to_string(), code);
        // Member consumption reports its final name_span; Inout reports the '&' marker.
        // Anchor both to the actual body access, never the same-named field declaration.
        let span = diagnostic.primary_span();
        assert_eq!(
            rejection.sources.source_text(span.source_id()),
            Ok(source.as_str())
        );
        assert_eq!(rejection.sources.slice(span), Ok(fragment));
        assert_eq!(source.matches(body).count(), 1);
        let start = source.find(body).unwrap() + body.find(fragment).unwrap();
        assert_eq!((span.start(), span.end()), (start, start + fragment.len()));
    }
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
fn original_borrowed_nullable_resource_keeps_readonly_formal_without_cleanup() {
    use crate::ssa::model::TerminatorKind;
    use lang_frontend::{
        name_resolution::SourceUnitKey,
        type_checking::{ParameterMode, UnitTypeKind},
    };

    // Single rejected the representation. Ordinary Unit accepts an unused
    // borrowed nullable formal without owning or destroying its argument.
    let text = "class Resource { deinit() {} }\nfun inspect(value: Resource?): Unit {}";
    let unit = lower_resource(text, "inspect");
    let [document] = unit.source.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.source.parsed().source_id());
    assert_eq!(
        unit.source.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(
        unit.source.sources().source_text(document.source_id()),
        Ok(text)
    );
    let resource = unit.source.declaration("Resource", SymbolKind::Classifier);
    let inspect = unit.source.declaration("inspect", SymbolKind::Function);
    let signatures = unit.source.types().signatures();
    let nominal = signatures.declaration(resource).unwrap().nominal().unwrap();
    let destructor = nominal
        .deinit()
        .expect("actual original deinit declaration");
    assert_eq!(destructor.owner(), resource);
    assert_eq!(destructor.receiver_mode(), ParameterMode::Borrow);
    let callable = signatures.declaration(inspect).unwrap().callable().unwrap();
    let [parameter] = callable.parameters() else {
        panic!("original formal");
    };
    assert_eq!(parameter.mode(), ParameterMode::Borrow);
    let Some(UnitTypeKind::Nullable(inner)) = unit.source.types().types().get(parameter.ty())
    else {
        panic!("original nullable formal");
    };
    assert!(matches!(unit.source.types().types().get(*inner),
        Some(UnitTypeKind::Nominal { declaration, arguments })
            if *declaration == resource && arguments.is_empty()));
    assert!(unit.source.ownership().drops().is_empty());

    crate::ssa::verify::verify_program(&unit.program).unwrap();
    render_verified_program(&unit.program).unwrap();
    let entry = unit.entry();
    let [block] = entry.blocks.as_slice() else {
        panic!("one original empty body");
    };
    let [formal @ EntityId::Loan(_)] = block.parameters.as_slice() else {
        panic!("actual borrowed SSA formal");
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entry.entity(*formal).unwrap().ty
    else {
        panic!("nullable formal never becomes owned");
    };
    let module = unit.module();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(target) else {
        panic!("actual nullable heap handle ABI");
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    let actual_deinit = unit.deinit("Resource");
    assert_eq!(module.deinit(*inner), Some(actual_deinit.id()));
    assert_eq!(module.deinits.len(), 1);
    assert!(entry.instructions.is_empty());
    assert!(matches!(
        &block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty()
    ));
    // This inspects the real static registration; it does not execute a destructor.
}

#[test]
fn single_resource_deinit_rejects_unsupported_resource_wrappers() {
    let generic = "class Resource<T>(val value: T) { deinit() {} }\nfun run(): Unit { val resource = Resource(1) }";
    let unit = UnitSource::analyze("lowering.ko", generic)
        .expect("generic deinit is a real declaration in the validated frontend");
    unit.assert_frontend_clean();
    let source = unit.parsed().source_id();
    assert_eq!(unit.sources().source_text(source), Ok(generic));
    let declaration = unit.declaration("Resource", SymbolKind::Classifier);
    let resource = unit
        .types()
        .signatures()
        .declaration(declaration)
        .unwrap()
        .nominal()
        .unwrap();
    assert_eq!(resource.type_parameters().len(), 1);
    assert!(resource.deinit().is_some());
    let run = unit.declaration("run", SymbolKind::Function);
    let Err(failure) = unit.lower_entry(run) else {
        panic!("generic deinit still cannot produce an executable Unit program");
    };
    assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
    let origin = failure
        .span
        .expect("reported resource occurrence remains source qualified");
    assert_eq!(origin.source_id(), source);
    assert!(!unit.sources().slice(origin).unwrap().is_empty());

    // This original generic wrapper has the same complete rejection on both routes.
    let source = "class Resource { deinit() {} }\nclass Holder<T>(val value: T)\nfun run(): Unit { val wrapped = Holder(Resource()) }";
    let unit = UnitSource::analyze("lowering.ko", source)
        .expect("the original resource wrapper passes strict Unit frontend admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    assert_eq!(unit.parsed().roots().len(), 3);
    let source_id = unit.parsed().source_id();
    assert_eq!(unit.sources().source_name(source_id), Ok("lowering.ko"));
    assert_eq!(unit.sources().source_text(source_id), Ok(source));
    let run = unit.declaration("run", SymbolKind::Function);
    assert_eq!(
        unit.sources().slice(unit.declaration_span(run)),
        Ok("fun run(): Unit { val wrapped = Holder(Resource()) }")
    );
    let failure = unit
        .lower_entry(run)
        .err()
        .expect("the unsupported generic resource wrapper cannot produce a Program");
    let span = unit.sources().span(source_id, 93, 111).unwrap();
    assert_eq!(unit.sources().slice(span), Ok("Holder(Resource())"));
    assert_eq!(
        failure,
        crate::ssa::LoweringError {
            kind: LoweringErrorKind::UnsupportedNode,
            span: Some(span)
        }
    );
}

#[test]
fn single_resource_deinit_inside_owned_closure_uses_body_and_environment_cleanup() {
    direct_closure::assert_cleanup(
        "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }",
        1,
    );
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
    // Keep the historical FQN; only the supported Unit body contract remains.
    for (source, classifier, call) in [
        (
            "class Resource { fun observe(): Unit {}\n deinit() { this.observe() } }\nfun run(): Unit { val resource = Resource() }",
            "Resource",
            "this.observe()",
        ),
        (
            "fun inspect(text: String): Unit {}\nclass Inner(val text: String)\nclass Outer(val inner: Inner) { deinit() { inspect(inner.text) } }\nfun run(): Unit { val outer = Outer(Inner(\"nested\")) }",
            "Outer",
            "inspect(inner.text)",
        ),
    ] {
        let unit = lower_resource(source, "run");
        let unit_source = unit.source.parsed().source_id();
        assert_eq!(
            unit.source.sources().source_name(unit_source),
            Ok("lowering.ko")
        );
        assert_eq!(unit.source.sources().source_text(unit_source), Ok(source));
        assert!(unit.source.ownership().deferred().is_empty());
        let destructor = unit.deinit(classifier);
        let calls = destructor
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { .. })
                    && matches!(instruction.origin, Origin::Source(span)
                    if unit.source.sources().slice(span) == Ok(call))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls.len(),
            1,
            "original destructor call must actually lower"
        );
        render_verified_program(&unit.program).expect("same original source Unit LLVM");
    }
}

#[test]
fn unit_resource_deinit_defers_original_partial_move_at_ownership_boundary() {
    use crate::native_tests::boxed_enum_tests::unit_source::SourceRejectionKind;
    use lang_frontend::{
        name_resolution::{SourceUnitInput, SourceUnitKey},
        ownership_checking::OwnershipDeferredReason,
        parser::Expression,
    };

    let text = r#"
        class Resource { deinit() { println("released") } }
        fun consume(own value: Resource): Unit {}
        fun run(flag: Boolean): Unit {
            val resource = Resource()
            if (flag) { consume(resource) }
            println("after branch")
        }
    "#;
    // Unit cannot publish executable owner transport for the original branch. Its
    // earlier recovery boundary supersedes the Legacy lowerer error, not the old
    // conditional-drop representation.
    let Err(rejection) = UnitSource::analyze("lowering.ko", text) else {
        panic!("conditional resource transport must not publish an executable UnitSource");
    };
    assert_eq!(
        rejection.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 0)]
    );
    assert!(rejection.report.diagnostics.is_empty());
    let SourceRejectionKind::IncompleteBasicOwnership(recovery) = rejection.kind else {
        panic!("the real ownership product must carry a recovery obligation");
    };
    let [unit] = recovery.names.names().index().source_units() else {
        panic!("the original memory document");
    };
    let id = recovery.parsed.source_id();
    assert_eq!(unit.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(unit.source_id(), id);
    assert_eq!(rejection.sources.source_text(id), Ok(text));
    let inputs = [SourceUnitInput::memory_document(id, &recovery.parsed)];
    assert!(recovery.typed.types().is_compatible_with(
        &rejection.sources,
        &inputs,
        &recovery.names,
        &recovery.environment
    ));
    assert!(recovery.owned.is_compatible_with(&recovery.typed));
    assert!(recovery.owned.diagnostics().is_empty());
    let [deferred] = recovery.owned.deferred() else {
        panic!("one real resource transport obligation")
    };
    assert_eq!(deferred.reason(), OwnershipDeferredReason::ResourceLifetime);
    assert_eq!(deferred.expression().source_unit(), unit.id());
    let node = recovery
        .parsed
        .ast()
        .expressions()
        .get(deferred.expression().expression())
        .unwrap();
    assert!(matches!(node.payload(), Expression::If { .. }));
    let expected = "if (flag) { consume(resource) }";
    let start = text.find(expected).unwrap();
    let origin = node.span();
    assert_eq!(origin.source_id(), id);
    assert_eq!(
        (origin.start(), origin.end()),
        (start, start + expected.len())
    );
    assert_eq!(rejection.sources.slice(origin), Ok(expected));
    let expected_deferred = recovery.owned.deferred().to_vec();
    let still_rejected = (*recovery.owned)
        .validate()
        .expect_err("no executable ownership authority");
    assert_eq!(still_rejected.deferred(), expected_deferred);
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
        // Unit may also carry the original Copyable stop binding on the legacy path;
        // refine liveness carries only the live resource owner.
        assert!(
            edge.arguments.len() == 1 || edge.arguments.len() == 2,
            "at most stop and the current resource only"
        );
        assert!(
            edge.arguments
                .iter()
                .filter(|argument| *argument == stop)
                .count()
                <= 1
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
    let unit = lower_resource(source, "work");
    nested_scope_drops::assert_original_facts(&unit);
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
