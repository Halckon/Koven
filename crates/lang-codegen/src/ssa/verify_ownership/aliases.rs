//! SSA entity storage alias roots 与 CFG 不动点，以及 direct owned heap sibling fields 证明。
use super::*;

pub(in crate::ssa) mod direct_fields {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Path {
        Unknown,
        Payload(EntityId, SsaTypeId),
        Field(EntityId, SsaTypeId, usize),
    }

    pub(in crate::ssa) struct DirectFields(BTreeMap<EntityId, BTreeSet<Path>>);

    impl DirectFields {
        pub(in crate::ssa) fn compute(
            function: &Function,
            roots: &BTreeMap<EntityId, BTreeSet<EntityId>>,
        ) -> Self {
            if !function.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::HeapPayloadPlace { .. })
            }) {
                return Self(BTreeMap::new());
            }
            let incoming_blocks: BTreeSet<_> = function
                .blocks
                .iter()
                .flat_map(|block| {
                    edges(&block.terminator.as_ref().expect("verified terminator").kind)
                        .into_iter()
                        .map(|edge| edge.target)
                })
                .collect();
            let mut paths: BTreeMap<_, BTreeSet<_>> = all_entities(function)
                .into_iter()
                .map(|id| (id, BTreeSet::new()))
                .collect();
            for block in &function.blocks {
                if block.id.index() == 0 || !incoming_blocks.contains(&block.id) {
                    for parameter in &block.parameters {
                        paths.get_mut(parameter).unwrap().insert(Path::Unknown);
                    }
                }
            }
            loop {
                let mut changed = false;
                for instruction in &function.instructions {
                    for result in &instruction.results {
                        let incoming: BTreeSet<_> = match &instruction.operation {
                            Operation::HeapPayloadPlace { owner } => {
                                let owner = EntityId::Value(*owner);
                                match roots[&owner].iter().copied().collect::<Vec<_>>().as_slice() {
                                    [root] => [Path::Payload(
                                        *root,
                                        function.entity(owner).unwrap().ty.semantic_type(),
                                    )]
                                    .into(),
                                    _ => [Path::Unknown].into(),
                                }
                            }
                            Operation::FieldPlace { base, field } => paths[&EntityId::Place(*base)]
                                .iter()
                                .map(|path| match path {
                                    Path::Payload(root, ty) => Path::Field(*root, *ty, *field),
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::BorrowBegin { place, .. } => paths[&EntityId::Place(*place)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) => *path,
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::SharedReborrow { source } => paths[&EntityId::Loan(*source)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) => *path,
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            _ => [Path::Unknown].into(),
                        };
                        let target = paths.get_mut(result).unwrap();
                        let before = target.len();
                        target.extend(incoming);
                        changed |= target.len() != before;
                    }
                }
                for block in &function.blocks {
                    for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind)
                    {
                        let target = function.block(edge.target).unwrap();
                        for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                            let incoming = paths[argument].clone();
                            let target = paths.get_mut(parameter).unwrap();
                            let before = target.len();
                            target.extend(incoming);
                            changed |= target.len() != before;
                        }
                    }
                }
                if !changed {
                    for path in paths.values_mut().filter(|path| path.is_empty()) {
                        path.insert(Path::Unknown);
                        changed = true;
                    }
                    if !changed {
                        break;
                    }
                }
            }
            Self(paths)
        }

        pub(in crate::ssa) fn disjoint(
            &self,
            left: EntityId,
            right: EntityId,
        ) -> bool {
            let (Some(left), Some(right)) = (self.0.get(&left), self.0.get(&right)) else {
                return false;
            };
            if left.len() != 1 || right.len() != 1 {
                return false;
            }
            matches!((left.first(), right.first()),
                (Some(Path::Field(a, ta, fa)), Some(Path::Field(b, tb, fb)))
                    if a == b && ta == tb && fa != fb)
        }
    }
}

