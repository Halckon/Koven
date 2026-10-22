//! Ordinary Basic-Unit direct Declaration prefix transactions, without synthetic products.
//! Every fixture is a new whole source. No nullable, constant, range, or fault path is selected.

use super::{OwnedProducts, UnitSource};
use crate::{
    native_tests::emit_link_and_run,
    ssa::{
        LoweringErrorKind,
        model::{BlockId, Edge, EntityId, Function, Operation, Origin, Program, TerminatorKind},
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
    },
    source::Span,
    type_checking::{CallableResultSource, Copyability, ParameterMode, UnitCallTarget, UnitExpressionId},
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prefix {
    TemporaryBorrow,
    NamedValue,
    IncomingBorrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    Return,
    Break,
    Continue,
    Commit,
}

impl Exit {
    fn expression(self) -> &'static str {
        match self {
            Self::Return => "return",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Commit => "7",
        }
    }
}

struct Fixture {
    name: String,
    text: String,
    call: String,
    outer_call: String,
    control: String,
    prefix: Prefix,
    exit: Exit,
}

fn fixtures() -> Vec<Fixture> {
    let mut result = Vec::new();
    for prefix in [
        Prefix::TemporaryBorrow,
        Prefix::NamedValue,
        Prefix::IncomingBorrow,
    ] {
        for exit in [Exit::Return, Exit::Break, Exit::Continue, Exit::Commit] {
            let (mode, parameters, binding, argument, invoke) = match prefix {
                Prefix::TemporaryBorrow => (
                    "", "own flag: Boolean", "", "Leaf()", "exercise(flag)",
                ),
                Prefix::NamedValue => (
                    "own ", "own flag: Boolean", "val prefix = Leaf()", "prefix", "exercise(flag)",
                ),
                Prefix::IncomingBorrow => (
                    "", "prefix: Leaf, own flag: Boolean", "", "prefix",
                    "val callerOwner = Leaf(); exercise(callerOwner, flag)",
                ),
            };
            let control = format!(
                "when (flag) {{ true -> {{ {} }} else -> {{ 8 }} }}", exit.expression()
            );
            let call = format!("accept({argument}, {control})");
            let outer_call = format!("enclose(Outer(), {call})");
            let body = format!("{binding}\nval result = {outer_call}\ncheckpoint()");
            // The mutable Boolean bounds continue without arithmetic, hidden abort, or a range.
            let body = if matches!(exit, Exit::Break | Exit::Continue) {
                format!("var once = true\nwhile (once) {{ once = false\n{body}\n}}")
            } else {
                body
            };
            let text = format!(
                r#"class Leaf {{ deinit() {{ println("drop") }} }}
class Outer {{ deinit() {{ println("outer-drop") }} }}
fun enclose(prefix: Outer, own number: Int): Int {{ println("outer-callee"); return number }}
fun touch(prefix: Leaf): Unit {{}}
fun accept({mode}prefix: Leaf, own number: Int): Int {{
    println("callee")
    touch(prefix)
    return number
}}
fun checkpoint(): Unit {{ println("after-call") }}
fun tail(): Unit {{ println("tail") }}
fun exercise({parameters}): Unit {{
    {body}
    tail()
}}
fun run(own flag: Boolean): Unit {{ {invoke}; println("caller") }}
fun entry(): Unit {{ println("true"); run(true); println("false"); run(false); println("done") }}
"#
            );
            result.push(Fixture {
                name: format!("direct-prefix-{prefix:?}-{exit:?}.ko"),
                text,
                call,
                outer_call,
                control,
                prefix,
                exit,
            });
        }
    }
    result
}

fn item_span(source: &UnitSource, name: &str) -> Span {
    let id = source.declaration(name, SymbolKind::Function);
    let declaration = &source.names.names().index().declarations()[id.index()];
    source.parsed.ast().items().get(declaration.root()).unwrap().span()
}

fn expression(source: &UnitSource, within: Span, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one original MemoryDocument")
    };
    let found = source.parsed.ast().expressions().iter().filter(|(_, node)| {
        let span = node.span();
        within.start() <= span.start() && span.end() <= within.end()
            && source.sources.slice(span) == Ok(text)
    }).collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one real expression {text:?} in the selected source function")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn function<'a>(source: &UnitSource, program: &'a Program, name: &str) -> &'a Function {
    let origin = Origin::Source(item_span(source, name));
    let found = program.modules.iter().flat_map(|module| &module.functions)
        .filter(|function| function.origin == origin).collect::<Vec<_>>();
    let [function] = found.as_slice() else {
        panic!("one source-qualified function {name}")
    };
    function
}

