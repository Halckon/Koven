//! Direct heap-field exchange preserves the parent owner and proves exact field provenance.

use super::*;

pub(super) struct Request {
    pub(super) owner: ValueId,
    pub(super) field: usize,
    pub(super) loan: LoanId,
    pub(super) replacement: ValueId,
}

pub(super) fn apply(
    module: &Module,
    function: &Function,
    request: Request,
    aliases: &AliasRoots,
    state: &mut BlockState,
    instruction: &super::super::model::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let Request {
        owner,
        field,
        loan,
        replacement,
    } = request;
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    let before = errors.len();
    require_value(
        module,
        function,
        owner,
        state,
        location.clone(),
        origin,
        errors,
    );
    require_value(
        module,
        function,
        replacement,
        state,
        location.clone(),
        origin,
        errors,
    );
    if !state.loans.contains(&loan) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan },
            location.clone(),
            origin,
        ));
    }
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let loan_roots = &aliases.roots[&EntityId::Loan(loan)];
    if owner_roots.is_empty()
        || owner_roots != loan_roots
        || !exact_path(
            function,
            owner,
            EntityId::Loan(loan),
            field,
            Stage::Loan,
            &mut BTreeSet::new(),
        )
    {
        errors.push(error(VerifyErrorKind::OperationContract {
            reason: "field exchange requires an exclusive loan of the exact direct owned heap field",
        }, location.clone(), origin));
    }
    if state.loans.iter().any(|active| {
        *active != loan
            && aliases.overlap(EntityId::Value(owner), EntityId::Loan(*active))
            && !aliases
                .direct_fields
                .disjoint(EntityId::Loan(loan), EntityId::Loan(*active))
    }) {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: owner },
            location.clone(),
            origin,
        ));
    }
    if aliases.overlap(EntityId::Value(owner), EntityId::Value(replacement))
        || has_any_value_loan(replacement, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: replacement },
            location.clone(),
            origin,
        ));
    }
    if errors.len() != before {
        return;
    }

    // The parent remains a single complete owner. Only replacement and loan are consumed;
    // result registration gives the old field a fresh, independent owner identity.
    state.loans.remove(&loan);
    consume_value(
        module,
        function,
        replacement,
        aliases,
        state,
        &BTreeSet::new(),
        &BTreeSet::new(),
        location,
        origin,
        errors,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Loan,
    Field,
    Payload,
    Owner,
}

type Visit = (ValueId, EntityId, Stage);

