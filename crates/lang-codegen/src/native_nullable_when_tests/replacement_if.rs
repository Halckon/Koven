//! Complete ordinary replacement + fresh If proofs, never a reused When generation.
use super::*;
use lang_frontend::{
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
    },
    parser::{BinaryOperator, LiteralKind},
    type_checking::ParameterMode,
};

const ORIGINAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit { var x = input; when (x) { null -> {}; else -> { x = replacement; if (x == null) {} else { observe(x) } } } }
"#;

const NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (((x) != null)) { observe((x)) } else {}
        }
    }
}
"#;

const REVERSED_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if ((null) == (x)) {} else { observe((x)) }
        }
    }
}
"#;

const REVERSED_NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (((null) != ((x)))) { observe(((x))) } else {}
        }
    }
}
"#;

const NATIVE: &str = r#"class Node {}
fun observe(value: Node): Unit { println("observe") }
fun equal(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> { println("old-null") };
        else -> {
            println("old-present")
            x = replacement
            if (x == null) {} else { observe(x) }
            println("after-if")
        }
    }
    println("after-when")
}
fun notEqual(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> { println("old-null") };
        else -> {
            println("old-present")
            x = replacement
            if (((x) != null)) { observe((x)) } else {}
            println("after-if")
        }
    }
    println("after-when")
}
fun reversedEqual(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> { println("old-null") };
        else -> {
            println("old-present")
            x = replacement
            if ((null) == (x)) {} else { observe((x)) }
            println("after-if")
        }
    }
    println("after-when")
}
fun reversedNotEqual(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> { println("old-null") };
        else -> {
            println("old-present")
            x = replacement
            if (((null) != ((x)))) { observe(((x))) } else {}
            println("after-if")
        }
    }
    println("after-when")
}
fun entry(): Unit {
    println("eq-nn")
    equal(null, null)
    println("eq-np")
    val eqNP: Node? = Node()
    equal(null, eqNP)
    println("eq-pn")
    val eqPN: Node? = Node()
    equal(eqPN, null)
    println("eq-pp")
    val eqPPOld: Node? = Node()
    val eqPPNew: Node? = Node()
    equal(eqPPOld, eqPPNew)
    println("ne-nn")
    notEqual(null, null)
    println("ne-np")
    val neNP: Node? = Node()
    notEqual(null, neNP)
    println("ne-pn")
    val nePN: Node? = Node()
    notEqual(nePN, null)
    println("ne-pp")
    val nePPOld: Node? = Node()
    val nePPNew: Node? = Node()
    notEqual(nePPOld, nePPNew)
    println("rev-eq-nn")
    reversedEqual(null, null)
    println("rev-eq-np")
    val revEqNP: Node? = Node()
    reversedEqual(null, revEqNP)
    println("rev-eq-pn")
    val revEqPN: Node? = Node()
    reversedEqual(revEqPN, null)
    println("rev-eq-pp")
    val revEqPPOld: Node? = Node()
    val revEqPPNew: Node? = Node()
    reversedEqual(revEqPPOld, revEqPPNew)
    println("rev-ne-nn")
    reversedNotEqual(null, null)
    println("rev-ne-np")
    val revNeNP: Node? = Node()
    reversedNotEqual(null, revNeNP)
    println("rev-ne-pn")
    val revNePN: Node? = Node()
    reversedNotEqual(revNePN, null)
    println("rev-ne-pp")
    val revNePPOld: Node? = Node()
    val revNePPNew: Node? = Node()
    reversedNotEqual(revNePPOld, revNePPNew)
    println("done")
}
"#;