fn assert_products(source: &UnitSource, fixture: &Fixture) {
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("the new ordinary source must use Basic capability, never a constant fallback")
    };
    let typed = typed.types();
    let owned = owned.ownership();
    let scope = item_span(source, "exercise");
    let (call, call_span) = expression(source, scope, &fixture.call);
    let argument_text = if fixture.prefix == Prefix::TemporaryBorrow {
        "Leaf()"
    } else {
        "prefix"
    };
    let (argument, argument_span) = expression(source, scope, argument_text);
    let descriptor = typed.call(call).expect("real typed direct call");
    assert_eq!(
        descriptor.target(),
        UnitCallTarget::Declaration(source.declaration("accept", SymbolKind::Function))
    );
    assert!(descriptor.receiver().is_none());
    assert_eq!(descriptor.result_source(), CallableResultSource::Owned);
    assert!(!descriptor.aborts() && !descriptor.prints_line());
    assert!(typed.ownership_primitive(call).is_none());
    assert_eq!(descriptor.arguments().len(), 2);
    let (outer_call, outer_span) = expression(source, scope, &fixture.outer_call);
    let (outer_argument, outer_origin) = expression(source, scope, "Outer()");
    let outer_descriptor = typed.call(outer_call).expect("real enclosing call descriptor");
    assert_eq!(
        outer_descriptor.target(),
        UnitCallTarget::Declaration(source.declaration("enclose", SymbolKind::Function))
    );
    assert!(outer_descriptor.receiver().is_none());
    assert_eq!(outer_descriptor.result_source(), CallableResultSource::Owned);
    let outer_loans = owned.loans().iter().filter(|loan| {
        loan.call() == outer_call && loan.argument() == outer_argument
    }).collect::<Vec<_>>();
    let [outer_loan] = outer_loans.as_slice() else {
        panic!("one real enclosing Borrow temporary")
    };
    assert_eq!(outer_loan.target(), &UnitLoanTarget::Temporary(outer_argument));
    assert_eq!(outer_loan.kind(), FrontendLoanKind::Shared);
    assert_eq!(outer_loan.begin_span(), outer_origin);
    assert_eq!(outer_loan.end_span(), outer_span);
    let mapping = &descriptor.arguments()[0];
    assert_eq!(mapping.argument_index(), 0);
    assert_eq!(mapping.parameter_index(), 0);
    assert_eq!(
        mapping.mode(),
        if fixture.prefix == Prefix::NamedValue {
            ParameterMode::Value
        } else {
            ParameterMode::Borrow
        }
    );
    assert_eq!(
        typed.copyability(typed.expression_type(argument).unwrap()),
        Copyability::MoveOnly
    );
    let contracts = owned.call_argument_contracts_for(call).collect::<Vec<_>>();
    assert_eq!(contracts.len(), 2);
    let contract = contracts.iter().find(|contract| contract.argument() == argument).unwrap();
    assert_eq!(contract.parameter_index(), 0);
    assert_eq!(contract.parameter_type(), mapping.parameter_type());
    assert_eq!(contract.category(), mapping.category());
    assert_eq!(typed.expression_category(argument), Some(mapping.category()));
    assert_eq!(contract.argument_span(), argument_span);
    assert_eq!(contract.call_span(), call_span);
    assert!(!contract.crosses_thread());
    assert_eq!(
        contract.kind(),
        if fixture.prefix == Prefix::NamedValue {
            UnitCallArgumentOwnershipKind::Value
        } else {
            UnitCallArgumentOwnershipKind::SharedLoan
        }
    );
    if fixture.prefix == Prefix::NamedValue {
        let deliveries = owned.value_deliveries().iter().filter(|fact| {
            fact.call() == call && fact.argument() == argument
        }).collect::<Vec<_>>();
        let [delivery] = deliveries.as_slice() else {
            panic!("one real evaluated Value prefix")
        };
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
        assert!(delivery.place().is_some_and(|place| place.is_root()));
    } else {
        let loans = owned.loans().iter().filter(|fact| {
            fact.call() == call && fact.argument() == argument
        }).collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            panic!("one real evaluated Borrow prefix")
        };
        assert_eq!(loan.kind(), FrontendLoanKind::Shared);
        assert_eq!(loan.begin_span(), argument_span);
        assert_eq!(loan.end_span(), call_span);
        match (fixture.prefix, loan.target()) {
            (Prefix::TemporaryBorrow, UnitLoanTarget::Temporary(owner)) => {
                assert_eq!(*owner, argument);
            }
            (Prefix::IncomingBorrow, UnitLoanTarget::Place(place)) => assert!(place.is_root()),
            _ => panic!("prefix target must remain exactly temporary or whole root"),
        }
    }
    if fixture.exit != Exit::Commit {
        let (_, control_span) = expression(source, scope, &fixture.control);
        let (exit, _) = expression(source, control_span, fixture.exit.expression());
        let outer_drops = owned.drops().iter().filter(|fact| {
            fact.point() == UnitDropPoint::ControlTransfer(exit)
                && fact.target() == UnitDropTarget::Temporary(outer_argument)
        }).collect::<Vec<_>>();
        let [outer_drop] = outer_drops.as_slice() else {
            panic!("the enclosing owner is protected until the whole argument transaction ends")
        };
        assert_eq!(outer_drop.value_origin(), outer_origin);
        let pending = owned.drops().iter().filter(|fact| {
            fact.point() == UnitDropPoint::ControlTransfer(exit)
                && fact.target() == UnitDropTarget::Temporary(argument)
        }).collect::<Vec<_>>();
        assert_eq!(
            pending.len(),
            usize::from(fixture.prefix != Prefix::IncomingBorrow)
        );
        if let [drop] = pending.as_slice() {
            assert_eq!(drop.value_origin(), argument_span);
        }
    }
}

fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional { when_true, when_false, .. } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => Vec::new(),
        _ => panic!("ordinary fixture has no nullable, borrowed return, or Abort terminator"),
    }
}

/// Only follow actual edge parameters; never fabricate an ID or equate same-typed entities.
fn transported_aliases(function: &Function, seed: EntityId) -> BTreeSet<EntityId> {
    let mut aliases = BTreeSet::from([seed]);
    loop {
        let before = aliases.len();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().unwrap().kind) {
                let target = function.block(edge.target).unwrap();
                assert_eq!(edge.arguments.len(), target.parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    if aliases.contains(argument) { aliases.insert(*parameter); }
                }
            }
        }
        if aliases.len() == before {
            // A join is the same source owner/loan only if every incoming edge agrees.
            for target in &function.blocks {
                for (index, parameter) in target.parameters.iter().enumerate() {
                    if !aliases.contains(parameter) || *parameter == seed { continue; }
                    let incoming = function.blocks.iter().flat_map(|block| {
                        edges(&block.terminator.as_ref().unwrap().kind)
                    }).filter(|edge| edge.target == target.id).collect::<Vec<_>>();
                    assert!(!incoming.is_empty(), "a transported alias has a real predecessor");
                    assert!(incoming.iter().all(|edge| aliases.contains(&edge.arguments[index])),
                        "every incoming edge must preserve the same actual owner/loan identity");
                }
            }
            return aliases;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Commit,
    End,
    Drop,
    OuterCommit,
    OuterEnd,
    OuterDrop,
}

struct Transaction<'a> {
    function: &'a Function,
    fixture: &'a Fixture,
    call: Span,
    outer_call: Span,
    checkpoint: Span,
    exit: Option<Span>,
    owners: BTreeSet<EntityId>,
    outer_owners: BTreeSet<EntityId>,
    outer_loans: BTreeSet<EntityId>,
    loans: BTreeSet<EntityId>,
}