impl AliasRoots {
    pub(in crate::ssa) fn compute(function: &Function) -> Self {
        let mut roots = all_entities(function)
            .into_iter()
            .map(|entity| (entity, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        let mut incoming = BTreeMap::<EntityId, usize>::new();
        for block in &function.blocks {
            let terminator = block.terminator.as_ref().expect("terminator must exist");
            for edge in edges(&terminator.kind) {
                let target = function.block(edge.target).expect("target must exist");
                for parameter in &target.parameters {
                    *incoming.entry(*parameter).or_default() += 1;
                }
            }
        }
        for block in &function.blocks {
            for parameter in &block.parameters {
                if block.id.index() == 0 || incoming.get(parameter).copied().unwrap_or(0) == 0 {
                    roots
                        .get_mut(parameter)
                        .expect("parameter root exists")
                        .insert(*parameter);
                }
            }
        }
        for instruction in &function.instructions {
            for result in &instruction.results {
                if matches!(result, EntityId::Value(_)) {
                    roots
                        .get_mut(result)
                        .expect("result root exists")
                        .insert(*result);
                }
            }
        }

        loop {
            let mut changed = false;
            for block in &function.blocks {
                let terminator = block.terminator.as_ref().expect("terminator must exist");
                for edge in edges(&terminator.kind) {
                    let target = function.block(edge.target).expect("target must exist");
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        changed |= union_from(&mut roots, *parameter, *argument);
                    }
                }
            }
            for instruction in &function.instructions {
                match &instruction.operation {
                    Operation::RootPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::BorrowBegin { place, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*place));
                    }
                    Operation::HeapPayloadPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::SharedPayloadPlace { owner } => {
                        changed |= union_from(&mut roots, instruction.results[0], *owner);
                    }
                    Operation::RangeElementPlace { view, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*view));
                    }
                    Operation::ContainerElementPlace { owner, .. } => {
                        changed |= union_from(&mut roots, instruction.results[0], *owner);
                    }
                    Operation::FieldPlace { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*base));
                    }
                    Operation::SharedFieldLoan { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*base));
                    }
                    Operation::SharedHeapFieldLoan { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*base));
                    }
                    Operation::RangeConstruct { source, .. }
                    | Operation::RangeCall { source, .. } => {
                        if let Some(loan) = instruction
                            .results
                            .iter()
                            .find(|entity| matches!(entity, EntityId::Loan(_)))
                            && let Some(root) = range::source::root(function, *source)
                        {
                            changed |= union_from(&mut roots, *loan, EntityId::Loan(root));
                        }
                    }
                    Operation::SharedReborrow { source }
                    | Operation::CallableViewOf { source }
                    | Operation::SharedReferenceFollow { source }
                    | Operation::BorrowCall { source, .. }
                    | Operation::MapIterationKeyLoan { source, .. }
                    | Operation::MapIterationValueLoan { source, .. }
                    | Operation::MapRequireValue { source, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*source));
                    }
                    _ => {}
                }
            }
            for block in &function.blocks {
                let Some(terminator) = &block.terminator else {
                    continue;
                };
                if let TerminatorKind::NullableBranch { owner, view, .. } = terminator.kind {
                    changed |= union_from(&mut roots, EntityId::Loan(view), EntityId::Value(owner));
                }
            }
            if !changed {
                break;
            }
        }

        let direct_fields = direct_fields::DirectFields::compute(function, &roots);
        Self {
            roots,
            direct_fields,
        }
    }

    pub(in crate::ssa) fn overlap(&self, left: EntityId, right: EntityId) -> bool {
        let left = self.roots.get(&left).expect("left alias roots must exist");
        let right = self
            .roots
            .get(&right)
            .expect("right alias roots must exist");
        left.iter().any(|root| right.contains(root))
    }
}

pub(in crate::ssa) mod provider_lifetime {
    //! Bounded path-correlated loan provenance for provider places; no may-origin union.
    use super::super::super::{
        model::{
            BlockId, Definition, EntityId, Function, LoanId, Operation, PlaceId, TerminatorKind,
            ValueId,
        },
        verify::{VerifyError, VerifyErrorKind, VerifyLocation, compute_dominators},
    };
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    struct EntryProjection {
        dominators: Vec<BTreeSet<usize>>,
    }

    impl EntryProjection {
        fn new(function: &Function) -> Self {
            Self {
                dominators: compute_dominators(function),
            }
        }

        fn retain_entry(&self, function: &Function, block: BlockId, state: &mut State) {
            let mut loans = state
                .loans
                .keys()
                .copied()
                .filter(|loan| self.available(function, block, EntityId::Loan(*loan)))
                .collect::<BTreeSet<_>>();
            state
                .places
                .retain(|place, _| self.available(function, block, EntityId::Place(*place)));
            state
                .cursors
                .retain(|value, _| self.available(function, block, EntityId::Value(*value)));
            let mut pending = loans.iter().copied().collect::<Vec<_>>();
            for reference in state.places.values().chain(state.cursors.values()) {
                pending.extend(reference.lineage.iter().copied());
            }
            let mut expanded = BTreeSet::new();
            while let Some(loan) = pending.pop() {
                if !expanded.insert(loan) {
                    continue;
                }
                if let Some(reference) = state.loans.get(&loan) {
                    loans.insert(loan);
                    pending.extend(reference.lineage.iter().copied());
                }
            }
            state.loans.retain(|loan, _| loans.contains(loan));
        }

