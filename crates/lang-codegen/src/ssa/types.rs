//! Target-independent named aggregate and heap-owner type construction.

use super::model::{
    EntityType, FunctionId, LoanKind, MapContainerKind, ModelError, Module, Ownership,
    SequentialContainerKind, SsaTypeId, SsaTypeKind,
};

impl Module {
    /// 返回本 module 唯一的 UTF-8 String owner identity。
    pub(crate) fn add_string_owner_type(&mut self) -> SsaTypeId {
        self.intern_type(SsaTypeKind::StringOwner)
    }

    pub(crate) fn add_aggregate_type(
        &mut self,
        name: impl Into<String>,
        fields: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        let ownership = self.aggregate_ownership(&fields)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::Aggregate {
                name,
                fields,
                ownership,
            },
        ))
    }

    pub(crate) fn add_tagged_union_type(
        &mut self,
        name: impl Into<String>,
        variants: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        if variants.iter().any(|variant| {
            !matches!(
                self.type_kind(*variant),
                Some(SsaTypeKind::Aggregate { .. })
            )
        }) {
            return Err(ModelError::ExpectedAggregate {
                ty: variants.first().copied().unwrap_or(SsaTypeId {
                    module: self.id,
                    index: self.types.len(),
                }),
            });
        }
        let ownership = self.aggregate_ownership(&variants)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::TaggedUnion {
                name,
                variants,
                ownership,
            },
        ))
    }

    pub(crate) fn declare_heap_owner(
        &mut self,
        name: impl Into<String>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::HeapOwner {
                name,
                payload: None,
            },
        ))
    }

    pub(crate) fn define_heap_owner(
        &mut self,
        id: SsaTypeId,
        payload: SsaTypeId,
    ) -> Result<(), ModelError> {
        self.check_type_id(id)?;
        self.check_type_id(payload)?;
        if !matches!(
            self.type_kind(payload),
            Some(SsaTypeKind::Aggregate { .. } | SsaTypeKind::TaggedUnion { .. })
        ) {
            return Err(ModelError::ExpectedAggregate { ty: payload });
        }
        let kind = self
            .types
            .get_mut(id.index())
            .ok_or(ModelError::UnknownType { ty: id })?;
        let SsaTypeKind::HeapOwner {
            payload: definition,
            ..
        } = kind
        else {
            return Err(ModelError::ExpectedHeapOwner { ty: id });
        };
        if definition.is_some() {
            return Err(ModelError::TypeAlreadyDefined { ty: id });
        }
        *definition = Some(payload);
        Ok(())
    }

    pub(crate) fn declare_shared_owner(
        &mut self,
        name: impl Into<String>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::SharedOwner {
                name,
                payload: None,
            },
        ))
    }

    pub(crate) fn define_shared_owner(
        &mut self,
        id: SsaTypeId,
        payload: SsaTypeId,
    ) -> Result<(), ModelError> {
        self.check_type_id(id)?;
        self.check_type_id(payload)?;
        if !self.type_is_defined(payload) {
            return Err(ModelError::UnknownType { ty: payload });
        }
        let kind = self
            .types
            .get_mut(id.index())
            .ok_or(ModelError::UnknownType { ty: id })?;
        let SsaTypeKind::SharedOwner {
            payload: definition,
            ..
        } = kind
        else {
            return Err(ModelError::ExpectedSharedOwner { ty: id });
        };
        if definition.is_some() {
            return Err(ModelError::TypeAlreadyDefined { ty: id });
        }
        *definition = Some(payload);
        Ok(())
    }

    pub(crate) fn add_sequential_container_type(
        &mut self,
        kind: SequentialContainerKind,
        element: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(element)?;
        Ok(self.intern_type(SsaTypeKind::SequentialContainer { kind, element }))
    }

    pub(crate) fn add_map_container_type(
        &mut self,
        kind: MapContainerKind,
        key: SsaTypeId,
        value: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(key)?;
        self.check_type_id(value)?;
        Ok(self.intern_type(SsaTypeKind::MapContainer { kind, key, value }))
    }

    /// 建立只接受已定义 pointer-like owner 的 nullable handle identity。
    pub(crate) fn add_nullable_handle_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        if !self.type_is_defined(inner)
            || !matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. })
            )
        {
            return Err(ModelError::ExpectedPointerLikeOwner { ty: inner });
        }
        Ok(self.intern_type(SsaTypeKind::NullableHandle { inner }))
    }

    pub(crate) fn type_ownership(&self, id: SsaTypeId) -> Option<Ownership> {
        match self.type_kind(id)? {
            SsaTypeKind::Unit
            | SsaTypeKind::Boolean
            | SsaTypeKind::Char
            | SsaTypeKind::Integer { .. } => Some(Ownership::Copyable),
            SsaTypeKind::SharedReference { .. } | SsaTypeKind::CallableView { .. } => {
                Some(Ownership::Copyable)
            }
            SsaTypeKind::Opaque { ownership, .. }
            | SsaTypeKind::ZeroSized { ownership, .. }
            | SsaTypeKind::Aggregate { ownership, .. }
            | SsaTypeKind::TaggedUnion { ownership, .. } => Some(*ownership),
            SsaTypeKind::HeapOwner { .. }
            | SsaTypeKind::SharedOwner { .. }
            | SsaTypeKind::StringOwner
            | SsaTypeKind::NullableHandle { .. } => Some(Ownership::MoveOnly),
            SsaTypeKind::InlineNullable { inner, .. } => self.type_ownership(*inner),
            SsaTypeKind::RangeView { .. }
            | SsaTypeKind::SequentialContainer { .. }
            | SsaTypeKind::MapContainer { .. } => Some(Ownership::MoveOnly),
            SsaTypeKind::FunctionPointer { .. } | SsaTypeKind::ConcreteClosure { .. } => {
                Some(Ownership::MoveOnly)
            }
        }
    }

    pub(crate) fn type_is_defined(&self, id: SsaTypeId) -> bool {
        match self.type_kind(id) {
            Some(
                SsaTypeKind::HeapOwner { payload, .. } | SsaTypeKind::SharedOwner { payload, .. },
            ) => payload.is_some(),
            Some(_) => true,
            None => false,
        }
    }

    pub(crate) fn type_kind(&self, id: SsaTypeId) -> Option<&SsaTypeKind> {
        (id.module() == self.id)
            .then(|| self.types.get(id.index()))
            .flatten()
    }

    pub(crate) fn aggregate_fields(&self, id: SsaTypeId) -> Option<&[SsaTypeId]> {
        match self.type_kind(id)? {
            SsaTypeKind::Aggregate { fields, .. } => Some(fields),
            _ => None,
        }
    }

    pub(crate) fn heap_payload(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::HeapOwner { payload, .. } => *payload,
            _ => None,
        }
    }

    pub(crate) fn shared_payload(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::SharedOwner { payload, .. } => *payload,
            _ => None,
        }
    }

    pub(crate) fn nullable_inner(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::NullableHandle { inner } | SsaTypeKind::InlineNullable { inner, .. } => {
                Some(*inner)
            }
            _ => self.presence_payload_type(id),
        }
    }

    pub(crate) fn tagged_variants(&self, id: SsaTypeId) -> Option<&[SsaTypeId]> {
        match self.type_kind(id)? {
            SsaTypeKind::TaggedUnion { variants, .. } => Some(variants),
            _ => None,
        }
    }

    pub(crate) fn sequential_container(
        &self,
        id: SsaTypeId,
    ) -> Option<(SequentialContainerKind, SsaTypeId)> {
        match self.type_kind(id)? {
            SsaTypeKind::SequentialContainer { kind, element } => Some((*kind, *element)),
            _ => None,
        }
    }

    pub(crate) fn map_container(
        &self,
        id: SsaTypeId,
    ) -> Option<(MapContainerKind, SsaTypeId, SsaTypeId)> {
        match self.type_kind(id)? {
            SsaTypeKind::MapContainer { kind, key, value } => Some((*kind, *key, *value)),
            _ => None,
        }
    }

    fn aggregate_ownership(&self, fields: &[SsaTypeId]) -> Result<Ownership, ModelError> {
        let mut ownership = Ownership::Copyable;
        for field in fields {
            self.check_type_id(*field)?;
            if self.type_ownership(*field) == Some(Ownership::MoveOnly) {
                ownership = Ownership::MoveOnly;
            }
        }
        Ok(ownership)
    }

    pub(super) fn check_new_type_name(&self, name: &str) -> Result<(), ModelError> {
        if name.is_empty() {
            return Err(ModelError::EmptyTypeName);
        }
        if self.named_type_ids.contains_key(name) {
            return Err(ModelError::DuplicateTypeName {
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    pub(super) fn push_named_type(&mut self, name: String, kind: SsaTypeKind) -> SsaTypeId {
        let id = SsaTypeId {
            module: self.id,
            index: self.types.len(),
        };
        self.types.push(kind);
        self.named_type_ids.insert(name, id);
        id
    }

    pub(super) fn check_type_id(&self, ty: SsaTypeId) -> Result<(), ModelError> {
        if ty.module() != self.id {
            return Err(ModelError::WrongTypeOwner {
                expected: self.id,
                actual: ty.module(),
            });
        }
        self.type_kind(ty)
            .map(|_| ())
            .ok_or(ModelError::UnknownType { ty })
    }

    /// Interns `T?` for an inline inner type. Pointer-like owners keep `NullableHandle`;
    /// String and sequential containers use the null data-pointer niche.
    pub(crate) fn add_inline_nullable_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        let niche = self
            .type_is_defined(inner)
            .then(|| inline_nullable_niche(self, inner))
            .flatten()
            .ok_or(ModelError::ExpectedInlineNullableInner { ty: inner })?;
        Ok(self.intern_type(SsaTypeKind::InlineNullable { inner, niche }))
    }

    /// Returns `(inner, niche)` for an ADR-0033 inline nullable type.
    pub(crate) fn inline_nullable(&self, ty: SsaTypeId) -> Option<(SsaTypeId, bool)> {
        match self.type_kind(ty)? {
            SsaTypeKind::InlineNullable { inner, niche } => Some((*inner, *niche)),
            _ => None,
        }
    }

    pub(crate) fn add_integer_range_type(
        &mut self,
        endpoint: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(endpoint)?;
        if !matches!(
            self.type_kind(endpoint),
            Some(SsaTypeKind::Integer {
                bits: 32 | 64,
                signed: true
            })
        ) {
            return Err(ModelError::ExpectedAggregate { ty: endpoint });
        }
        if let Some((&range, _)) = self.integer_ranges.iter().find(|(_, ty)| **ty == endpoint) {
            return Ok(range);
        }
        let empty = self.intern_type(SsaTypeKind::Boolean);
        let range = self.add_aggregate_type(
            format!("$integer.range.t{}", endpoint.index()),
            vec![endpoint, endpoint, empty],
        )?;
        self.integer_ranges.insert(range, endpoint);
        Ok(range)
    }

    pub(crate) fn integer_range_endpoint(&self, range: SsaTypeId) -> Option<SsaTypeId> {
        self.integer_ranges.get(&range).copied()
    }

    pub(super) fn valid_integer_range_type(&self, range: SsaTypeId, endpoint: SsaTypeId) -> bool {
        let Some([first, last, empty]) = self.aggregate_fields(range) else {
            return false;
        };
        matches!(
            self.type_kind(endpoint),
            Some(SsaTypeKind::Integer {
                bits: 32 | 64,
                signed: true
            })
        ) && *first == endpoint
            && *last == endpoint
            && matches!(self.type_kind(*empty), Some(SsaTypeKind::Boolean))
            && self.type_ownership(range) == Some(Ownership::Copyable)
    }

    /// Associate a compiler-only body; callers cannot turn this into an ordinary owned call.
    pub(crate) fn set_deinit(
        &mut self,
        owner: SsaTypeId,
        function: FunctionId,
    ) -> Result<(), ModelError> {
        if !valid_deinit_signature(self, owner, function) {
            return Err(ModelError::InvalidDeinit { owner, function });
        }
        if self.deinits.contains_key(&owner) {
            return Err(ModelError::DeinitAlreadyDefined { owner });
        }
        self.deinits.insert(owner, function);
        Ok(())
    }

    pub(crate) fn deinit(&self, owner: SsaTypeId) -> Option<FunctionId> {
        self.deinits.get(&owner).copied()
    }
}

pub(super) fn valid_deinit_signature(
    module: &Module,
    owner: SsaTypeId,
    function: FunctionId,
) -> bool {
    matches!(module.type_kind(owner), Some(SsaTypeKind::HeapOwner { .. }))
        && module.function(function).is_some_and(|function| {
            function.receiver
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                })
                && function.return_types.is_empty()
                && !function.never_returns
        })
}

