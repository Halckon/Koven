//! SSA entity storage alias roots 与 CFG 不动点，以及 direct owned heap sibling fields 与顺序容器常量下标证明。
use super::*;

pub(in crate::ssa) mod direct_fields {
    use super::*;
    use crate::ssa::model::{CheckedArithmeticOperator, ScalarConstant};

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Path {
        Unknown,
        Payload(EntityId, SsaTypeId),
        Field(EntityId, SsaTypeId, usize),
        Element(SsaTypeId, i32),
    }

    pub(in crate::ssa) struct DirectFields(BTreeMap<EntityId, BTreeSet<Path>>);

    impl DirectFields {
        pub(in crate::ssa) fn compute(
            function: &Function,
            roots: &BTreeMap<EntityId, BTreeSet<EntityId>>,
            module: Option<&Module>,
        ) -> Self {
            let element_paths = module.filter(|module| {
                function.instructions.iter().any(|instruction| {
                    matches!(instruction.results.as_slice(), [EntityId::Loan(loan)]
                        if super::super::element_swap::is_mutable_element(module, function, *loan))
                })
            });
            if element_paths.is_none()
                && !function.instructions.iter().any(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::HeapPayloadPlace { .. }
                            | Operation::ExclusiveHeapFieldLoan { .. }
                    ) || matches!(instruction.operation, Operation::SharedHeapFieldLoan { base, .. }
                    if loan_kind(function, base) == LoanKind::Exclusive)
                })
            {
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
            let forwards = value_forwarding(function);
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
                            Operation::HeapPayloadPlace { owner: value } => {
                                let owner = EntityId::Value(*value);
                                let root = roots[&owner]
                                    .first()
                                    .copied()
                                    .filter(|_| roots[&owner].len() == 1)
                                    .unwrap_or_else(|| {
                                        EntityId::Value(forwarded_owner(&forwards, *value))
                                    });
                                [Path::Payload(
                                    root,
                                    function.entity(owner).unwrap().ty.semantic_type(),
                                )]
                                .into()
                            }
                            Operation::ExclusiveHeapFieldLoan { base, field }
                            | Operation::SharedHeapFieldLoan { base, field }
                                if loan_kind(function, *base) == LoanKind::Exclusive =>
                            {
                                let base = EntityId::Loan(*base);
                                match roots[&base].first().filter(|_| roots[&base].len() == 1) {
                                    Some(root) => [Path::Field(
                                        *root,
                                        function.entity(base).unwrap().ty.semantic_type(),
                                        *field,
                                    )]
                                    .into(),
                                    None => [Path::Unknown].into(),
                                }
                            }
                            Operation::FieldPlace { base, field } => paths[&EntityId::Place(*base)]
                                .iter()
                                .map(|path| match path {
                                    Path::Payload(root, ty) => Path::Field(*root, *ty, *field),
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::ContainerElementPlace { owner, index }
                                if element_paths.is_some_and(|module| {
                                    super::super::element_swap::is_mutable_owner(
                                        module, function, *owner,
                                    )
                                }) =>
                            {
                                let ty = function.entity(*owner).unwrap().ty.semantic_type();
                                let path = constant_index(function, &forwards, *index)
                                    .map_or(Path::Unknown, |index| Path::Element(ty, index));
                                [path].into()
                            }
                            Operation::BorrowBegin { place, .. } => paths[&EntityId::Place(*place)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) | Path::Element(..) => *path,
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::SharedReborrow { source } => paths[&EntityId::Loan(*source)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) | Path::Element(..) => *path,
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

        pub(in crate::ssa::verify_ownership) fn disjoint_from_receiver_field(
            &self,
            loan: LoanId,
            root: EntityId,
            ty: SsaTypeId,
            field: usize,
        ) -> bool {
            let Some(paths) = self.0.get(&EntityId::Loan(loan)) else {
                return false;
            };
            paths.len() == 1
                && matches!(paths.first(), Some(Path::Field(other_root, other_ty, other_field))
                    if *other_root == root && *other_ty == ty && *other_field != field)
        }

        pub(in crate::ssa) fn disjoint(&self, left: EntityId, right: EntityId) -> bool {
            let (Some(left), Some(right)) = (self.0.get(&left), self.0.get(&right)) else {
                return false;
            };
            if left.len() != 1 || right.len() != 1 {
                return false;
            }
            match (left.first(), right.first()) {
                (Some(Path::Field(a, ta, fa)), Some(Path::Field(b, tb, fb))) => {
                    a == b && ta == tb && fa != fb
                }
                _ => self.disjoint_elements_only(left, right),
            }
        }

        pub(in crate::ssa::verify_ownership) fn disjoint_elements(
            &self,
            left: EntityId,
            right: EntityId,
        ) -> bool {
            self.0
                .get(&left)
                .zip(self.0.get(&right))
                .is_some_and(|(left, right)| self.disjoint_elements_only(left, right))
        }

        fn disjoint_elements_only(&self, left: &BTreeSet<Path>, right: &BTreeSet<Path>) -> bool {
            // A sequential owner represents its whole buffer. If two such views
            // alias, their storage starts at the same element with the same stride.
            left.len() == 1
                && right.len() == 1
                && matches!((left.first(), right.first()),
                    (Some(Path::Element(ta, ia)), Some(Path::Element(tb, ib)))
                        if ta == tb && ia != ib)
        }
    }

    /// A CFG value parameter forwards an exact SSA value only if every
    /// incoming edge carries that same value. Entry parameters have no proof.
    fn value_forwarding(function: &Function) -> BTreeMap<ValueId, Option<ValueId>> {
        let mut forwards = BTreeMap::new();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                if edge.target.index() == 0 {
                    continue;
                }
                let parameters = &function.block(edge.target).unwrap().parameters;
                for (parameter, argument) in parameters.iter().zip(&edge.arguments) {
                    if let (EntityId::Value(parameter), EntityId::Value(argument)) =
                        (*parameter, *argument)
                    {
                        let forward = forwards.entry(parameter).or_insert(Some(argument));
                        if *forward != Some(argument) {
                            *forward = None;
                        }
                    }
                }
            }
        }
        forwards
    }