impl Transaction<'_> {
    fn expected(&self, committed: bool) -> Vec<Event> {
        use Event::{Commit, Drop, End, OuterCommit, OuterDrop, OuterEnd};
        match (self.fixture.prefix, committed) {
            (Prefix::TemporaryBorrow, true) => {
                vec![Commit, End, Drop, OuterCommit, OuterEnd, OuterDrop]
            }
            (Prefix::TemporaryBorrow, false) => vec![End, OuterEnd, Drop, OuterDrop],
            (Prefix::NamedValue | Prefix::IncomingBorrow, true) => {
                vec![Commit, OuterCommit, OuterEnd, OuterDrop]
            }
            (Prefix::NamedValue, false) => vec![OuterEnd, Drop, OuterDrop],
            (Prefix::IncomingBorrow, false) => vec![OuterEnd, OuterDrop],
        }
    }

    fn visit(&self, block_id: BlockId, mut events: Vec<Event>, mut seen: Vec<BlockId>) -> Vec<bool> {
        assert!(
            !seen.contains(&block_id),
            "transaction must retire before the next loop iteration"
        );
        seen.push(block_id);
        let block = self.function.block(block_id).unwrap();
        for id in &block.instructions {
            let instruction = self.function.instruction(*id).unwrap();
            if instruction.origin == Origin::Source(self.checkpoint) {
                assert!(matches!(
                    instruction.operation,
                    Operation::DirectCall { receiver: None, .. }
                ));
                assert_eq!(
                    events, self.expected(true),
                    "{}: complete normal path {seen:?}", self.fixture.name
                );
                return vec![true];
            }
            match &instruction.operation {
                Operation::DirectCall { receiver, arguments, .. }
                    if instruction.origin == Origin::Source(self.call) =>
                {
                    assert!(receiver.is_none());
                    assert_eq!(arguments.len(), 2);
                    let aliases = if self.fixture.prefix == Prefix::NamedValue {
                        &self.owners
                    } else {
                        &self.loans
                    };
                    assert!(aliases.contains(&arguments[0]), "commit must use the real rebound prefix");
                    events.push(Event::Commit);
                }
                Operation::DirectCall { receiver, arguments, .. }
                    if instruction.origin == Origin::Source(self.outer_call) =>
                {
                    assert!(receiver.is_none());
                    assert!(self.outer_loans.contains(&arguments[0]));
                    events.push(Event::OuterCommit);
                }
                Operation::BorrowEnd { loan }
                    if self.outer_loans.contains(&EntityId::Loan(*loan)) =>
                {
                    events.push(Event::OuterEnd);
                }
                Operation::Drop { owner }
                    if self.outer_owners.contains(&EntityId::Value(*owner)) =>
                {
                    events.push(Event::OuterDrop);
                }
                Operation::BorrowEnd { loan } if self.loans.contains(&EntityId::Loan(*loan)) => {
                    events.push(Event::End);
                }
                Operation::Drop { owner } if self.owners.contains(&EntityId::Value(*owner)) => {
                    events.push(Event::Drop);
                }
                _ => {}
            }
        }
        let terminator = block.terminator.as_ref().unwrap();
        if self.exit.is_some_and(|span| terminator.origin == Origin::Source(span)) {
            assert_eq!(
                events, self.expected(false),
                "{}: complete cancellation path {seen:?}", self.fixture.name
            );
            match (&terminator.kind, self.fixture.exit) {
                (TerminatorKind::Return { values }, Exit::Return) => assert!(values.is_empty()),
                (TerminatorKind::Branch(_), Exit::Break | Exit::Continue) => {}
                _ => panic!("exact source transfer must have its corresponding SSA terminator"),
            }
            for edge in edges(&terminator.kind) {
                assert!(edge.arguments.iter().all(|argument| {
                    !self.owners.contains(argument) && !self.outer_owners.contains(argument)
                        && !self.outer_loans.contains(argument)
                }));
                if self.fixture.prefix != Prefix::IncomingBorrow {
                    assert!(edge.arguments.iter().all(|argument| !self.loans.contains(argument)));
                }
            }
            return vec![false];
        }
        let successors = edges(&terminator.kind);
        assert!(
            !successors.is_empty(),
            "every transaction must reach its exact commit or control exit"
        );
        successors.into_iter()
            .flat_map(|edge| self.visit(edge.target, events.clone(), seen.clone()))
            .collect()
    }
}

fn owner_and_loan_aliases(
    source: &UnitSource,
    function: &Function,
    scope: Span,
    constructor: &str,
    borrowed: bool,
) -> (BTreeSet<EntityId>, BTreeSet<EntityId>) {
    let (_, origin) = expression(source, scope, constructor);
    let births = function.instructions.iter().filter(|instruction| {
        instruction.origin == Origin::Source(origin)
            && matches!(instruction.operation, Operation::HeapAllocate { .. })
    }).collect::<Vec<_>>();
    let [birth] = births.as_slice() else {
        panic!("one actual prefix owner allocation")
    };
    let [owner @ EntityId::Value(_)] = birth.results.as_slice() else {
        panic!("one owner result")
    };
    let owners = transported_aliases(function, *owner);
    let loans = function.instructions.iter().filter_map(|instruction| {
        let Operation::BorrowBegin { place, .. } = instruction.operation else {
            return None;
        };
        let root = function.instructions.iter().any(|root| {
            root.results.contains(&EntityId::Place(place))
                && matches!(root.operation,
                    Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)))
        });
        root.then_some(instruction)
    }).collect::<Vec<_>>();
    if !borrowed {
        assert!(loans.is_empty(), "Value prefix is not disguised as a loan");
        return (owners, BTreeSet::new());
    }
    let [loan] = loans.as_slice() else {
        panic!("one newly created prefix loan")
    };
    let [loan @ EntityId::Loan(_)] = loan.results.as_slice() else {
        panic!("one loan result")
    };
    (owners, transported_aliases(function, *loan))
}