fn exact_path(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    field: usize,
    stage: Stage,
    visiting: &mut BTreeSet<Visit>,
) -> bool {
    if stage == Stage::Owner && access == EntityId::Value(owner) {
        return true;
    }
    if !visiting.insert((owner, access, stage)) {
        // A loop may revisit the same proof obligation. The caller additionally requires
        // nonempty matching origins, and every incoming edge is still checked.
        return true;
    }
    let valid = match function.entity(access).expect("verified entity").definition {
        Definition::InstructionResult {
            instruction,
            index: 0,
        } => {
            match (
                &function
                    .instruction(instruction)
                    .expect("verified instruction")
                    .operation,
                stage,
                access,
            ) {
                (
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Exclusive,
                    },
                    Stage::Loan,
                    EntityId::Loan(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Place(*place),
                    field,
                    Stage::Field,
                    visiting,
                ),
                (
                    Operation::FieldPlace {
                        base,
                        field: actual,
                    },
                    Stage::Field,
                    EntityId::Place(_),
                ) if *actual == field => exact_path(
                    function,
                    owner,
                    EntityId::Place(*base),
                    field,
                    Stage::Payload,
                    visiting,
                ),
                (
                    Operation::HeapPayloadPlace { owner: source },
                    Stage::Payload,
                    EntityId::Place(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Value(*source),
                    field,
                    Stage::Owner,
                    visiting,
                ),
                (_, Stage::Owner, EntityId::Value(_)) => {
                    expand_owner(function, owner, access, field, stage, visiting)
                }
                _ => false,
            }
        }
        Definition::BlockParameter { block, index } => {
            let incoming = incoming_edges(function, block);
            if incoming.is_empty() {
                stage == Stage::Owner
                    && matches!(access, EntityId::Value(_))
                    && expand_owner(function, owner, access, field, stage, visiting)
            } else {
                incoming.iter().all(|edge| {
                    exact_path(
                        function,
                        rebound_owner(function, owner, block, edge),
                        edge.arguments[index],
                        field,
                        stage,
                        visiting,
                    )
                })
            }
        }
        Definition::InstructionResult { .. } => {
            stage == Stage::Owner
                && matches!(access, EntityId::Value(_))
                && expand_owner(function, owner, access, field, stage, visiting)
        }
    };
    visiting.remove(&(owner, access, stage));
    valid
}

fn expand_owner(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    field: usize,
    stage: Stage,
    visiting: &mut BTreeSet<Visit>,
) -> bool {
    let Definition::BlockParameter { block, index } = function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    else {
        return false;
    };
    let incoming = incoming_edges(function, block);
    !incoming.is_empty()
        && incoming.iter().all(|edge| {
            let EntityId::Value(source) = edge.arguments[index] else {
                return false;
            };
            let access = match function.entity(access).expect("verified access").definition {
                Definition::BlockParameter {
                    block: access_block,
                    index,
                } if access_block == block => edge.arguments[index],
                _ => access,
            };
            exact_path(function, source, access, field, stage, visiting)
        })
}

fn rebound_owner(function: &Function, owner: ValueId, block: BlockId, edge: &Edge) -> ValueId {
    match function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    {
        Definition::BlockParameter {
            block: owner_block,
            index,
        } if owner_block == block => {
            let EntityId::Value(owner) = edge.arguments[index] else {
                unreachable!("verified edge value type")
            };
            owner
        }
        _ => owner,
    }
}

fn incoming_edges(function: &Function, target: BlockId) -> Vec<&Edge> {
    function
        .blocks
        .iter()
        .flat_map(|block| edges(&block.terminator.as_ref().expect("verified terminator").kind))
        .filter(|edge| edge.target == target)
        .collect()
}

pub(in crate::ssa) mod closure_escape {
    //! Reject unproved borrowed environments only at owned escape delivery boundaries.

    pub(in crate::ssa) mod proof {
        use std::collections::BTreeMap;

        use super::TypeCaptures;
        use crate::ssa::verify::content_flow;
        use crate::ssa::{
            model::{
                ClosureCaptureOperand, EntityId, Function, Instruction, Operation, PlaceAccess,
                PlaceId, TerminatorKind, ValueId,
            },
            verify_ownership::{AliasRoots, all_entities, edges, root_exchange},
        };

        pub(super) type State = Vec<bool>;

        pub(super) struct ValueProof<'a> {
            function: &'a Function,
            defaults: State,
            projected: Vec<bool>,
            affected: BTreeMap<EntityId, Vec<(usize, bool)>>,
        }

        impl<'a> ValueProof<'a> {
            pub(super) fn new(function: &'a Function, types: &TypeCaptures) -> Self {
                // Keep Values first so delivery checks can still index directly by ValueId.
                let entities = all_entities(function);
                let defaults = entities
                    .iter()
                    .map(|entity| {
                        types.contains(
                            function
                                .entity(*entity)
                                .expect("verified entity")
                                .ty
                                .semantic_type(),
                        )
                    })
                    .collect::<Vec<_>>();
                let aliases = AliasRoots::compute(function);
                let projected = projected_places(function, &aliases);
                let mut affected = BTreeMap::new();
                for instruction in &function.instructions {
                    let access = match instruction.operation {
                        Operation::Mutate { place, .. } => EntityId::Place(place),
                        Operation::HeapFieldReplace { receiver, .. }
                        | Operation::InlineFieldReplace { receiver, .. }
                        | Operation::LoanMutate { loan: receiver, .. } => EntityId::Loan(receiver),
                        Operation::HeapFieldExchange { owner, .. }
                        | Operation::ContainerReplace { owner, .. }
                        | Operation::ContainerAppend { owner, .. }
                        | Operation::ContainerClear { owner, .. }
                        | Operation::ContainerRemoveAt { owner, .. }
                        | Operation::ContainerRemoveFirst { owner, .. }
                        | Operation::ContainerRemoveLast { owner, .. }
                        | Operation::ContainerInsertAt { owner, .. } => EntityId::Value(owner),
                        _ => continue,
                    };
                    affected.entry(access).or_insert_with(|| {
                        let exact_owners = match access {
                            EntityId::Place(place) if !projected[place.index()] => entities
                                .iter()
                                .filter_map(|entity| match entity {
                                    EntityId::Value(owner)
                                        if aliases.overlap(access, *entity)
                                            && root_exchange::exact_place(
                                                function, *owner, place, &aliases,
                                            ) =>
                                    {
                                        Some(*owner)
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>(),
                            _ => Vec::new(),
                        };
                        entities
                            .iter()
                            .enumerate()
                            .filter_map(|(index, entity)| {
                                if !defaults[index] || !aliases.overlap(access, *entity) {
                                    return None;
                                }
                                let exact = match entity {
                                    EntityId::Value(owner) => exact_owners.contains(owner),
                                    EntityId::Place(place) => exact_owners.iter().any(|owner| {
                                        root_exchange::exact_inline_place(
                                            function, *owner, *place, &aliases,
                                        )
                                    }),
                                    _ => false,
                                };
                                Some((index, exact))
                            })
                            .collect()
                    });
                }
                Self {
                    function,
                    defaults,
                    projected,
                    affected,
                }
            }

            pub(super) fn is_projected(&self, place: PlaceId) -> bool {
                self.projected[place.index()]
            }

            /// OR joins preserve proofs only when every incoming execution is known clean.
            pub(super) fn block_entries(&self) -> Vec<State> {
                let function = self.function;
                content_flow::block_entries(
                    function,
                    &self.defaults,
                    |block, state| {
                        for id in &block.instructions {
                            self.apply(
                                function.instruction(*id).expect("verified instruction"),
                                state,
                            );
                        }
                        self.apply_terminator(
                            &block.terminator.as_ref().expect("verified terminator").kind,
                            state,
                        );
                    },
                    |edge, state| {
                        let target = function.block(edge.target).expect("verified target");
                        let rebound = edge
                            .arguments
                            .iter()
                            .zip(&target.parameters)
                            .map(|(argument, parameter)| {
                                (
                                    content_index(function, *parameter),
                                    state[content_index(function, *argument)],
                                )
                            })
                            .collect::<Vec<_>>();
                        let mut incoming = state.clone();
                        for (parameter, may_capture) in rebound {
                            incoming[parameter] = may_capture;
                        }
                        incoming
                    },
                    |previous, incoming| {
                        let mut changed = false;
                        for (previous, incoming) in previous.iter_mut().zip(incoming) {
                            if incoming && !*previous {
                                *previous = true;
                                changed = true;
                            }
                        }
                        changed
                    },
                )
            }

            pub(super) fn apply(&self, instruction: &Instruction, state: &mut State) {
                let may = |value: ValueId| state[value.index()];
                let results = instruction
                    .results
                    .iter()
                    .filter_map(|entity| match entity {
                        EntityId::Value(value) => Some(*value),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let proof = match &instruction.operation {
                    Operation::FunctionAddress { .. } | Operation::NullableNull { .. } => {
                        Some(false)
                    }
                    Operation::ClosureConstruct { captures, .. } => {
                        Some(captures.iter().any(|capture| match capture {
                            ClosureCaptureOperand::Shared(_) => true,
                            ClosureCaptureOperand::Owned(value) => may(*value),
                        }))
                    }
                    Operation::AggregateConstruct { fields, .. } => {
                        Some(fields.iter().copied().any(may))
                    }
                    Operation::ContainerConstruct { elements, .. } => {
                        Some(elements.iter().copied().any(may))
                    }
                    Operation::TaggedConstruct { payload, .. }
                    | Operation::HeapAllocate { payload, .. }
                    | Operation::SharedAllocate { payload, .. } => Some(may(*payload)),
                    Operation::NullableWrap { owner, .. }
                    | Operation::NullableTake { owner, .. }
                    | Operation::RootPlaceTake { owner, .. } => Some(may(*owner)),
                    Operation::Copy { source } => Some(may(*source)),
                    Operation::SharedRetain { owner }
                    | Operation::SharedPayloadPlace { owner }
                    | Operation::ContainerElementPlace { owner, .. } => {
                        Some(state[content_index(self.function, *owner)])
                    }
                    Operation::RootPlace { owner }
                    | Operation::HeapPayloadPlace { owner }
                    | Operation::TaggedPayloadPlace { owner, .. } => Some(may(*owner)),
                    Operation::BorrowBegin { place, .. }
                    | Operation::FieldPlace { base: place, .. } => {
                        Some(state[content_index(self.function, EntityId::Place(*place))])
                    }
                    Operation::SharedReborrow { source }
                    | Operation::CallableViewOf { source }
                    | Operation::RangeElementPlace { view: source, .. }
                    | Operation::SharedFieldLoan { base: source, .. }
                    | Operation::SharedHeapFieldLoan { base: source, .. } => {
                        Some(state[content_index(self.function, EntityId::Loan(*source))])
                    }
                    Operation::SharedReferenceFollow { .. } => None,
                    Operation::Read { source } => Some(
                        state[content_index(
                            self.function,
                            match source {
                                PlaceAccess::Place(place) => EntityId::Place(*place),
                                PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
                            },
                        )],
                    ),
                    Operation::AggregateProject { aggregate, .. }
                    | Operation::AggregateExplode { aggregate }
                    | Operation::AggregateCopyExplode { aggregate }
                        if !may(*aggregate) =>
                    {
                        Some(false)
                    }
                    Operation::RootReplace {
                        owner, replacement, ..
                    } => {
                        let old = may(*owner);
                        let replacement = may(*replacement);
                        state[results[0].index()] = replacement;
                        state[results[1].index()] = old;
                        return;
                    }
                    Operation::RootSwap { owners, .. } => {
                        let left = may(owners[0]);
                        let right = may(owners[1]);
                        state[results[0].index()] = right;
                        state[results[1].index()] = left;
                        return;
                    }
                    Operation::HeapFieldExchange { owner, .. } if !may(*owner) => Some(false),
                    _ => None,
                };
                let write = match instruction.operation {
                    Operation::Mutate { place, value } => Some((
                        EntityId::Place(place),
                        may(value),
                        !self.is_projected(place),
                    )),
                    Operation::HeapFieldReplace {
                        receiver, value, ..
                    }
                    | Operation::InlineFieldReplace {
                        receiver, value, ..
                    }
                    | Operation::LoanMutate {
                        loan: receiver,
                        value,
                    } => Some((EntityId::Loan(receiver), may(value), false)),
                    Operation::HeapFieldExchange {
                        owner, replacement, ..
                    } => Some((EntityId::Value(owner), may(replacement), false)),
                    Operation::ContainerReplace { owner, value, .. } => {
                        Some((EntityId::Value(owner), may(value), false))
                    }
                    Operation::ContainerAppend { owner, element }
                    | Operation::ContainerInsertAt { owner, element, .. } => {
                        Some((EntityId::Value(owner), may(element), false))
                    }
                    _ => None,
                };
                if let Some((access, replacement, whole_root)) = write {
                    for (owner, exact) in &self.affected[&access] {
                        if whole_root && *exact {
                            state[*owner] = replacement;
                        } else {
                            state[*owner] |= replacement;
                        }
                    }
                    if matches!(instruction.operation, Operation::Mutate { .. }) {
                        let target = content_index(self.function, access);
                        state[target] = self.defaults[target] && replacement;
                    }
                }
                for result in &instruction.results {
                    let index = content_index(self.function, *result);
                    state[index] = self.defaults[index] && proof.unwrap_or(true);
                }
            }

            pub(super) fn apply_terminator(&self, terminator: &TerminatorKind, state: &mut State) {
                if let TerminatorKind::NullableBranch { owner, view, .. } = terminator {
                    let view = content_index(self.function, EntityId::Loan(*view));
                    state[view] = self.defaults[view] && state[owner.index()];
                }
            }
        }

        pub(in crate::ssa) fn content_index(function: &Function, entity: EntityId) -> usize {
            match entity {
                EntityId::Value(value) => value.index(),
                EntityId::Place(place) => function.values.len() + place.index(),
                EntityId::Loan(loan) => {
                    function.values.len() + function.places.len() + loan.index()
                }
            }
        }

        /// A mixed root/projected CFG join is a possible storage write, never a proven local root.
        pub(in crate::ssa) fn projected_places(
            function: &Function,
            aliases: &AliasRoots,
        ) -> Vec<bool> {
            let mut projected = vec![false; function.places.len()];
            let mut incoming = vec![false; function.places.len()];
            for block in &function.blocks {
                for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                    for parameter in &function
                        .block(edge.target)
                        .expect("verified target")
                        .parameters
                    {
                        if let EntityId::Place(place) = parameter {
                            incoming[place.index()] = true;
                        }
                    }
                }
            }
            for block in &function.blocks {
                for parameter in &block.parameters {
                    if let EntityId::Place(place) = parameter {
                        projected[place.index()] = !incoming[place.index()]
                            || !aliases.overlap(EntityId::Place(*place), EntityId::Place(*place));
                    }
                }
            }
            for instruction in &function.instructions {
                if matches!(
                    instruction.operation,
                    Operation::FieldPlace { .. }
                        | Operation::ContainerElementPlace { .. }
                        | Operation::RangeElementPlace { .. }
                        | Operation::TaggedPayloadPlace { .. }
                        | Operation::HeapPayloadPlace { .. }
                        | Operation::SharedPayloadPlace { .. }
                ) {
                    let EntityId::Place(place) = instruction.results[0] else {
                        unreachable!("verified place result")
                    };
                    projected[place.index()] = true;
                }
            }
            loop {
                let mut changed = false;
                for block in &function.blocks {
                    for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind)
                    {
                        for (argument, parameter) in edge.arguments.iter().zip(
                            &function
                                .block(edge.target)
                                .expect("verified target")
                                .parameters,
                        ) {
                            if let (EntityId::Place(argument), EntityId::Place(parameter)) =
                                (argument, parameter)
                                && projected[argument.index()]
                                && !projected[parameter.index()]
                            {
                                projected[parameter.index()] = true;
                                changed = true;
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            projected
        }
    }

    use std::collections::VecDeque;

    use crate::ssa::{
        model::{
            ClosureCaptureMode, EntityId, Function, Module, Operation, SsaTypeId, SsaTypeKind,
            TerminatorKind,
        },
        verify::{VerifyError, VerifyErrorKind, VerifyLocation},
    };

    #[cfg(test)]
    mod graph_tests {
        use super::*;
        use crate::ssa::model::{ClosureCaptureType, Program};

        fn borrowed(module: &mut Module) -> SsaTypeId {
            let integer = module.intern_type(SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            });
            let reference = module
                .add_shared_reference_type(integer)
                .expect("reference");
            let environment = module
                .add_aggregate_type("Env", vec![reference])
                .expect("environment");
            module
                .add_concrete_closure_type(
                    "Borrowed",
                    vec![],
                    vec![],
                    environment,
                    vec![ClosureCaptureType {
                        mode: ClosureCaptureMode::Shared,
                        ty: integer,
                    }],
                )
                .expect("closure")
        }

        #[test]
        fn capture_type_graph_deep_storage_uses_iteration_and_visits_each_edge_once() {
            let mut program = Program::default();
            let id = program.add_module("deep-capture-types");
            let module = program.module_mut(id).expect("module");
            let mut current = borrowed(module);
            // This is an IR type-classification test, not a claim of source/native deep-layout support.
            for index in 0..2000 {
                current = module
                    .add_aggregate_type(format!("Layer{index}"), vec![current])
                    .expect("layer");
            }
            let proof = TypeCaptures::compute(module);
            assert!(proof.contains(current));
            assert_eq!(proof.stats.node_visits, 2004);
            assert_eq!(proof.stats.storage_edge_visits, 2001);
            assert_eq!(proof.stats.propagation_edge_visits, 2000);
        }

        #[test]
        fn capture_type_graph_shared_dag_counts_storage_edges_instead_of_expanding_paths() {
            let mut program = Program::default();
            let id = program.add_module("shared-capture-types");
            let module = program.module_mut(id).expect("module");
            let mut current = borrowed(module);
            // Each layer has two paths to one existing identity, never two newly cloned subgraphs.
            for index in 0..256 {
                current = module
                    .add_aggregate_type(format!("Pair{index}"), vec![current, current])
                    .expect("pair");
            }
            let proof = TypeCaptures::compute(module);
            assert!(proof.contains(current));
            assert_eq!(proof.stats.node_visits, 260);
            assert_eq!(proof.stats.storage_edge_visits, 513);
            assert_eq!(proof.stats.propagation_edge_visits, 512);
        }

        #[test]
        fn capture_type_graph_owner_cycles_reach_a_fixed_point_and_clean_cycles_stay_clean() {
            let mut program = Program::default();
            let id = program.add_module("recursive-capture-types");
            let module = program.module_mut(id).expect("module");
            let closure = borrowed(module);
            let owner = module.declare_heap_owner("Recursive").expect("owner");
            let payload = module
                .add_aggregate_type("Payload", vec![owner, closure])
                .expect("payload");
            module
                .define_heap_owner(owner, payload)
                .expect("recursive payload");
            let clean_owner = module
                .declare_heap_owner("CleanRecursive")
                .expect("clean owner");
            let clean_payload = module
                .add_aggregate_type("CleanPayload", vec![clean_owner])
                .expect("clean payload");
            module
                .define_heap_owner(clean_owner, clean_payload)
                .expect("clean recursive payload");
            let proof = TypeCaptures::compute(module);
            assert!(proof.contains(owner));
            assert!(proof.contains(payload));
            assert!(!proof.contains(clean_owner));
            assert!(!proof.contains(clean_payload));
            assert_eq!(proof.stats.node_visits, 8);
            assert_eq!(proof.stats.storage_edge_visits, 6);
            assert_eq!(proof.stats.propagation_edge_visits, 3);
        }

        #[test]
        fn capture_type_graph_does_not_treat_signatures_or_reference_targets_as_owned_storage() {
            let mut program = Program::default();
            let id = program.add_module("capture-storage-boundaries");
            let module = program.module_mut(id).expect("module");
            let closure = borrowed(module);
            let pointer = module
                .add_function_pointer_type(vec![closure], vec![closure])
                .expect("pointer signature");
            let reference = module
                .add_shared_reference_type(closure)
                .expect("reference target");
            let wrapper = module
                .add_aggregate_type("LeafWrapper", vec![pointer, reference])
                .expect("wrapper");
            let proof = TypeCaptures::compute(module);
            assert!(proof.contains(closure));
            for leaf in [pointer, reference, wrapper] {
                assert!(!proof.contains(leaf));
            }
            assert_eq!(proof.stats.node_visits, 7);
            assert_eq!(proof.stats.storage_edge_visits, 3);
            assert_eq!(proof.stats.propagation_edge_visits, 0);
        }
    }

    const REASON: &str = "borrowed closure cannot escape through owned value delivery";

    pub(in crate::ssa) struct TypeCaptures {
        may_contain: Vec<bool>,
        #[cfg(test)]
        stats: ClassificationStats,
    }

    #[cfg(test)]
    #[derive(Default)]
    struct ClassificationStats {
        node_visits: usize,
        storage_edge_visits: usize,
        propagation_edge_visits: usize,
    }

    impl TypeCaptures {
        /// The module's type and deinit checks must have succeeded before indexing storage edges.
        pub(in crate::ssa) fn compute(module: &Module) -> Self {
            let mut parents = vec![Vec::new(); module.types.len()];
            let mut may_contain = vec![false; module.types.len()];
            let mut pending = VecDeque::new();
            #[cfg(test)]
            let mut stats = ClassificationStats::default();
            for (index, kind) in module.types.iter().enumerate() {
                #[cfg(test)]
                {
                    stats.node_visits += 1;
                }
                let children = match kind {
                    SsaTypeKind::Aggregate { fields, .. } => fields.clone(),
                    SsaTypeKind::TaggedUnion { variants, .. } => variants.clone(),
                    SsaTypeKind::HeapOwner { payload, .. }
                    | SsaTypeKind::SharedOwner { payload, .. } => payload.iter().copied().collect(),
                    SsaTypeKind::NullableHandle { inner }
                    | SsaTypeKind::InlineNullable { inner, .. } => vec![*inner],
                    SsaTypeKind::SequentialContainer { element, .. } => vec![*element],
                    SsaTypeKind::ConcreteClosure { captures, .. } => {
                        if captures
                            .iter()
                            .any(|capture| capture.mode == ClosureCaptureMode::Shared)
                        {
                            may_contain[index] = true;
                            pending.push_back(index);
                        }
                        captures
                            .iter()
                            .filter(|capture| capture.mode == ClosureCaptureMode::Owned)
                            .map(|capture| capture.ty)
                            .collect()
                    }
                    _ => Vec::new(),
                };
                for child in children {
                    parents[child.index()].push(index);
                    #[cfg(test)]
                    {
                        stats.storage_edge_visits += 1;
                    }
                }
            }
            while let Some(child) = pending.pop_front() {
                for parent in &parents[child] {
                    #[cfg(test)]
                    {
                        stats.propagation_edge_visits += 1;
                    }
                    if !may_contain[*parent] {
                        may_contain[*parent] = true;
                        pending.push_back(*parent);
                    }
                }
            }
            Self {
                may_contain,
                #[cfg(test)]
                stats,
            }
        }

        pub(in crate::ssa) fn contains(&self, ty: SsaTypeId) -> bool {
            self.may_contain[ty.index()]
        }
    }

    pub(in crate::ssa) fn verify(
        function: &Function,
        types: &TypeCaptures,
        errors: &mut Vec<VerifyError>,
    ) {
        if !function
            .values
            .iter()
            .any(|entity| types.contains(entity.ty.semantic_type()))
        {
            return;
        }
        let proof = proof::ValueProof::new(function, types);
        let entries = proof.block_entries();
        for block in &function.blocks {
            let mut state = entries[block.id.index()].clone();
            for id in &block.instructions {
                let instruction = function.instruction(*id).expect("verified instruction");
                let may = |value: crate::ssa::model::ValueId| state[value.index()];
                let escapes = match &instruction.operation {
                    Operation::DirectCall {
                        receiver,
                        arguments,
                        ..
                    } => receiver
                        .iter()
                        .chain(arguments)
                        .any(|argument| matches!(argument, EntityId::Value(value) if may(*value))),
                    Operation::BorrowCall { arguments, .. }
                    | Operation::RangeCall { arguments, .. }
                    | Operation::CallableInvoke { arguments, .. }
                    | Operation::CallableInvokeView { arguments, .. } => arguments
                        .iter()
                        .any(|argument| matches!(argument, EntityId::Value(value) if may(*value))),
                    Operation::AggregateConstruct { fields, .. } => fields.iter().copied().any(may),
                    Operation::ContainerConstruct { elements, .. } => {
                        elements.iter().copied().any(may)
                    }
                    Operation::TaggedConstruct { payload, .. }
                    | Operation::HeapAllocate { payload, .. }
                    | Operation::SharedAllocate { payload, .. } => may(*payload),
                    Operation::HeapFieldReplace { value, .. }
                    | Operation::InlineFieldReplace { value, .. }
                    | Operation::ContainerReplace { value, .. }
                    | Operation::ContainerAppend { element: value, .. }
                    | Operation::ContainerInsertAt { element: value, .. } => may(*value),
                    Operation::HeapFieldExchange { replacement, .. } => may(*replacement),
                    Operation::Mutate { place, value } => proof.is_projected(*place) && may(*value),
                    Operation::LoanMutate { value, .. } => may(*value),
                    _ => false,
                };
                if escapes {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::OperationContract { reason: REASON },
                        location: VerifyLocation::Instruction(instruction.id),
                        origin: Some(instruction.origin.clone()),
                    });
                }
                proof.apply(instruction, &mut state);
            }
            let terminator = block.terminator.as_ref().expect("verified terminator");
            proof.apply_terminator(&terminator.kind, &mut state);
            if let TerminatorKind::Return { values } = &terminator.kind
                && values.iter().any(|value| state[value.index()])
            {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::OperationContract { reason: REASON },
                    location: VerifyLocation::Terminator(block.id),
                    origin: Some(terminator.origin.clone()),
                });
            }
        }
    }
}