const NATIVE_STDOUT: &[u8] = concat!(
    "eq-nn\nold-null\nafter-when\n",
    "eq-np\nold-null\nafter-when\n",
    "eq-pn\nold-present\nafter-if\nafter-when\n",
    "eq-pp\nold-present\nobserve\nafter-if\nafter-when\n",
    "ne-nn\nold-null\nafter-when\n",
    "ne-np\nold-null\nafter-when\n",
    "ne-pn\nold-present\nafter-if\nafter-when\n",
    "ne-pp\nold-present\nobserve\nafter-if\nafter-when\n",
    "rev-eq-nn\nold-null\nafter-when\n",
    "rev-eq-np\nold-null\nafter-when\n",
    "rev-eq-pn\nold-present\nafter-if\nafter-when\n",
    "rev-eq-pp\nold-present\nobserve\nafter-if\nafter-when\n",
    "rev-ne-nn\nold-null\nafter-when\n",
    "rev-ne-np\nold-null\nafter-when\n",
    "rev-ne-pn\nold-present\nafter-if\nafter-when\n",
    "rev-ne-pp\nold-present\nobserve\nafter-if\nafter-when\n",
    "done\n",
)
.as_bytes();

#[derive(Clone, Copy)]
struct Shape {
    non_null_when_true: bool,
    reversed: bool,
    argument: &'static str,
}

const SHAPES: [Shape; 4] = [
    Shape {
        non_null_when_true: false,
        reversed: false,
        argument: "x",
    },
    Shape {
        non_null_when_true: true,
        reversed: false,
        argument: "(x)",
    },
    Shape {
        non_null_when_true: false,
        reversed: true,
        argument: "(x)",
    },
    Shape {
        non_null_when_true: true,
        reversed: true,
        argument: "((x))",
    },
];

struct IfSource {
    id: UnitExpressionId,
    span: Span,
    condition: UnitExpressionId,
    comparison: UnitExpressionId,
    subject: UnitExpressionId,
    present: Span,
    null: Span,
    call: UnitExpressionId,
    argument: UnitExpressionId,
    null_index: usize,
}

fn leaf(source: &UnitSource, mut id: UnitExpressionId) -> UnitExpressionId {
    loop {
        let node = source.parsed.ast().expressions().get(id.expression()).unwrap();
        match node.payload() {
            Expression::Group { expression } => {
                id = UnitExpressionId::new(id.source_unit(), *expression);
            }
            _ => return id,
        }
    }
}

fn symbol_span(source: &UnitSource, symbol: UnitSymbolId) -> Span {
    source.names.names().source_units()[symbol.source_unit().index()]
        .resolution()
        .symbols()[symbol.symbol().index()]
        .span()
}

fn actual_if(source: &UnitSource, outer: &Control, shape: Shape) -> IfSource {
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            contains(outer.entries[1].1, node.span())
                && matches!(node.payload(), Expression::If { .. })
        })
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one actual nested If in the original non-null When arm")
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = *node.payload()
    else {
        panic!("the exact ordinary statement If has two actual arms")
    };
    let unit = outer.id.source_unit();
    let condition = UnitExpressionId::new(unit, condition);
    let comparison = leaf(source, condition);
    let binary = source.parsed.ast().expressions().get(comparison.expression()).unwrap();
    let Expression::Binary { left, operator, right, .. } = *binary.payload() else {
        panic!("real equality comparison, not inferred from text")
    };
    let expected_operator = if shape.non_null_when_true {
        BinaryOperator::NotEqual
    } else {
        BinaryOperator::Equal
    };
    assert_eq!(operator, expected_operator);
    let (subject, null) = if shape.reversed {
        (right, left)
    } else {
        (left, right)
    };
    let subject = UnitExpressionId::new(unit, subject);
    let null = leaf(source, UnitExpressionId::new(unit, null));
    let null = source.parsed.ast().expressions().get(null.expression()).unwrap();
    assert!(matches!(null.payload(), Expression::Literal(LiteralKind::Null)));
    assert_eq!(root(source, subject), root(source, outer.subject));
    let (present, null) = if shape.non_null_when_true {
        (then_branch, else_branch)
    } else {
        (else_branch, then_branch)
    };
    let present = source.parsed.ast().statements().get(present).unwrap();
    let null = source.parsed.ast().statements().get(null).unwrap();
    assert!(matches!(null.payload(), Statement::ControlBody { elements } if elements.is_empty()));
    let Statement::ControlBody { elements } = present.payload() else {
        panic!("one actual control body")
    };
    let [statement] = elements.as_slice() else {
        panic!("only one direct synchronous Borrow call, no nested control or assignment")
    };
    let statement = source.parsed.ast().statements().get(*statement).unwrap();
    let Statement::Expression { expression: call } = *statement.payload() else {
        panic!("real call statement")
    };
    let call = UnitExpressionId::new(unit, call);
    let call_node = source.parsed.ast().expressions().get(call.expression()).unwrap();
    let Expression::Call { arguments, .. } = call_node.payload() else {
        panic!("actual ordinary call")
    };
    let [argument] = arguments.as_slice() else {
        panic!("one Borrow argument")
    };
    let argument = UnitExpressionId::new(unit, argument.value);
    assert_eq!(source.sources.slice(span(source, argument)), Ok(shape.argument));
    assert_eq!(root(source, argument), root(source, subject));
    IfSource {
        id: UnitExpressionId::new(unit, *id),
        span: node.span(),
        condition,
        comparison,
        subject,
        present: present.span(),
        null: null.span(),
        call,
        argument,
        null_index: usize::from(shape.non_null_when_true),
    }
}

