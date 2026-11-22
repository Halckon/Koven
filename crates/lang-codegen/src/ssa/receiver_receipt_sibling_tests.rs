//! One analysis containing P2/P4 and both existing owner consumers, without native execution.
use super::*;
#[test]
fn receiver_consumer_and_owner_siblings_keep_production_receipts_separate() {
    let text = r#"class Node { fun touch(): Int = 1 }
fun receiver(own node: Node): Unit { node.touch() }
fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun loan(): Unit { val owner = "kept"; consume(view(owner)) }
fun sink(own item: String): Unit {}
fun owner_call(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; sink(held) }
fun owner_drop(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
fun entry(): Unit { receiver(Node()); loan(); owner_call(false); owner_drop(false) }"#;
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "receipt-siblings.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receipt-siblings.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let ((((lowered, calls), drops), consumer), receiver) =
            super::super::refine::receiver_receipt::observe(|| {
                super::super::refine::loan_receipt::observe(|| {
                    crate::ssa::observe_owner_drops(|| {
                        crate::ssa::observe_owner_calls(|| {
                            super::super::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, &owned, entry,
                            )
                        })
                    })
                })
            });
        let (program, _) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        let [p2] = receiver.as_slice() else {
            panic!("one P2 receipt");
        };
        let [p4] = consumer.as_slice() else {
            panic!("one P4 receipt");
        };
        assert_ne!(p2.caller, p4.caller);
        assert_ne!(p2.call, p4.call);
        assert_ne!(p2.callable, p4.callable);
        assert!(p2.ids.iter().all(|id| !p4.ids.contains(id)));
        assert!(
            p2.entities
                .iter()
                .all(|entity| !p4.entities.contains(entity))
        );
        assert_eq!(
            drops.iter().filter(|drop| drop.3.is_some()).count(),
            1,
            "only B owner Drop authorization"
        );
        assert_eq!(
            calls.iter().filter(|call| call.3.is_some()).count(),
            1,
            "only own String Call authorization"
        );
        for (protocol, ids, call) in [(2, &p2.ids, p2.call), (4, &p4.ids, p4.call)] {
            let matches = owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .filter(|r| r.protocol_version() == protocol && r.call() == call)
                .collect::<Vec<_>>();
            let [region] = matches.as_slice() else {
                panic!("one same-analysis region");
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(*ids, region.action_ids().collect::<Vec<_>>());
        }
    }
}

pub(in crate::ssa) mod map_iteration {
    //! Bounded two-component Map provider; independent from Sequential snapshots.
    pub(in crate::ssa) use crate::ssa::lower_frontend::control::map_frames::carrier;
    pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_inputs::query_cleanup::map_iteration_conditional as conditional;
    pub(in crate::ssa) use conditional::checked;
    pub(in crate::ssa) use checked::nested;
    pub(in crate::ssa) use crate::ssa::checked_integer_conversion_tests::map_iteration_exits as exits;
    pub(in crate::ssa) use exits::exit_shape;
    pub(in crate::ssa) use crate::ssa::unit_lower_control_result_tests::map_iteration_formed as formed;
    pub(in crate::ssa) use crate::ssa::mixed_borrow_boundary_tests::map_iteration_local as local;
    pub(in crate::ssa) use crate::ssa::lower_frontend::nullable_if_cleanup::local_runtime;
    pub(in crate::ssa) use crate::ssa::lower_frontend_tests::root_boundaries::map_iteration_resource as resource;
    pub(in crate::ssa) use crate::ssa::multi_borrow_range_boundary_tests::map_iteration_source as source;
    pub(in crate::ssa) use source::borrowed;
    pub(in crate::ssa) use crate::ssa::tests::source_owner;
    pub(in crate::ssa) use crate::ssa::borrowed_container_lowering_tests::map_iteration_temporary as temporary;
    pub(in crate::ssa) use temporary::{guard, qualify};