    fn forwarded_owner(forwards: &BTreeMap<ValueId, Option<ValueId>>, start: ValueId) -> ValueId {
        let mut owner = start;
        // Merge points with differing incoming values are identity boundaries.
        // A cycle has no canonical predecessor.
        for _ in 0..=forwards.len() {
            let Some(Some(previous)) = forwards.get(&owner) else {
                return owner;
            };
            owner = *previous;
        }
        start
    }

    fn value_definition(function: &Function, value: ValueId) -> Option<(&Operation, usize)> {
        let Definition::InstructionResult { instruction, index } =
            function.entity(EntityId::Value(value))?.definition
        else {
            return None;
        };
        Some((&function.instruction(instruction)?.operation, index))
    }

    fn constant_index(
        function: &Function,
        forwards: &BTreeMap<ValueId, Option<ValueId>>,
        mut index: ValueId,
    ) -> Option<i32> {
        let mut negate = false;
        let mut has_negation = false;
        let mut seen = BTreeSet::new();
        while seen.insert(index) {
            if let Some(forward) = forwards.get(&index) {
                index = (*forward)?;
                continue;
            }
            match value_definition(function, index)? {
                (Operation::Constant(ScalarConstant::Integer(value)), 0) => {
                    let value = i32::try_from(*value).ok()?;
                    // Any negation of the minimum Int would take the overflow edge.
                    if has_negation && value == i32::MIN {
                        return None;
                    }
                    return Some(if negate { -value } else { value });
                }
                (
                    Operation::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Subtract,
                        left,
                        right,
                    },
                    0,
                ) if matches!(
                    value_definition(function, *left),
                    Some((Operation::Constant(ScalarConstant::Integer(0)), 0))
                ) =>
                {
                    has_negation = true;
                    negate = !negate;
                    index = *right;
                }
                _ => return None,
            }
        }
        None
    }
}

impl AliasRoots {
    pub(in crate::ssa) fn compute(function: &Function) -> Self {
        Self::compute_impl(function, None)
    }

    pub(super) fn for_verifier(module: &Module, function: &Function) -> Self {
        Self::compute_impl(function, Some(module))
    }