fn assert_products(source: &UnitSource, outer: &Control, inner: &IfSource, shape: Shape) {
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("real Basic Unit")
    };
    let typed = typed.types();
    let owned = owned.ownership();
    let symbol = root(source, outer.subject);
    let descriptor = typed.nullable_when(outer.id).expect("original real When descriptor");
    assert_eq!(descriptor.expression(), outer.id);
    assert_eq!(descriptor.subject(), outer.subject);
    assert_eq!(descriptor.subject_reference(), Some(outer.subject));
    assert_eq!(descriptor.stable_symbol(), Some(symbol));
    assert!(descriptor.source_generation().is_some());
    let plan = owned.nullable_when(outer.id).expect("same-analysis When plan");
    assert_eq!(plan.descriptor(), descriptor);
    assert!(plan.uses().is_empty(), "replacement invalidates every old use witness");
    let place = plan.source_place().unwrap();
    assert!(place.is_root());
    assert_eq!(place.root(), symbol);
    let comparison = typed.null_comparison(inner.comparison).expect("fresh actual If comparison");
    assert_eq!(comparison.expression(), inner.comparison);
    assert_eq!(comparison.symbol(), symbol);
    assert_eq!(comparison.nullable_type(), descriptor.nullable_type());
    assert_eq!(comparison.non_null_when_true(), shape.non_null_when_true);
    assert_eq!(Some(comparison.nullable_type()), typed.symbol_type(symbol));
    assert_eq!(typed.expression_type(inner.subject), Some(comparison.nullable_type()));
    assert!(typed.non_null_use(inner.subject).is_none());
    assert!(contains(outer.entries[1].1, inner.span));
    assert!(contains(inner.present, span(source, inner.call)));
    assert!(contains(inner.span, span(source, inner.condition)));
    let actual = typed.call(inner.call).expect("actual source call contract");
    let observe = source.declaration("observe", SymbolKind::Function);
    assert_eq!(actual.target(), UnitCallTarget::Declaration(observe));
    assert!(actual.receiver().is_none());
    let [mapping] = actual.arguments() else {
        panic!("one source argument")
    };
    assert_eq!(mapping.mode(), ParameterMode::Borrow);
    assert_eq!(mapping.argument_index(), 0);
    let mut current = inner.argument;
    loop {
        let proof = typed.non_null_use(current).expect("fresh real If non-null use");
        assert_eq!(proof.expression(), current);
        assert_eq!(proof.symbol(), symbol);
        assert_eq!(proof.declared_type(), comparison.nullable_type());
        assert_eq!(proof.narrowed_type(), descriptor.inner_type());
        assert_eq!(
            typed.types().get(proof.declared_type()),
            Some(&UnitTypeKind::Nullable(proof.narrowed_type()))
        );
        assert!(typed.nullable_when_use(current).is_none(), "never revive the old generation");
        assert!(owned.nullable_when_use(current).is_none());
        let node = source.parsed.ast().expressions().get(current.expression()).unwrap();
        match node.payload() {
            Expression::Group { expression } => {
                current = UnitExpressionId::new(current.source_unit(), *expression);
            }
            Expression::Name => break,
            _ => panic!("only real Name/Group roots"),
        }
    }
    let contracts = owned.call_argument_contracts_for(inner.call).collect::<Vec<_>>();
    let [contract] = contracts.as_slice() else {
        panic!("one exact original ownership contract")
    };
    assert_eq!(contract.argument(), inner.argument);
    assert_eq!(contract.kind(), UnitCallArgumentOwnershipKind::SharedLoan);
    assert_eq!(contract.parameter_type(), mapping.parameter_type());
    assert_eq!(contract.parameter_index(), mapping.parameter_index());
    assert_eq!(contract.argument_span(), span(source, inner.argument));
    assert_eq!(contract.call_span(), span(source, inner.call));
    assert!(!contract.crosses_thread());
    let loans = owned.loans().iter().filter(|fact| fact.call() == inner.call).collect::<Vec<_>>();
    let [loan] = loans.as_slice() else {
        panic!("one real owned Shared loan")
    };
    assert_eq!(loan.argument(), inner.argument);
    assert_eq!(loan.kind(), FrontendLoanKind::Shared);
    assert_eq!(loan.begin_span(), span(source, inner.argument));
    assert_eq!(loan.end_span(), span(source, inner.call));
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("whole current root")
    };
    assert!(place.is_root());
    assert_eq!(place.root(), symbol);
    assert!(!owned.value_deliveries().iter().any(|fact| fact.call() == inner.call));
    let (assignment, assignment_span) = expression(source, outer.span, "x = replacement");
    let assignment = typed.assignment(assignment).expect("actual replacement storage fact");
    assert_eq!(root(source, assignment.target()), symbol);
    assert_eq!(Some(assignment.target_type()), typed.symbol_type(symbol));
    assert!(assignment.falls_through());
    let replacement = root(source, assignment.value());
    assert_ne!(replacement, symbol);
    let old_origin = symbol_span(source, symbol);
    let unused_origin = symbol_span(source, replacement);
    assert_eq!(source.sources.slice(old_origin), Ok("x"));
    assert_eq!(source.sources.slice(unused_origin), Ok("replacement"));
    let targets = [UnitDropTarget::Named(symbol), UnitDropTarget::Named(replacement)];
    let actual = owned.drops().iter().filter(|fact| targets.contains(&fact.target()))
        .map(|fact| (fact.target(), fact.point(), fact.value_origin())).collect::<Vec<_>>();
    let expected = [
        (targets[0], UnitDropPoint::AfterExpression(outer.subject), old_origin),
        (targets[1], UnitDropPoint::BranchExit { control: outer.id, branch: 0 }, unused_origin),
        (targets[0], UnitDropPoint::BranchExit {
            control: inner.id,
            branch: inner.null_index,
        }, assignment_span),
        (targets[0], UnitDropPoint::CallReturn(inner.call), assignment_span),
    ];
    assert_eq!(actual.len(), expected.len(), "no extra generation cleanup obligations");
    for expected in expected {
        assert_eq!(
            actual.iter().filter(|fact| **fact == expected).count(),
            1,
            "one exact source-qualified drop fact: {expected:?}"
        );
    }
}