    pub(in crate::ssa) use crate::ssa::lower_frontend::*;
    pub(in crate::ssa) use crate::ssa::model::{Definition, MapContainerKind, Module};
    pub(in crate::ssa) use control::LinearBindingSlot;
    pub(in crate::ssa) use lang_frontend::ownership_checking::{
        CleanupCondition, IterationCleanupAction, IterationExitKind, IterationExitPlan,
        MapIterationOwnershipPlan, MapIterationSourceAccess,
    };

    pub(in crate::ssa) use exits::Context;
    pub(in crate::ssa) use source::SourceIdentity;
    pub(in crate::ssa) fn supported_drop(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: lang_frontend::ownership_checking::DropFact,
    ) -> bool {
        local::supported_drop(parsed, names, typed, owned, fact)
            || source_owner::supported_drop(parsed, names, typed, owned, fact)
            || conditional::supported_local_drop(parsed, names, typed, owned, fact)
            || typed.map_iterations().iter().any(|d| {
                let Some(plan) = owned.map_iteration(d.statement()) else {
                    return false;
                };
                let Ok(node) = parsed.ast().statements().get(d.statement()) else {
                    return false;
                };
                formed::retained::proof((parsed, names, typed, owned), plan, fact, node.span())
                    .is_ok_and(|proof| proof.is_some())
            })
    }

    type Storage = BTreeMap<SsaTypeId, (MapContainerKind, SsaTypeId, SsaTypeId)>;

    /// Read the actual Module only after ordinary type planning has finished.
    pub(in crate::ssa) fn storage(module: &Module, types: &BTreeMap<TypeId, SsaTypeId>) -> Storage {
        types
            .values()
            .filter_map(|&ty| module.map_container(ty).map(|layout| (ty, layout)))
            .collect()
    }

    #[derive(Default)]
    pub(in crate::ssa) struct State {
        pub(in crate::ssa) entries: BTreeMap<SymbolId, EntityId>,
        pub(in crate::ssa) resources: BTreeMap<SymbolId, resource::Carrier>,
        pub(in crate::ssa) storage: Storage,
    }
    impl State {
        pub(in crate::ssa) fn new(
            bindings: &BTreeMap<SymbolId, LoweredValue>,
            loans: &BTreeMap<SymbolId, LoanId>,
            storage: &Storage,
        ) -> Self {
            Self {
                entries: bindings
                    .iter()
                    .filter_map(|(&symbol, binding)| match binding {
                        LoweredValue::Value(owner) => Some((symbol, EntityId::Value(*owner))),
                        _ => None,
                    })
                    .chain(
                        loans
                            .iter()
                            .map(|(&symbol, &loan)| (symbol, EntityId::Loan(loan))),
                    )
                    .collect(),
                storage: storage.clone(),
                resources: BTreeMap::new(),
            }
        }
    }

    pub(in crate::ssa) fn unique<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
        let result = values.next()?;
        values.next().is_none().then_some(result)
    }

    /// One path-local provider frame; false is rebuilt from its independent snapshot.
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Frame {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) source: Option<LoanId>,
        pub(in crate::ssa) components: [(SymbolId, Option<LoanId>); 2],
        pub(in crate::ssa) finished: bool,
    }

    /// Reuse complete provider qualification for this exact stable binding's For.
    pub(in crate::ssa) fn stable_borrow_provider(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        statement: StatementId,
        binding: SymbolId,
    ) -> bool {
        typed.map_iteration(statement).is_some_and(|descriptor| {
            qualify::qualify(parsed, names, typed, owned, *descriptor).is_ok_and(|qualified|
                matches!(qualified.source, SourceIdentity::BorrowedBinding(proof) if proof.binding == binding))
        })
    }

    pub(in crate::ssa) fn roles(
        parameters: &[EntityId],
        source: usize,
        cursor: usize,
        span: Span,
    ) -> Result<(LoanId, ValueId), LoweringError> {
        match (parameters.get(source), parameters.get(cursor)) {
            (Some(EntityId::Loan(source)), Some(EntityId::Value(cursor))) => Ok((*source, *cursor)),
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }
}