    fn compute_impl(function: &Function, module: Option<&Module>) -> Self {
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
                    Operation::SharedHeapFieldLoan { base, .. }
                    | Operation::ExclusiveHeapFieldLoan { base, .. } => {
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
                    | Operation::ExclusiveRootReborrow { source }
                    | Operation::CallableViewOf { source }
                    | Operation::SharedReferenceFollow { source }
                    | Operation::SharedPayloadLoan { source }
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
                let (view, source) = match terminator.kind {
                    TerminatorKind::NullableBranch { owner, view, .. } => {
                        (view, EntityId::Value(owner))
                    }
                    TerminatorKind::NullableLoanBranch { source, view, .. } => {
                        (view, EntityId::Loan(source))
                    }
                    _ => continue,
                };
                changed |= union_from(&mut roots, EntityId::Loan(view), source);
            }
            if !changed {
                break;
            }
        }

        let direct_fields = direct_fields::DirectFields::compute(function, &roots, module);
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

pub(in crate::ssa) mod provider_lifetime;

pub(in crate::ssa) mod map {
    use super::super::*;
    use crate::ssa::model::Origin;

    pub(in crate::ssa) mod reborrow {
        use super::super::super::*;

        #[allow(clippy::too_many_arguments)]
        pub(in crate::ssa::verify_ownership) fn end_loan(
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
            pub(in crate::ssa) fn compute(module: &Module, function: &Function) -> Self {
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
                        | Operation::ExclusiveRootReborrow { source }
                        | Operation::CallableViewOf { source }
                        | Operation::SharedHeapFieldLoan { base: source, .. }
                        | Operation::ExclusiveHeapFieldLoan { base: source, .. }
                        | Operation::SharedPayloadLoan { source }
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
                for block in &function.blocks {
                    if let Some(TerminatorKind::NullableLoanBranch { source, view, .. }) =
                        block.terminator.as_ref().map(|term| &term.kind)
                    {
                        parent_by_child.insert(*view, *source);
                        must_end_children.insert(*view);
                    }
                }
                Self {
                    has_exclusive_root_reborrow: function.instructions.iter().any(|instruction| {
                        matches!(
                            instruction.operation,
                            Operation::ExclusiveRootReborrow { .. }
                        )
                    }),
                    parent_by_child,
                    must_end_children,
                    flows: LoanFlowAliases::compute(function),
                }
            }

            pub(in crate::ssa) fn is_derived(&self, loan: LoanId, _aliases: &AliasRoots) -> bool {
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

            fn unique_origin(&self, loan: LoanId) -> Option<LoanId> {
                let origins = self.flows.origins.get(&loan)?;
                (origins.len() == 1).then(|| *origins.first().expect("unique origin"))
            }

            pub(in crate::ssa::verify_ownership) fn same_origin(
                &self,
                left: LoanId,
                right: LoanId,
            ) -> bool {
                left == right
                    || self
                        .unique_origin(left)
                        .is_some_and(|origin| self.unique_origin(right) == Some(origin))
            }

            /// A unique source on each hop authorizes access through a suspended owner loan.
            pub(in crate::ssa::verify_ownership) fn has_exact_ancestor(
                &self,
                child: LoanId,
                ancestor: LoanId,
            ) -> bool {
                let (Some(mut child), Some(ancestor)) =
                    (self.unique_origin(child), self.unique_origin(ancestor))
                else {
                    return false;
                };
                for _ in 0..self.parent_by_child.len() {
                    let Some(parent) = self
                        .parent_by_child
                        .get(&child)
                        .copied()
                        .and_then(|loan| self.unique_origin(loan))
                    else {
                        return false;
                    };
                    if parent == ancestor {
                        return true;
                    }
                    child = parent;
                }
                false
            }

            pub(in crate::ssa) fn active_descendant_among(
                &self,
                parent: LoanId,
                active_loans: impl Iterator<Item = LoanId> + Clone,
            ) -> Option<LoanId> {
                for child in self.parent_by_child.keys() {
                    let mut pending = vec![*child];
                    let mut visited = BTreeSet::new();
                    while let Some(cursor) = pending.pop() {
                        if !visited.insert(cursor) {
                            continue;
                        }
                        if let Some(&next) = self.parent_by_child.get(&cursor) {
                            if self.flows.equivalent(parent, next)
                                && let Some(active) = active_loans
                                    .clone()
                                    .find(|active| self.flows.equivalent(*active, *child))
                            {
                                return Some(active);
                            }
                            pending.push(next);
                        } else if let Some(origins) = self.flows.origins.get(&cursor) {
                            // A block parameter may carry a reborrow whose parent
                            // is known only at the instruction that created it.
                            pending.extend(origins);
                        }
                    }
                }
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::verify_ownership) fn verify_map_ownership(
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
