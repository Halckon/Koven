//! Original direct resource closure through strict Unit, alongside the Legacy witness.

use super::{
    EntityId, EntityType, Operation, SsaTypeKind, lower_resource, render_verified_program,
};

#[test]
fn original_owned_resource_closure_keeps_body_and_environment_cleanup() {
    let source = "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }";
    assert_cleanup(source, 1);
}

#[test]
fn grouped_alias_and_repeated_calls_share_the_original_resource_thunk() {
    assert_cleanup(
        "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; val alias = callback; (alias)(); (alias)() }",
        2,
    );
}

fn assert_cleanup(source: &str, invocations: usize) {
    let unit = lower_resource(source, "run");
    assert!(unit.source.ownership().deferred().is_empty());
    assert_eq!(
        unit.source
            .sources()
            .source_text(unit.source.parsed().source_id()),
        Ok(source)
    );
    let module = unit.module();
    let entry = unit.entry();
    let constructions = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::ClosureConstruct { closure, thunk, .. } => Some((closure, thunk)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(closure_type, thunk)] = constructions.as_slice() else {
        panic!("the original callback forms exactly one concrete owned closure");
    };
    let thunk = module
        .function(*thunk)
        .expect("the constructed closure selects its actual body thunk");
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
    assert_eq!(module.deinit(*resource), Some(unit.deinit("Resource").id()));
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::CallableInvoke { .. }))
            .count(),
        invocations
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
    assert_eq!(closure, closure_type);
    let Some(SsaTypeKind::ConcreteClosure { captures, .. }) = module.type_kind(*closure) else {
        panic!("the entry drops its concrete closure environment");
    };
    let [capture] = captures.as_slice() else {
        panic!("the closure retains exactly the original text capture");
    };
    assert_eq!(capture.mode, crate::ssa::model::ClosureCaptureMode::Owned);
    assert_eq!(
        module.type_kind(capture.ty),
        Some(&SsaTypeKind::StringOwner)
    );
    render_verified_program(&unit.program)
        .expect("body Resource and owned environment cleanup verify through LLVM");
}

#[test]
fn resource_lambdas_without_a_dispatch_keep_their_exact_existing_boundary() {
    use super::{LoweringErrorKind, UnitSource};
    use lang_frontend::name_resolution::SymbolKind;

    for source in [
        "class Resource { deinit() {} }\nfun run(): Unit { val callback = move { val resource = Resource() } }",
        "class Resource { deinit() {} }\nfun run(): Unit { val callback: move (Int) -> Unit = move { value -> val resource = Resource() }; callback(return) }",
    ] {
        let unit =
            UnitSource::analyze("lowering.ko", source).expect("original strict Unit frontend");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let failure = unit
            .lower_entry(unit.declaration("run", SymbolKind::Function))
            .err()
            .expect("unselected resource lambdas remain outside this bounded slice");
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
        let span = failure.span.expect("original resource construction span");
        let start = source.find("Resource()").unwrap();
        assert_eq!(span.source_id(), unit.parsed().source_id());
        assert_eq!(
            (span.start(), span.end()),
            (start, start + "Resource()".len())
        );
        assert_eq!(unit.sources().slice(span), Ok("Resource()"));
    }
}