fn if_birth<'a>(function: &'a Function, control: &IfSource) -> Birth<'a> {
    let headers = function.blocks.iter().filter(|block| {
        block.terminator.as_ref().is_some_and(|term| {
            term.origin == Origin::Source(control.span)
                && matches!(term.kind, TerminatorKind::NullableBranch { .. })
        })
    }).collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        panic!("one real source If NullableBranch")
    };
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    let null = function.block(when_null.target).unwrap();
    let present = function.block(when_non_null.target).unwrap();
    assert_eq!(null.origin, Origin::Source(control.null));
    assert_eq!(present.origin, Origin::Source(control.present));
    assert_ne!(null.id, present.id);
    assert_eq!(when_null.arguments, when_non_null.arguments);
    assert_eq!(
        when_null.arguments.iter().filter(|id| **id == EntityId::Value(*owner)).count(),
        1
    );
    assert_eq!(present.parameters.last(), Some(&EntityId::Loan(*view)));
    let incoming = function.blocks.iter().filter(|block| {
        block.terminator.as_ref().unwrap().kind.targets().contains(&present.id)
    }).map(|block| block.id).collect::<Vec<_>>();
    assert_eq!(incoming, [header.id], "only the actual non-null edge births this proof");
    Birth {
        header: header.id,
        owner: *owner,
        null: when_null,
        present: when_non_null,
        view: *view,
        owners: aliases(function, EntityId::Value(*owner)),
        views: aliases(function, EntityId::Loan(*view)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Branch(usize, bool),
    End(usize),
    Drop(usize),
    Observe,
}

struct Paths<'a> {
    function: &'a Function,
    births: [Birth<'a>; 2],
    controls: [Span; 2],
    call: Span,
    observe: FunctionId,
    old_origin: Span,
    replacement_origin: Span,
    unused_origin: Span,
}

impl Paths<'_> {
    fn visit(
        &self,
        id: BlockId,
        mut seen: BTreeSet<BlockId>,
        mut events: Vec<Event>,
    ) -> Vec<Vec<Event>> {
        assert!(seen.insert(id), "these complete sources have no loop or repeated visit");
        let block = self.function.block(id).unwrap();
        for id in &block.instructions {
            let instruction = self.function.instruction(*id).unwrap();
            match &instruction.operation {
                Operation::DirectCall { callee, receiver, arguments } => {
                    assert_eq!(*callee, self.observe);
                    assert!(receiver.is_none());
                    assert_eq!(instruction.origin, Origin::Source(self.call));
                    assert_eq!(arguments, &[EntityId::Loan(self.births[1].view)]);
                    assert_eq!(instruction.block, self.births[1].present.target);
                    events.push(Event::Observe);
                }
                Operation::BorrowEnd { loan } => {
                    let found = self.births.iter().enumerate().filter(|(_, birth)| {
                        birth.views.contains(&EntityId::Loan(*loan))
                    }).map(|(i, _)| i).collect::<Vec<_>>();
                    let [index] = found.as_slice() else {
                        panic!("end exactly one real scoped view")
                    };
                    assert!(!events.contains(&Event::End(*index)));
                    assert!(!events.contains(&Event::Drop(*index)));
                    let origin = if *index == 0 {
                        self.old_origin
                    } else {
                        self.replacement_origin
                    };
                    assert_eq!(instruction.origin, Origin::Source(origin));
                    events.push(Event::End(*index));
                }
                Operation::Drop { owner } => {
                    let found = self.births.iter().enumerate().filter(|(_, birth)| {
                        birth.owners.contains(&EntityId::Value(*owner))
                    }).map(|(i, _)| i).collect::<Vec<_>>();
                    let [index] = found.as_slice() else {
                        panic!("drop exactly one actual owner generation")
                    };
                    assert!(!events.contains(&Event::Drop(*index)));
                    let origin = if *index == 0 {
                        self.old_origin
                    } else if events.contains(&Event::Branch(0, true)) {
                        self.replacement_origin
                    } else {
                        self.unused_origin
                    };
                    assert_eq!(instruction.origin, Origin::Source(origin));
                    events.push(Event::Drop(*index));
                }
                Operation::RootPlace { .. }
                | Operation::BorrowBegin { .. }
                | Operation::NullableTake { .. }
                | Operation::SharedRetain { .. } => {
                    panic!("read-only proofs never reinterpret, reborrow, take or retain")
                }
                _ => {}
            }
        }
        let term = block.terminator.as_ref().unwrap();
        match &term.kind {
            TerminatorKind::NullableBranch { owner, when_null, when_non_null, view } => {
                let index = self.births.iter().position(|birth| birth.header == id)
                    .expect("only the two actual source-qualified proof headers");
                let birth = &self.births[index];
                assert_eq!(term.origin, Origin::Source(self.controls[index]));
                assert_eq!((*owner, *view), (birth.owner, birth.view));
                assert_eq!((when_null, when_non_null), (birth.null, birth.present));
                if index == 1 {
                    assert_eq!(events, [Event::Branch(0, true), Event::End(0), Event::Drop(0)]);
                }
                let mut paths = Vec::new();
                for (present, edge) in [(false, when_null), (true, when_non_null)] {
                    let mut next = events.clone();
                    next.push(Event::Branch(index, present));
                    paths.extend(self.visit(edge.target, seen.clone(), next));
                }
                paths
            }
            TerminatorKind::Branch(edge) => {
                assert!(self.controls.iter().any(|origin| term.origin == Origin::Source(*origin)),
                    "actual arm/join edges, not invented branch origins");
                for (index, birth) in self.births.iter().enumerate() {
                    if events.contains(&Event::Drop(index)) {
                        assert!(edge.arguments.iter().all(|entity| {
                            !birth.owners.contains(entity) && !birth.views.contains(entity)
                        }));
                    }
                }
                self.visit(edge.target, seen, events)
            }
            TerminatorKind::Return { values } => {
                assert_eq!(term.origin, self.function.origin);
                assert!(values.is_empty());
                vec![events]
            }
            _ => panic!("the complete source has no Boolean fallback, loop, escape or carrier"),
        }
    }
}