fn assert_paths(source: &UnitSource, fixture: &Fixture, program: &Program) {
    verify_program(program).expect("real whole-program SSA verifies ownership before path inspection");
    let function = function(source, program, "exercise");
    let scope = item_span(source, "exercise");
    let (_, call) = expression(source, scope, &fixture.call);
    let (_, control) = expression(source, scope, &fixture.control);
    let (_, checkpoint) = expression(source, scope, "checkpoint()");
    let exit = (fixture.exit != Exit::Commit)
        .then(|| expression(source, control, fixture.exit.expression()).1);
    let (_, outer_call) = expression(source, scope, &fixture.outer_call);
    let (outer_owners, outer_loans) = owner_and_loan_aliases(source, function, scope, "Outer()", true);
    let (owners, loans) = if fixture.prefix == Prefix::IncomingBorrow {
        let entry = function.entry_block().and_then(|id| function.block(id))
            .expect("the real lowered function has its declared entry block");
        let parameter = entry.parameters[0];
        assert!(matches!(parameter, EntityId::Loan(_)));
        (BTreeSet::new(), transported_aliases(function, parameter))
    } else {
        owner_and_loan_aliases(
            source, function, scope, "Leaf()", fixture.prefix == Prefix::TemporaryBorrow,
        )
    };
    let branches = function.blocks.iter().filter(|block| {
        block.terminator.as_ref().is_some_and(|terminator| {
            terminator.origin == Origin::Source(control)
                && matches!(terminator.kind, TerminatorKind::Conditional { .. })
        })
    }).collect::<Vec<_>>();
    let [branch] = branches.as_slice() else {
        panic!("one real Boolean-when decision")
    };
    assert!(owners.is_disjoint(&outer_owners));
    assert!(loans.is_disjoint(&outer_loans));
    let transaction = Transaction {
        function, fixture, call, outer_call, checkpoint, exit,
        owners, loans, outer_owners, outer_loans,
    };
    let mut outcomes = Vec::new();
    for edge in edges(&branch.terminator.as_ref().unwrap().kind) {
        assert_eq!(
            edge.arguments.iter().filter(|argument| transaction.outer_owners.contains(argument)).count(),
            1
        );
        assert_eq!(
            edge.arguments.iter().filter(|argument| transaction.outer_loans.contains(argument)).count(),
            1
        );
        if fixture.prefix != Prefix::IncomingBorrow {
            assert_eq!(
                edge.arguments.iter().filter(|argument| transaction.owners.contains(argument)).count(),
                1
            );
        }
        if fixture.prefix == Prefix::TemporaryBorrow {
            assert_eq!(
                edge.arguments.iter().filter(|argument| transaction.loans.contains(argument)).count(),
                1
            );
        }
        outcomes.extend(transaction.visit(edge.target, Vec::new(), Vec::new()));
    }
    outcomes.sort();
    assert_eq!(
        outcomes,
        if fixture.exit == Exit::Commit { vec![true, true] } else { vec![false, true] }
    );
    if fixture.prefix == Prefix::IncomingBorrow {
        assert!(function.instructions.iter().all(|instruction| !matches!(
            instruction.operation, Operation::BorrowEnd { loan }
                if transaction.loans.contains(&EntityId::Loan(loan))
        )), "the incoming loan is never ended by this caller");
    }
    crate::llvm::render_verified_program(program)
        .expect("the complete source program emits verified LLVM");
}