/// `Some(niche)` when `inner` may carry an inline nullable wrapper, `None` otherwise.
pub(super) fn inline_nullable_niche(module: &Module, inner: SsaTypeId) -> Option<bool> {
    match module.type_kind(inner)? {
        SsaTypeKind::Boolean | SsaTypeKind::Char | SsaTypeKind::Integer { .. } => Some(false),
        SsaTypeKind::Aggregate { .. } if module.presence_payload_type(inner).is_none() => {
            Some(false)
        }
        SsaTypeKind::TaggedUnion { .. } => Some(false),
        SsaTypeKind::StringOwner | SsaTypeKind::SequentialContainer { .. } => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod closure_runtime_tests {
    //! Concrete constructor callbacks require resource thunks and source-address ABI slots.
    use crate::ssa::{
        model::{EntityId, EntityType, Operation, SsaTypeKind},
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::UnitCallableOrigin,
        parser::Expression,
        source::SourceMap,
        type_checking::{UnitCallableTarget, standard_environments},
    };

    #[test]
    fn unit_runtime_leaf_callback_reaches_deinit_body_and_borrowed_capture_fields() {
        let mut observations = Vec::new();
        for (environment, literal) in [
            (0, "{ index -> Leaf(index) }"),
            (1, "{ index -> Leaf(index + scale.number) }"),
            (2, "move { index -> Leaf(index + scale.number) }"),
        ] {
            let text = format!(
                "package test\nclass Leaf(val number: Int) {{ deinit() {{ println(\"leaf\") }} }}\nfun entry(): Int {{ val scale = Leaf(7)\nval callback: (Int)->Leaf = {literal}\nval items = List<Leaf>(3, callback)\nreturn items.size }}"
            );
            let mut sources = SourceMap::new();
            let (source, file) = parsed(&mut sources, "test/leaf-thunk.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/leaf-thunk.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let arena = typed.types().types().len();
            let (program, entry) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "test", "entry"),
            )
            .unwrap_or_else(|error| panic!("Leaf environment {environment}: {error:?}"));
            let module = &program.modules[0];
            let thunk = module
                .functions
                .iter()
                .find(|function| function.name.contains(".thunk"))
                .expect("the selected constructor initializer has a concrete thunk");
            assert!(
                thunk.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::HeapAllocate { .. }
                )),
                "each logical callback returns a fresh complete Resource owner"
            );
            assert!(
                thunk.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::HeapAllocate { owner, .. } if module.deinit(owner).is_some())),
                "lambda-only Resource construction must schedule the existing hidden deinit body"
            );
            if environment != 0 {
                assert!(
                    thunk.instructions.iter().any(|instruction| matches!(
                        instruction.operation, Operation::HeapFieldRead { receiver, .. }
                            if matches!(thunk.entity(EntityId::Loan(receiver)).map(|entity| entity.ty),
                                Some(EntityType::Loan { .. }))
                    )),
                    "captured MoveOnly receiver is read through its shared view without moving it"
                );
            }
            crate::llvm::render_verified_program(&program)
                .expect("Resource callback reaches verified LLVM");
            assert_eq!(typed.types().types().len(), arena);
            if environment == 1 {
                // Catch only the new read-only query; all three original source products
                // still complete their real frontend/lowering/SSA/LLVM gates directly.
                observations.push(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || {
                        assert_shared_capture_current_loans(&sources, &program, entry, literal);
                    },
                )));
            }
            println!("Leaf environment {environment}: original SSA/LLVM gates passed");
        }
        assert_eq!(observations.len(), 1, "one Shared product observation");
        let mut failures = Vec::new();
        for result in observations {
            match result {
                Ok(()) => println!("Shared capture current-loan observation: ok"),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string query panic payload");
                    let failure = format!("Shared capture current-loan observation: {message}");
                    println!("{failure}");
                    failures.push(failure);
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn assert_shared_capture_current_loans(
        sources: &SourceMap,
        program: &crate::ssa::model::Program,
        entry: crate::ssa::model::FunctionId,
        literal: &str,
    ) {
        use crate::ssa::model::{
            CheckedArithmeticOperator, ClosureCaptureOperand, Definition, LoanKind, Origin,
            TerminatorKind,
        };

        let module = &program.modules[0];
        let entry = module.function(entry).unwrap();
        let formations = entry
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::ClosureConstruct { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            formations.len(),
            1,
            "one actual captured callback formation"
        );
        let formation = formations[0];
        let Operation::ClosureConstruct {
            thunk, captures, ..
        } = &formation.operation
        else {
            unreachable!()
        };
        assert!(matches!(
            captures.as_slice(),
            [ClosureCaptureOperand::Shared(_)]
        ));
        let function = module.function(*thunk).unwrap();
        let Origin::Source(lambda_span) = &function.origin else {
            panic!("the actual closure thunk must retain its source lambda");
        };
        assert_eq!(
            sources.source_name(lambda_span.source_id()).unwrap(),
            "test/leaf-thunk.ko"
        );
        let source = sources.source_text(lambda_span.source_id()).unwrap();
        let start = source
            .find(literal)
            .expect("the exact Shared lambda exists");
        assert_eq!(
            (lambda_span.start(), lambda_span.end()),
            (start, start + literal.len())
        );
        assert_eq!(sources.slice(*lambda_span).unwrap(), literal);
        assert_eq!(formation.origin.span(), *lambda_span);
        let entry_block = function.block(function.entry_block().unwrap()).unwrap();
        let EntityId::Loan(environment) = entry_block.parameters[0] else {
            panic!("the actual thunk receives its borrowed environment first");
        };
        let parents = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation,
                Operation::SharedFieldLoan { base, field: 0 } if base == environment)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parents.len(),
            1,
            "field zero has one actual environment-slot loan"
        );
        let parent_instruction = parents[0];
        let [EntityId::Loan(parent)] = parent_instruction.results.as_slice() else {
            panic!("SharedFieldLoan produces its parent slot loan");
        };
        let follows = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation,
                Operation::SharedReferenceFollow { source } if source == *parent)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            follows.len(),
            1,
            "follow the actual parent, not a same-typed loan"
        );
        let follow = follows[0];
        let [EntityId::Loan(child)] = follow.results.as_slice() else {
            panic!("the Follow produces its distinct child view");
        };
        assert_ne!(parent, child);
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target: storage,
        } = function.entity(EntityId::Loan(*parent)).unwrap().ty
        else {
            panic!("the environment slot is a shared loan");
        };
        let Some(SsaTypeKind::SharedReference { target }) = module.type_kind(storage) else {
            panic!("the parent holds the actual SharedReference storage");
        };
        assert_eq!(
            function.entity(EntityId::Loan(*child)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *target
            }
        );
        let checked = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::CheckedArithmetic { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(checked.len(), 1);
        let checked = checked[0];
        assert!(matches!(
            checked.operation,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Add,
                ..
            }
        ));
        assert_eq!(
            sources.slice(checked.origin.span()).unwrap(),
            "index + scale.number"
        );
        assert_eq!(checked.origin.span().source_id(), lambda_span.source_id());
        assert!(
            lambda_span.start() <= checked.origin.span().start()
                && checked.origin.span().end() <= lambda_span.end()
        );
        let [EntityId::Value(_), EntityId::Value(failed)] = checked.results.as_slice() else {
            panic!("checked Add returns a scalar and a failure flag");
        };
        let before = function.block(checked.block).unwrap();
        let before_position = |id| {
            before
                .instructions
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap()
        };
        assert_eq!(parent_instruction.block, before.id);
        assert_eq!(follow.block, before.id);
        assert!(before_position(parent_instruction.id) < before_position(follow.id));
        assert!(before_position(follow.id) < before_position(checked.id));
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &before.terminator.as_ref().unwrap().kind
        else {
            panic!("the checked flag selects failure or success");
        };
        assert_eq!(condition, failed);
        assert_eq!(when_true.arguments, when_false.arguments);
        assert_ne!(when_true.target, when_false.target);
        let failure = function.block(when_true.target).unwrap();
        let success = function.block(when_false.target).unwrap();
        assert!(matches!(
            failure.terminator.as_ref().unwrap().kind,
            TerminatorKind::Abort
        ));
        assert!(
            failure.instructions.is_empty(),
            "Abort does not unwind capture views"
        );
        assert_eq!(failure.parameters.len(), when_true.arguments.len());
        assert_eq!(success.parameters.len(), when_false.arguments.len());
        let current = |source| {
            let columns = when_false
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, argument)| (*argument == source).then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(
                columns.len(),
                1,
                "one physical column for this exact source loan"
            );
            let index = columns[0];
            let EntityId::Loan(loan) = success.parameters[index] else {
                panic!("the current capture column is a Loan parameter");
            };
            let parameter = EntityId::Loan(loan);
            assert_ne!(parameter, source, "success must use the rebound identity");
            for (block, parameter) in [(success, parameter), (failure, failure.parameters[index])] {
                let data = function.entity(parameter).unwrap();
                assert_eq!(
                    data.definition,
                    Definition::BlockParameter {
                        block: block.id,
                        index
                    }
                );
                assert_eq!(data.ty, function.entity(source).unwrap().ty);
            }
            (index, loan)
        };
        let (parent_column, current_parent) = current(EntityId::Loan(*parent));
        let (child_column, current_child) = current(EntityId::Loan(*child));
        assert_ne!(
            parent_column, child_column,
            "a parent and its Follow child stay distinct"
        );
        let ends = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::BorrowEnd { loan } => Some((instruction, loan)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            ends.iter().all(|(_, loan)| loan != parent && loan != child),
            "neither pre-CFG loan may be ended instead of its current parameter"
        );
        let end_position = |loan| {
            let matching = ends
                .iter()
                .filter(|(_, actual)| *actual == loan)
                .collect::<Vec<_>>();
            assert_eq!(matching.len(), 1, "end this exact current loan once");
            let instruction = matching[0].0;
            assert_eq!(instruction.block, success.id);
            success
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
                .unwrap()
        };
        assert!(
            end_position(current_child) < end_position(current_parent),
            "end the current Follow child before the current parent slot"
        );
        assert!(
            matches!(
                success.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ),
            "both current loans end on the normal returning path"
        );
    }

    #[test]
    fn unit_runtime_bare_known_function_initializer_uses_a_typed_borrow_slot() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "test/known-function.ko",
            "package test\nfun identity(index: Int): Int = index\nfun entry(): Int { val items = List<Int>(3, identity)\nreturn items[2] }",
        );
        let inputs = [SourceUnitInput::new(
            "root",
            "test/known-function.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let construction = &typed.types().container_constructions()[0];
        let Expression::Call { arguments, .. } = file
            .ast()
            .expressions()
            .get(construction.expression().expression())
            .expect("typed constructor AST")
            .payload()
        else {
            panic!("constructor descriptor identifies a Call")
        };
        let initializer = lang_frontend::type_checking::UnitExpressionId::new(
            construction.expression().source_unit(),
            arguments[1].value,
        );
        assert_eq!(
            owned
                .ownership()
                .callable_origin(initializer)
                .expect("sealed named origin")
                .origin(),
            UnitCallableOrigin::KnownFunction(UnitCallableTarget::Declaration(declaration(
                &names, "test", "identity"
            )))
        );
        let arena = typed.types().types().len();
        let (program, _) = lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &type_environment, &typed, &owned,
            declaration(&names, "test", "entry"),
        ).expect("a canonical source address needs a bounded ABI loan, without invented frontend storage facts");
        let module = &program.modules[0];
        assert!(
            module
                .functions
                .iter()
                .any(|function| function.instructions.iter().any(|instruction| {
                    matches!(instruction.operation, Operation::FunctionAddress { .. })
                }))
        );
        assert!(
            module
                .types
                .iter()
                .any(|ty| matches!(ty, SsaTypeKind::FunctionPointer { .. }))
        );
        assert!(
            !module
                .types
                .iter()
                .any(|ty| matches!(ty, SsaTypeKind::ConcreteClosure { .. }))
        );
        crate::llvm::render_verified_program(&program)
            .expect("known function initializer reaches LLVM");
        assert_eq!(typed.types().types().len(), arena);
    }

    #[test]
    fn unit_runtime_borrowed_resource_parameter_reads_copyable_field() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "test/borrowed-resource-field.ko",
            "package test\nclass Leaf(val number: Int) { deinit() {} }\nfun verify(value: Leaf): Int = value.number\nfun entry(): Int { val item = Leaf(7)\nreturn verify(item) }",
        );
        let inputs = [SourceUnitInput::new(
            "root",
            "test/borrowed-resource-field.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let arena = typed.types().types().len();
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .expect("Borrow Resource parameters expose Copyable fields through their active heap loan");
        let verify = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("test.verify"))
            .expect("selected verifier body");
        assert!(
            verify.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::HeapFieldRead { .. }
            )),
            "reading a Copyable field must retain the complete Resource receiver loan"
        );
        crate::llvm::render_verified_program(&program)
            .expect("borrowed resource field reaches LLVM");
        assert_eq!(typed.types().types().len(), arena);
    }

    #[test]
    fn unit_runtime_captured_resource_string_field_borrows_through_println() {
        for literal in [
            "{ index -> println(scale.name)\nindex }",
            "move { index -> println(scale.name)\nindex }",
        ] {
            let text = format!(
                "package test\nclass Resource(val name: String) {{ deinit() {{ println(this.name) }} }}\nfun entry(): Int {{ val scale = Resource(\"scale\")\nval callback: (Int)->Int = {literal}\nval items = List<Int>(3, callback)\nreturn items.size }}"
            );
            let mut sources = SourceMap::new();
            let (source, file) = parsed(&mut sources, "test/captured-resource-field.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/captured-resource-field.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let arena = typed.types().types().len();
            let (program, _) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "test", "entry"),
            )
            .expect(
                "captured Resource String fields use a child loan through the synchronous print call",
            );
            let thunk = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains(".thunk"))
                .expect("real capture thunk");
            assert!(
                thunk.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::SharedHeapFieldLoan { .. }
                )),
                "MoveOnly String payload must be borrowed from its live captured Resource view"
            );
            assert!(
                !thunk
                    .instructions
                    .iter()
                    .any(|instruction| instruction.results.iter().any(|&result| {
                        let Some(EntityType::Value(ty)) =
                            thunk.entity(result).map(|entity| entity.ty)
                        else {
                            return false;
                        };
                        matches!(
                            program.modules[0].types.get(ty.index()),
                            Some(SsaTypeKind::StringOwner)
                        )
                    })),
                "each callback preserves the captured Resource and String owners"
            );
            crate::llvm::render_verified_program(&program)
                .expect("capture field loan reaches LLVM");
            assert_eq!(typed.types().types().len(), arena);
        }
    }
}