fn assert_structure(source: &UnitSource, program: &Program, name: &str, shape: Shape) {
    let controls = controls(source, name);
    let [outer] = controls.as_slice() else {
        panic!("one real outer When")
    };
    let inner = actual_if(source, outer, shape);
    assert_products(source, outer, &inner, shape);
    let function = function(source, program, name);
    let births = [birth(function, outer), if_birth(function, &inner)];
    assert_ne!(births[0].header, births[1].header);
    assert_ne!(births[0].owner, births[1].owner);
    assert_ne!(births[0].view, births[1].view);
    assert!(births[0].owners.is_disjoint(&births[1].owners));
    assert!(births[0].views.is_disjoint(&births[1].views));
    let parameters = &function.block(function.entry_block().unwrap()).unwrap().parameters;
    let [input @ EntityId::Value(_), replacement @ EntityId::Value(_)] = parameters.as_slice()
    else {
        panic!("two real owned nullable parameters in source declaration order")
    };
    assert_eq!(births[0].owners, aliases(function, *input));
    assert_eq!(births[1].owners, aliases(function, *replacement));
    let module = program.module(function.id.module()).unwrap();
    for birth in &births {
        let owner = function.entity(EntityId::Value(birth.owner)).unwrap();
        let storage = owner.ty.semantic_type();
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(storage) else {
            panic!("the actual owner storage remains nullable")
        };
        assert!(matches!(module.type_kind(*inner), Some(SsaTypeKind::HeapOwner { .. })));
        assert_eq!(
            function.entity(EntityId::Loan(birth.view)).unwrap().ty,
            EntityType::Loan { kind: LoanKind::Shared, target: *inner }
        );
        let transported = branch_owner(function, birth.present, birth.owner);
        assert_eq!(
            function.entity(EntityId::Value(transported)).unwrap().ty,
            EntityType::Value(storage)
        );
    }
    let OwnedProducts::Basic(typed, _) = &source.products else {
        unreachable!()
    };
    let assignment = expression(source, outer.span, "x = replacement");
    let replacement = root(source, typed.types().assignment(assignment.0).unwrap().value());
    let paths = Paths {
        function,
        births,
        controls: [outer.span, inner.span],
        call: span(source, inner.call),
        observe: self::function(source, program, "observe").id,
        old_origin: symbol_span(source, root(source, outer.subject)),
        replacement_origin: assignment.1,
        unused_origin: symbol_span(source, replacement),
    };
    use Event::*;
    let actual = paths.visit(function.entry_block().unwrap(), BTreeSet::new(), Vec::new());
    assert_eq!(
        actual,
        [
            vec![Branch(0, false), Drop(0), Drop(1)],
            vec![Branch(0, true), End(0), Drop(0), Branch(1, false), Drop(1)],
            vec![
                Branch(0, true),
                End(0),
                Drop(0),
                Branch(1, true),
                Observe,
                End(1),
                Drop(1),
            ],
        ],
        "all three real CFG paths, one exact retirement/drop per live generation"
    );
}