        fn available(&self, function: &Function, block: BlockId, entity: EntityId) -> bool {
            let definition = function.entity(entity).expect("verified entity").definition;
            let defining_block = match definition {
                Definition::BlockParameter { block, .. } => block,
                Definition::InstructionResult { instruction, .. } => {
                    let defining_block = function
                        .instruction(instruction)
                        .expect("verified instruction")
                        .block;
                    if defining_block == block {
                        return false;
                    }
                    defining_block
                }
            };
            self.dominators[block.index()].contains(&defining_block.index())
        }
    }


    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Reference {
        lineage: BTreeSet<LoanId>,
        live: bool,
    }

    #[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct State {
        loans: BTreeMap<LoanId, Reference>,
        places: BTreeMap<PlaceId, Reference>,
        cursors: BTreeMap<ValueId, Reference>,
    }

    impl State {
        fn end(&mut self, loan: LoanId) {
            let Some(reference) = self.loans.get(&loan).cloned() else {
                return;
            };
            let token = reference
                .lineage
                .iter()
                .find(|token| {
                    self.loans
                        .get(token)
                        .is_some_and(|root| root.lineage == reference.lineage)
                })
                .copied()
                .unwrap_or(loan);
            for reference in self
                .loans
                .values_mut()
                .chain(self.places.values_mut())
                .chain(self.cursors.values_mut())
            {
                if reference.lineage.contains(&token) {
                    reference.live = false;
                }
            }
        }
    }

    pub(in crate::ssa) fn verify(
        function: &Function,
        errors: &mut Vec<VerifyError>,
    ) {
        if !function.instructions.iter().any(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
                    | Operation::RangeElementPlace { .. }
                    | Operation::MapIterationFirst { .. }
                    | Operation::MapIterationHasNext { .. }
                    | Operation::MapIterationNext { .. }
                    | Operation::MapIterationKeyLoan { .. }
                    | Operation::MapIterationValueLoan { .. }
            )
        }) {
            return;
        }
        let Some(entry) = function.entry_block() else {
            return;
        };
        let mut initial = State::default();
        for entity in &function.block(entry).expect("verified entry").parameters {
            if let EntityId::Loan(loan) = entity {
                initial.loans.insert(
                    *loan,
                    Reference {
                        lineage: BTreeSet::from([*loan]),
                        live: true,
                    },
                );
            }
        }
        let projection = EntryProjection::new(function);
        let mut queue = VecDeque::from([(entry, initial)]);
        let mut visited = BTreeSet::<(BlockId, State)>::new();
        let budget = function.blocks.len().saturating_mul(256).max(256);
        while let Some((block_id, mut state)) = queue.pop_front() {
            projection.retain_entry(function, block_id, &mut state);
            if !visited.insert((block_id, state.clone())) {
                continue;
            }
            if visited.len() > budget {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::OperationContract {
                        reason: "provider provenance fixed-point budget exceeded",
                    },
                    location: VerifyLocation::Block(block_id),
                    origin: Some(function.origin.clone()),
                });
                return;
            }
            let block = function.block(block_id).expect("verified block");
            for id in &block.instructions {
                let instruction = function.instruction(*id).expect("verified instruction");
                let result = instruction.results.first().copied();
                let inherited = match instruction.operation {
                    Operation::MapIterationFirst { source } => {
                        match map_reference(&state, source, None) {
                            Ok(reference) => Some(reference),
                            Err(kind) => {
                                errors.push(VerifyError {
                                    kind,
                                    location: VerifyLocation::Instruction(*id),
                                    origin: Some(instruction.origin.clone()),
                                });
                                return;
                            }
                        }
                    }
                    Operation::MapIterationHasNext { source, cursor }
                    | Operation::MapIterationNext { source, cursor }
                    | Operation::MapIterationKeyLoan { source, cursor }
                    | Operation::MapIterationValueLoan { source, cursor } => {
                        match map_reference(&state, source, Some(cursor)) {
                            Ok(reference) => Some(reference),
                            Err(kind) => {
                                errors.push(VerifyError {
                                    kind,
                                    location: VerifyLocation::Instruction(*id),
                                    origin: Some(instruction.origin.clone()),
                                });
                                return;
                            }
                        }
                    }
                    Operation::ContainerElementPlace {
                        owner: EntityId::Loan(loan),
                        ..
                    }
                    | Operation::RangeElementPlace { view: loan, .. }
                    | Operation::SharedReborrow { source: loan }
                    | Operation::SharedReferenceFollow { source: loan }
                    | Operation::SharedFieldLoan { base: loan, .. }
                    | Operation::SharedHeapFieldLoan { base: loan, .. } => {
                        let reference = state.loans.get(&loan).cloned();
                        if reference.as_ref().is_some_and(|reference| !reference.live) {
                            errors.push(VerifyError {
                                kind: VerifyErrorKind::LoanInactive { loan },
                                location: VerifyLocation::Instruction(*id),
                                origin: Some(instruction.origin.clone()),
                            });
                            return;
                        }
                        reference
                    }
                    Operation::BorrowBegin { place, .. } => {
                        let reference = state.places.get(&place).cloned();
                        if reference.as_ref().is_some_and(|reference| !reference.live) {
                            errors.push(VerifyError {
                                kind: VerifyErrorKind::PlaceUnavailable { place },
                                location: VerifyLocation::Instruction(*id),
                                origin: Some(instruction.origin.clone()),
                            });
                            return;
                        }
                        reference
                    }
                    Operation::BorrowEnd { loan } => {
                        state.end(loan);
                        None
                    }
                    _ => None,
                };
                match result {
                    Some(EntityId::Value(value)) => {
                        state.cursors.remove(&value);
                        if matches!(
                            instruction.operation,
                            Operation::MapIterationFirst { .. }
                                | Operation::MapIterationNext { .. }
                        ) && let Some(reference) = inherited
                        {
                            state.cursors.insert(value, reference);
                        }
                    }
                    Some(EntityId::Loan(loan)) => {
                        let mut reference = inherited.unwrap_or(Reference {
                            lineage: BTreeSet::new(),
                            live: true,
                        });
                        reference.lineage.insert(loan);
                        state.loans.insert(loan, reference);
                    }
                    Some(EntityId::Place(place)) => {
                        if let Some(reference) = inherited {
                            state.places.insert(place, reference);
                        } else {
                            state.places.remove(&place);
                        }
                    }
                    _ => {}
                }
            }
            let terminator = &block.terminator.as_ref().expect("verified terminator").kind;
            for edge in super::super::edges(terminator) {
                let mut successor = state.clone();
                let target = function.block(edge.target).expect("verified edge");
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    match (argument, parameter) {
                        (EntityId::Loan(from), EntityId::Loan(to)) => {
                            successor.loans.remove(to);
                            if let Some(reference) = state.loans.get(from) {
                                successor.loans.insert(*to, reference.clone());
                            }
                        }
                        (EntityId::Place(from), EntityId::Place(to)) => {
                            successor.places.remove(to);
                            if let Some(reference) = state.places.get(from) {
                                successor.places.insert(*to, reference.clone());
                            }
                        }
                        (EntityId::Value(from), EntityId::Value(to)) => {
                            successor.cursors.remove(to);
                            if let Some(reference) = state.cursors.get(from) {
                                successor.cursors.insert(*to, reference.clone());
                            }
                        }
                        _ => {}
                    }
                }
                if let TerminatorKind::NullableBranch {
                    view,
                    when_non_null,
                    ..
                } = terminator
                    && std::ptr::eq(edge, when_non_null)
                {
                    successor.loans.insert(
                        *view,
                        Reference {
                            lineage: BTreeSet::from([*view]),
                            live: true,
                        },
                    );
                }
                queue.push_back((edge.target, successor));
            }
        }
    }

    /// A cursor belongs to one live provider lease, not merely the same Map storage root.
    fn map_reference(
        state: &State,
        source: LoanId,
        cursor: Option<ValueId>,
    ) -> Result<Reference, VerifyErrorKind> {
        let reference = state
            .loans
            .get(&source)
            .filter(|reference| reference.live)
            .ok_or(VerifyErrorKind::LoanInactive { loan: source })?;
        if let Some(cursor) = cursor {
            let cursor = state
                .cursors
                .get(&cursor)
                .ok_or(VerifyErrorKind::OperationContract {
                    reason: "Map cursor must originate from First/Next and explicit CFG transport",
                })?;
            if !cursor.live || cursor.lineage != reference.lineage {
                return Err(VerifyErrorKind::OperationContract {
                    reason: "Map cursor and source must belong to the same live provider lease",
                });
            }
        }
        Ok(reference.clone())
    }
}

