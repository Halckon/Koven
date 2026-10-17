//! Bounded replay of actual binding updates, using emitted instruction identities.
//! No expression is lowered here and no value is selected merely by its type.
use super::*;
use std::collections::BTreeSet;

impl Fixture {
    pub(super) fn reference(&self, expression: ExpressionId) -> SymbolId {
        let expression = self.ungroup(expression);
        assert!(matches!(
            self.parsed
                .ast()
                .expressions()
                .get(expression)
                .unwrap()
                .payload(),
            Expression::Name
        ));
        self.references[&span_key(self.span(expression))]
    }

    pub(super) fn ungroup(&self, mut expression: ExpressionId) -> ExpressionId {
        while let Expression::Group { expression: inner } = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .payload()
        {
            expression = *inner;
        }
        expression
    }

    pub(super) fn declaration(&self, declaration: ItemId) -> (SymbolId, ExpressionId) {
        let (item, _) = unwrap_modified(&self.parsed, declaration).unwrap();
        let Item::Variable {
            name: NameMarker::Present(name),
            initializer,
            ..
        } = item
        else {
            panic!("ordinary named local")
        };
        let symbols = self
            .names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == name && symbol.kind() == SymbolKind::Variable)
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), 1);
        (symbols[0].id(), initializer)
    }

    pub(super) fn prelude_bindings(
        &self,
        function: &Function,
        control: ExpressionId,
    ) -> BTreeMap<SymbolId, LoweredValue> {
        let plan = &self.plans[&function.id];
        let Statement::Block { elements } = self
            .parsed
            .ast()
            .statements()
            .get(plan.body)
            .unwrap()
            .payload()
        else {
            panic!("ordinary function block")
        };
        let mut bindings = plan.entry_bindings.clone();
        let mut found = false;
        for &statement in elements {
            let node = self.parsed.ast().statements().get(statement).unwrap();
            match node.payload() {
                Statement::Expression { expression } if *expression == control => {
                    found = true;
                    break;
                }
                Statement::LocalVariable { declaration } => {
                    let (symbol, initializer) = self.declaration(*declaration);
                    let declared = self.typed.symbol_type(symbol).unwrap();
                    assert_eq!(
                        self.typed.copyability(declared),
                        Some(Copyability::MoveOnly)
                    );
                    assert!(self.owned.formed_owned_locals().contains(&symbol));
                    let mut owner = if self.typed.map_construction(initializer).is_some() {
                        let instruction =
                            one_instruction(function, self.span(initializer), |operation| {
                                matches!(operation, Operation::MapConstruct { .. })
                            });
                        result(instruction)
                    } else {
                        // C2 source <- seed and C3 source <- input use the actual old binding.
                        let LoweredValue::Value(value) = bindings[&self.reference(initializer)]
                        else {
                            panic!("existing owned initializer")
                        };
                        value
                    };
                    // binding.rs retires every old alias before optional NullableWrap.
                    bindings.retain(|_, binding| *binding != LoweredValue::Value(owner));
                    if self.typed.expression_type(initializer) != Some(declared) {
                        let instruction = one_instruction(function, node.span(), |operation| {
                            matches!(operation, Operation::NullableWrap { owner: actual, nullable }
                                if *actual == owner && *nullable == self.type_ids[&declared])
                        });
                        owner = result(instruction);
                    }
                    assert_eq!(
                        function.entity(EntityId::Value(owner)).unwrap().ty,
                        EntityType::Value(self.type_ids[&declared])
                    );
                    bindings.insert(symbol, LoweredValue::Value(owner));
                }
                Statement::Expression { expression } => {
                    // The only pre-If calls are the two frozen C2 MapPut statements.
                    // Both rebind seed to the actual returned owner, not the construction ID.
                    let descriptor = self
                        .typed
                        .map_put(*expression)
                        .expect("bounded prelude MapPut");
                    let symbol = self.reference(descriptor.receiver());
                    let LoweredValue::Value(owner) = bindings[&symbol] else {
                        panic!("put owner")
                    };
                    let instruction = one_instruction(
                        function,
                        self.span(*expression),
                        |operation| matches!(operation, Operation::MapPut { owner: actual, .. } if *actual == owner),
                    );
                    bindings.insert(symbol, LoweredValue::Value(result(instruction)));
                }
                _ => panic!("checkpoint cannot reconstruct an extended prelude"),
            }
            // These frozen prelude points cannot discard one of the retained named owners.
            for fact in self
                .owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == DropPoint::AfterStatement(statement))
            {
                if let DropTarget::Named(symbol) = fact.target() {
                    bindings.remove(&symbol);
                }
            }
        }
        assert!(
            found,
            "target If is a top-level statement, not a pending call operand"
        );
        self.assert_closed_header(function, &plan.entry_bindings, &bindings);
        bindings
    }

    fn assert_closed_header(
        &self,
        function: &Function,
        entry: &BTreeMap<SymbolId, LoweredValue>,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
    ) {
        let owners = |bindings: &BTreeMap<SymbolId, LoweredValue>| {
            bindings
                .values()
                .map(|binding| {
                    let LoweredValue::Value(owner) = binding else {
                        panic!("the frozen prelude has only owned bindings")
                    };
                    *owner
                })
                .collect::<BTreeSet<_>>()
        };
        let mut live = owners(entry);
        let mut places = BTreeMap::new();
        let mut loans = BTreeMap::new();
        let header = function.block(function.entry_block().unwrap()).unwrap();
        let string = self.type_ids[&self.typed.types().builtin(BuiltinType::String).unwrap()];
        // This is the actual header instruction order, never arena ordinal order.
        for &id in &header.instructions {
            let instruction = function.instruction(id).unwrap();
            assert_eq!(instruction.block, header.id);
            match instruction.operation {
                Operation::MapConstruct { .. } | Operation::StringLiteral { .. } => {
                    assert!(live.insert(result(instruction)), "one real owner birth");
                }
                Operation::RootPlace { owner } => {
                    assert!(live.contains(&owner), "place follows its live owner birth");
                    let [EntityId::Place(place)] = instruction.results.as_slice() else {
                        panic!("one real clone receiver place")
                    };
                    assert!(places.insert(*place, owner).is_none());
                }
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } => {
                    let owner = places[&place];
                    assert!(live.contains(&owner));
                    assert_eq!(
                        function.entity(EntityId::Value(owner)).unwrap().ty,
                        EntityType::Value(string)
                    );
                    let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                        panic!("one actual String clone loan")
                    };
                    assert_eq!(
                        function.entity(EntityId::Loan(*loan)).unwrap().ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: string
                        }
                    );
                    assert!(
                        loans.insert(*loan, owner).is_none(),
                        "loan birth follows its place"
                    );
                }
                Operation::StringClone { source } => {
                    let owner = loans[&source];
                    assert!(
                        live.contains(&owner),
                        "clone reads a presently live loan target"
                    );
                    assert_eq!(
                        function
                            .entity(EntityId::Value(result(instruction)))
                            .unwrap()
                            .ty,
                        EntityType::Value(string)
                    );
                    assert!(live.insert(result(instruction)));
                }
                Operation::BorrowEnd { loan } => {
                    assert!(
                        loans.remove(&loan).is_some(),
                        "End follows birth exactly once"
                    );
                    assert!(instruction.results.is_empty());
                }
                Operation::Drop { owner } => {
                    retire_owner(&mut live, &loans, owner);
                    assert!(instruction.results.is_empty());
                }
                Operation::MapPut { owner, key, value } => {
                    assert_eq!(self.id, "C2", "only the frozen String prelude has puts");
                    for string_owner in [key, value] {
                        assert_eq!(
                            function.entity(EntityId::Value(string_owner)).unwrap().ty,
                            EntityType::Value(string)
                        );
                    }
                    for moved in [owner, key, value] {
                        retire_owner(&mut live, &loans, moved);
                    }
                    assert!(
                        live.insert(result(instruction)),
                        "put publishes its real replacement owner"
                    );
                }
                Operation::NullableWrap { owner, .. } => {
                    retire_owner(&mut live, &loans, owner);
                    assert!(live.insert(result(instruction)));
                }
                _ => panic!(
                    "unaccounted operation in fixed six-source header: {:?}",
                    instruction.operation
                ),
            }
        }
        assert!(
            loans.is_empty(),
            "no created clone loan survives into the If"
        );
        assert_eq!(
            live,
            owners(bindings),
            "every literal/clone temporary was delivered or dropped"
        );
        // AST replay permits completed MapPut/local statements only. MapPut restores
        // its pending operands then forgets delivered K/V; local Move forgets every
        // initializer alias. The closed owner/loan account independently proves that
        // installing empty temporary/pending/borrow state loses no live carrier.
    }
}

pub(super) fn one_instruction(
    function: &Function,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> &crate::ssa::model::Instruction {
    let matches = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span) && predicate(&instruction.operation)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "one instruction with exact source and operation"
    );
    matches[0]
}

pub(super) fn result(instruction: &crate::ssa::model::Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("one actual result")
    };
    *value
}

fn retire_owner(live: &mut BTreeSet<ValueId>, loans: &BTreeMap<LoanId, ValueId>, owner: ValueId) {
    assert!(
        loans.values().all(|&target| target != owner),
        "owner delivery follows all loan ends"
    );
    assert!(
        live.remove(&owner),
        "consume exactly one already-produced owner"
    );
}