fn checked(name: &str, text: &str) -> UnitSource {
    let source = UnitSource::analyze(name, text)
        .unwrap_or_else(|error| panic!("{name}: unexpected earlier source rejection: {error:?}"));
    source.assert_frontend_clean();
    assert!(matches!(&source.products, OwnedProducts::Basic(..)));
    assert_eq!(source.sources.source_text(source.parsed.source_id()), Ok(text));
    assert_eq!(source.sources.source_name(source.parsed.source_id()), Ok(name));
    source
}

fn assert_complete(name: &str, text: &str, shape: Shape) {
    let source = checked(name, text);
    // A real old-production InvalidSsa is a failure here, never a passing red oracle.
    // Earlier source failures or missing API errors cannot be counted as that behavior.
    let (program, entry) = source
        .lower_entry(source.declaration("entry", SymbolKind::Function))
        .unwrap_or_else(|error| panic!("{name}: legal complete source must lower: {error:?}"));
    assert_eq!(entry, function(&source, &program, "entry").id);
    verify_program(&program).expect("actual complete Unit SSA verifies");
    assert_structure(&source, &program, "entry", shape);
    let llvm = crate::llvm::render_verified_program(&program)
        .expect("verified argument-bearing Unit reaches LLVM without a fabricated main");
    assert!(llvm.contains("define "));
}