pub(in crate::ssa) mod map {
    use super::super::*;
    use crate::ssa::model::Origin;

    pub(in crate::ssa) mod reborrow {
        use super::super::super::*;

        #[allow(clippy::too_many_arguments)]
        pub(in crate::ssa) fn end_loan(
            function: &Function,
            loan: LoanId,
            aliases: &AliasRoots,
            closure_loans: &closure::ClosureLoans,
            reborrows: &ReborrowDependencies,
            state: &mut BlockState,
            location: VerifyLocation,
            origin: &crate::ssa::model::Origin,
            errors: &mut Vec<VerifyError>,
        ) -> bool {
            let kind = if let Some(owner) = range::live_descriptor(function, loan, state)
                .or_else(|| closure_loans.live_owner_holding(loan, state))
            {
                VerifyErrorKind::OwnerLoanConflict { value: owner }
            } else if let Some(dependent) = reborrows.active_descendant(loan, aliases, state) {
                VerifyErrorKind::LoanDependencyActive {
                    parent: loan,
                    dependent,
                }
            } else if !state.loans.remove(&loan) {
                VerifyErrorKind::LoanInactive { loan }
            } else {
                return true;
            };
            errors.push(error(kind, location, origin));
            false
        }

        impl ReborrowDependencies {
            pub(in crate::ssa) fn compute(
                module: &Module,
                function: &Function,
            ) -> Self {
                let mut parent_by_child = BTreeMap::new();
                let mut must_end_children = BTreeSet::new();
                for instruction in &function.instructions {
                    let (source, must_end) = match instruction.operation {
                        Operation::RangeConstruct { source, .. }
                        | Operation::RangeCall { source, .. } => {
                            let Some(EntityId::Loan(child)) = instruction.results.last() else {
                                continue;
                            };
                            must_end_children.insert(*child);
                            if range::source::is_view(module, function, source) {
                                let Some(root) = range::source::root(function, source) else {
                                    continue;
                                };
                                if function.blocks[0]
                                    .parameters
                                    .contains(&EntityId::Loan(root))
                                {
                                    parent_by_child.insert(*child, root);
                                }
                                continue;
                            }
                            (source, true)
                        }
                        Operation::SharedReborrow { source }
                        | Operation::CallableViewOf { source }
                        | Operation::SharedHeapFieldLoan { base: source, .. }
                        | Operation::BorrowCall { source, .. }
                        | Operation::MapIterationKeyLoan { source, .. }
                        | Operation::MapIterationValueLoan { source, .. }
                        | Operation::MapRequireValue { source, .. } => (source, true),
                        Operation::SharedFieldLoan { base: source, .. }
                        | Operation::SharedReferenceFollow { source } => (source, false),
                        Operation::BorrowBegin { place, .. } => {
                            let Some(entity) = function.entity(EntityId::Place(place)) else {
                                continue;
                            };
                            let Definition::InstructionResult { instruction, .. } =
                                entity.definition
                            else {
                                continue;
                            };
                            let Some(instruction) = function.instruction(instruction) else {
                                continue;
                            };
                            let source = match instruction.operation {
                                Operation::ContainerElementPlace {
                                    owner: EntityId::Loan(source),
                                    ..
                                }
                                | Operation::RangeElementPlace { view: source, .. } => source,
                                _ => continue,
                            };
                            (source, true)
                        }
                        _ => continue,
                    };
                    let Some(EntityId::Loan(child)) = instruction
                        .results
                        .iter()
                        .find(|entity| matches!(entity, EntityId::Loan(_)))
                    else {
                        continue;
                    };
                    parent_by_child.insert(*child, source);
                    if must_end {
                        must_end_children.insert(*child);
                    }
                }
                Self {
                    parent_by_child,
                    must_end_children,
                    flows: LoanFlowAliases::compute(function),
                }
            }