#[test]
fn invoked_resource_closure_composes_with_borrow_initializer_prefix_exit() {
    for (argument, has_borrow_call) in
        [("return", false), ("if (flag) { return } else { 1 }", true)]
    {
        let source = format!(
            r#"class Resource {{ deinit() {{}} }}
fun view(source: String, own n: Int): borrow String from source = source
fun run(own flag: Boolean): Unit {{
    val text = "capture"
    val callback = move {{ println(text); val resource = Resource() }}
    callback()
    val owner = "kept".clone()
    borrow val item = view(owner, {argument})
    println(item)
}}"#
        );
        let unit = lower_resource(&source, "run");
        assert!(unit.source.ownership().deferred().is_empty());
        let entry = unit.entry();
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::CallableInvoke { .. }
                ))
                .count(),
            1
        );
        let thunk = entry
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::ClosureConstruct { thunk, .. } => unit.module().function(thunk),
                _ => None,
            })
            .expect("the dispatched resource closure keeps its actual thunk");
        let resources = thunk
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
        let [EntityType::Value(resource)] = resources.as_slice() else {
            panic!("the invoked closure must retain one Resource drop");
        };
        assert_eq!(
            unit.module().deinit(*resource),
            Some(unit.deinit("Resource").id())
        );
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
                .count(),
            usize::from(has_borrow_call)
        );
        if has_borrow_call {
            assert_borrow_prefix_exit(entry);
        } else {
            assert!(
                unit.module().functions.iter().all(|function| {
                    function.origin
                        != crate::ssa::model::Origin::Source(unit.source.declaration_span(
                            unit.source.declaration(
                                "view",
                                lang_frontend::name_resolution::SymbolKind::Function,
                            ),
                        ))
                }),
                "an abandoned initializer must not instantiate its callee"
            );
        }
        render_verified_program(&unit.program).expect("resource closure and borrow prefix compose");
    }
}

#[test]
fn dispatched_resource_lambda_does_not_admit_its_unselected_sibling() {
    use super::{LoweringErrorKind, UnitSource};
    use lang_frontend::name_resolution::SymbolKind;

    for call in ["", "unselected(return)"] {
        let source = format!(
            r#"class Resource {{ deinit() {{}} }}
fun run(): Unit {{
    val selected = move {{ val first = Resource() }}
    selected()
    val unselected: move (Int) -> Unit = move {{ value -> val second = Resource() }}
    {call}
}}"#
        );
        let unit = UnitSource::analyze("sibling.ko", &source).expect("strict Unit frontend");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let error = unit
            .lower_entry(unit.declaration("run", SymbolKind::Function))
            .err()
            .expect("only the invoked lambda may contribute resource plans");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error.span.expect("unselected Resource construction");
        let start = source.rfind("Resource()").unwrap();
        assert_eq!(span.source_id(), unit.parsed().source_id());
        assert_eq!(
            (span.start(), span.end()),
            (start, start + "Resource()".len())
        );
        assert_eq!(unit.sources().slice(span), Ok("Resource()"));
    }
}

fn assert_borrow_prefix_exit(entry: &crate::ssa::model::Function) {
    use crate::ssa::model::TerminatorKind;

    let (return_edge, normal_edge) = entry
        .blocks
        .iter()
        .find_map(|block| {
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref()?.kind
            else {
                return None;
            };
            Some((when_true, when_false))
        })
        .expect("borrow initializer argument conditional");
    assert_eq!(return_edge.arguments, normal_edge.arguments);
    let returning = &entry.blocks[return_edge.target.index()];
    let normal = &entry.blocks[normal_edge.target.index()];
    let [EntityId::Value(owner), EntityId::Loan(returning_loan)] = returning.parameters.as_slice()
    else {
        panic!("return arm receives the owner and its source loan")
    };
    let [EntityId::Value(_), EntityId::Loan(normal_loan)] = normal.parameters.as_slice() else {
        panic!("normal arm receives the owner and its source loan")
    };
    assert!(matches!(
        returning.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { .. }
    ));
    let dispatches = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .collect::<Vec<_>>();
    let [dispatch] = dispatches.as_slice() else {
        panic!("only the normal arm dispatches the borrow-result call")
    };
    assert_eq!(dispatch.block, normal.id);
    assert!(
        matches!(dispatch.operation, Operation::BorrowCall { source, .. } if source == *normal_loan)
    );
    let exit = returning
        .instructions
        .iter()
        .map(|id| &entry.instructions[id.index()])
        .collect::<Vec<_>>();
    let loan_end = exit
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BorrowEnd {
                    loan: *returning_loan,
                }
        })
        .expect("return arm ends its exact source loan");
    let owner_drop = exit
        .iter()
        .position(|instruction| instruction.operation == Operation::Drop { owner: *owner })
        .expect("return arm drops its exact owner");
    assert!(loan_end < owner_drop);
}