pub(super) fn assert_original_replacement(fixture: &sources::Negative) {
    assert_eq!(sources::NEGATIVES.len(), 17);
    assert_eq!(sources::NEGATIVES[11].name, "replaced-generation");
    let matching = sources::NEGATIVES.iter().filter(|row| row.name == fixture.name).count();
    assert_eq!(matching, 1);
    assert_eq!(fixture.text, ORIGINAL, "original #12 complete source is byte-exact");
    assert_eq!(fixture.text.len(), 217);
    assert_complete(fixture.name, fixture.text, SHAPES[0]);
}

#[test]
fn nullable_when_replacement_if_uses_current_owner_and_exact_cleanup() {
    for (index, (name, text)) in [
        ("replacement-if-original.ko", ORIGINAL),
        ("replacement-if-not-equal.ko", NOT_EQUAL),
        ("replacement-if-reversed-equal.ko", REVERSED_EQUAL),
        ("replacement-if-reversed-not-equal.ko", REVERSED_NOT_EQUAL),
    ]
    .into_iter()
    .enumerate()
    {
        assert_complete(name, text, SHAPES[index]);
    }
}

#[test]
fn nullable_when_replacement_if_ordinary_source_links_and_runs() {
    use std::{fs, process::Command};
    let source = checked("replacement-if-native.ko", NATIVE);
    let declaration = source.declaration("entry", SymbolKind::Function);
    let (program, entry) = source
        .lower_entry(declaration)
        .expect("independent complete ordinary native source lowers before native work");
    verify_program(&program).expect("actual complete native Unit SSA verifies");
    // This separate full source includes ordinary println String loans. The exact
    // two-generation cleanup walker belongs only to the four quiet source fixtures.
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("the real no-argument native entry reaches LLVM");
    assert!(llvm.contains("define i32 @main("));
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("nullable-replacement-if.o");
    let executable = directory.join("nullable-replacement-if");
    source.emit(declaration, &object).expect("the same real Basic Unit emits its object");
    let bytes = fs::read(&object).expect("actual emitted object bytes");
    assert!(bytes.len() >= 20);
    crate::test_support::assert_native_object(&bytes);
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("existing native linker launches");
    assert_eq!(linked.status.code(), Some(0), "{linked:?}");
    assert!(linked.stdout.is_empty(), "{linked:?}");
    assert!(linked.stderr.is_empty(), "{linked:?}");
    let run = Command::new(&executable).output().expect("new ordinary executable launches");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_eq!(run.stdout, NATIVE_STDOUT, "full independently source-authored stdout");
}