            pub(in crate::ssa) fn is_derived(
                &self,
                loan: LoanId,
                _aliases: &AliasRoots,
            ) -> bool {
                self.must_end_children
                    .iter()
                    .any(|child| self.flows.equivalent(loan, *child))
            }

            pub(in crate::ssa) fn active_descendant(
                &self,
                parent: LoanId,
                _aliases: &AliasRoots,
                state: &BlockState,
            ) -> Option<LoanId> {
                self.active_descendant_among(parent, state.loans.iter().copied())
            }

            pub(in crate::ssa) fn active_descendant_among(
                &self,
                parent: LoanId,
                active_loans: impl Iterator<Item = LoanId> + Clone,
            ) -> Option<LoanId> {
                for child in self.parent_by_child.keys() {
                    let mut cursor = *child;
                    let mut visited = BTreeSet::new();
                    while visited.insert(cursor) {
                        let Some(next) = self.parent_by_child.get(&cursor).copied() else {
                            break;
                        };
                        if self.flows.equivalent(parent, next) {
                            if let Some(active) = active_loans
                                .clone()
                                .find(|active| self.flows.equivalent(*active, *child))
                            {
                                return Some(active);
                            }
                            break;
                        }
                        cursor = next;
                    }
                }
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa) fn verify_map_ownership(
        module: &Module,
        function: &Function,
        operation: &Operation,
        aliases: &AliasRoots,
        reborrows: &ReborrowDependencies,
        state: &mut BlockState,
        location: VerifyLocation,
        origin: &Origin,
        errors: &mut Vec<VerifyError>,
    ) {
        match operation {
            Operation::MapIterationFirst { source } => {
                check_map_read_owner(
                    module,
                    function,
                    EntityId::Loan(*source),
                    state,
                    location,
                    origin,
                    errors,
                );
            }
            Operation::MapIterationHasNext { source, cursor }
            | Operation::MapIterationNext { source, cursor }
            | Operation::MapIterationKeyLoan { source, cursor }
            | Operation::MapIterationValueLoan { source, cursor } => {
                check_map_read_owner(
                    module,
                    function,
                    EntityId::Loan(*source),
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                require_value(
                    module,
                    function,
                    *cursor,
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                if matches!(operation, Operation::MapIterationNext { .. })
                    && let Some(dependent) = reborrows.active_descendant(*source, aliases, state)
                {
                    errors.push(error(
                        VerifyErrorKind::LoanDependencyActive {
                            parent: *source,
                            dependent,
                        },
                        location,
                        origin,
                    ));
                }
            }
            Operation::MapConsume { owner, .. } => {
                check_and_consume_container_owner(
                    module, function, *owner, aliases, state, location, origin, errors,
                );
            }
            Operation::MapConstruct { .. } => {}
            Operation::MapResultUnwrap { result } => {
                require_value(module, function, *result, state, location, origin, errors);
            }
            Operation::MapSize { owner } => {
                check_map_read_owner(module, function, *owner, state, location, origin, errors);
            }
            Operation::MapContains { owner, key } | Operation::MapGet { owner, key } => {
                check_map_read_owner(
                    module,
                    function,
                    *owner,
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                check_map_read_owner(module, function, *key, state, location, origin, errors);
            }
            Operation::MapRequireValue { source, key } => {
                check_map_read_owner(
                    module,
                    function,
                    EntityId::Loan(*source),
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                check_map_read_owner(module, function, *key, state, location, origin, errors);
            }
            Operation::MapWithValue {
                source,
                key,
                action,
            } => {
                check_map_read_owner(
                    module,
                    function,
                    EntityId::Loan(*source),
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                check_map_read_owner(
                    module,
                    function,
                    *key,
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                check_map_read_owner(
                    module,
                    function,
                    EntityId::Loan(*action),
                    state,
                    location,
                    origin,
                    errors,
                );
            }
            Operation::MapPut { owner, key, value } => {
                check_and_consume_container_owner(
                    module,
                    function,
                    *owner,
                    aliases,
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                consume_value(
                    module,
                    function,
                    *key,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
                consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location,
                    origin,
                    errors,
                );
            }
            Operation::MapRemove { owner, key } => {
                check_and_consume_container_owner(
                    module,
                    function,
                    *owner,
                    aliases,
                    state,
                    location.clone(),
                    origin,
                    errors,
                );
                check_map_read_owner(module, function, *key, state, location, origin, errors);
            }
            _ => {}
        }
    }

    fn check_map_read_owner(
        module: &Module,
        function: &Function,
        owner: EntityId,
        state: &BlockState,
        location: VerifyLocation,
        origin: &Origin,
        errors: &mut Vec<VerifyError>,
    ) {
        match owner {
            EntityId::Value(owner) => {
                require_value(module, function, owner, state, location, origin, errors);
            }
            EntityId::Loan(loan) => {
                if !state.loans.contains(&loan) {
                    errors.push(error(
                        VerifyErrorKind::LoanInactive { loan },
                        location,
                        origin,
                    ));
                }
            }
            EntityId::Place(_) => unreachable!("operation contract rejects a place owner"),
        }
    }
}