/// Visit every cell before the final success assertion. Old-production expected reds cannot
/// short-circuit after one case; a wrong kind/span is instead an immediate unexpected failure.
fn prepared_matrix() -> Vec<(Fixture, UnitSource)> {
    let mut prepared = Vec::new();
    let mut failures = Vec::new();
    for fixture in fixtures() {
        let source = UnitSource::analyze(&fixture.name, &fixture.text)
            .expect("strict whole-source admission");
        assert_products(&source, &fixture);
        match source.lower_entry(source.declaration("entry", SymbolKind::Function)) {
            Ok((program, _)) => assert_paths(&source, &fixture, &program),
            Err(error) => {
                assert_ne!(
                    fixture.exit, Exit::Commit,
                    "normal commit is an existing success, not a planned red"
                );
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{}", fixture.name);
                let (_, span) = expression(
                    &source, item_span(&source, "exercise"), &fixture.outer_call,
                );
                assert_eq!(
                    error.span, Some(span),
                    "{}: only the exact outer direct-call gate is a planned red", fixture.name
                );
                failures.push(format!("{}: {error:?}", fixture.name));
            }
        }
        prepared.push((fixture, source));
    }
    assert_eq!(prepared.len(), 12);
    assert!(
        failures.is_empty(),
        "all 12 cells were inspected; direct prefix transactions still rejected:\n{}",
        failures.join("\n")
    );
    prepared
}

#[test]
fn ordinary_direct_prefix_facts_paths_and_verified_llvm_cover_twelve_cells() {
    assert_eq!(prepared_matrix().len(), 12);
}

fn expected_stdout(prefix: Prefix, exit: Exit) -> String {
    // Leaf is a resource: the named incoming callerOwner drops at run's lexical end,
    // after "caller"; it is not a pure-memory ASAP owner.
    let normal = if prefix == Prefix::IncomingBorrow {
        "callee\nouter-callee\nouter-drop\nafter-call\ntail\ncaller\ndrop\n"
    } else {
        "callee\ndrop\nouter-callee\nouter-drop\nafter-call\ntail\ncaller\n"
    };
    let cancelled = match (prefix, exit) {
        (_, Exit::Commit) => normal,
        (Prefix::IncomingBorrow, Exit::Return) => "outer-drop\ncaller\ndrop\n",
        (Prefix::IncomingBorrow, Exit::Break | Exit::Continue) => "outer-drop\ntail\ncaller\ndrop\n",
        (_, Exit::Return) => "drop\nouter-drop\ncaller\n",
        (_, Exit::Break | Exit::Continue) => "drop\nouter-drop\ntail\ncaller\n",
    };
    format!("true\n{cancelled}false\n{normal}done\n")
}

#[test]
fn ordinary_direct_prefix_cancellations_and_commits_link_run_with_exact_stdout() {
    // Preflight all twelve exact sources before any object/link/run, including on old production.
    for (fixture, _) in prepared_matrix() {
        let output = emit_link_and_run(&fixture.name, &fixture.text, "entry");
        assert!(output.status.success(), "{}: {output:?}", fixture.name);
        assert!(output.stderr.is_empty(), "{}: {output:?}", fixture.name);
        assert_eq!(
            output.stdout, expected_stdout(fixture.prefix, fixture.exit).as_bytes(),
            "{}", fixture.name
        );
    }
}

#[test]
fn ordinary_direct_prefix_scope_negatives_keep_exact_call_rejections() {
    let control = "when (flag) { true -> { return } else -> { 7 } }";
    for (name, definitions, setup, call) in [
        (
            "field", "class Holder(val item: Leaf)", "val holder = Holder(Leaf())",
            format!("accept(holder.item, {control})"),
        ),
        (
            "element", "", "val items = arrayOf<Leaf>(Leaf())",
            format!("accept(items[0], {control})"),
        ),
        (
            "receiver", "class Host { fun method(item: Leaf, own number: Int): Unit {} }",
            "val host = Host()", format!("host.method(Leaf(), {control})"),
        ),
        (
            "function-value", "",
            "val action: (Leaf, own Int) -> Unit = { item, number -> }",
            format!("action(Leaf(), {control})"),
        ),
    ] {
        let text = format!(
            "class Leaf\n{definitions}\n\
             fun accept(item: Leaf, own number: Int): Unit {{}}\n\
             fun entry(own flag: Boolean): Unit {{ {setup}\n{call}\n}}"
        );
        let source = UnitSource::analyze(&format!("direct-prefix-scope-{name}.ko"), &text)
            .expect("ordinary negative source is frontend-valid");
        source.assert_frontend_clean();
        assert!(matches!(&source.products, OwnedProducts::Basic(..)));
        let (_, span) = expression(&source, item_span(&source, "entry"), &call);
        let error = source.lower_entry(source.declaration("entry", SymbolKind::Function))
            .err().expect("out-of-scope call shape remains rejected");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{name}");
        assert_eq!(error.span, Some(span), "{name}: exact original call boundary");
        assert_eq!(source.sources.slice(span), Ok(call.as_str()));
    }
}
