//! Independent real-source generation contracts; SSA only, never native execution.

use super::support::{
    Birth, Control, aliases, birth, controls, expression, function, item_span, root, span,
};
use super::{OwnedProducts, SourceRejectionKind, UnitSource};
use crate::ssa::{
    LoweringErrorKind,
    model::{BlockId, EntityId, Function, Operation, Origin, Program, TerminatorKind},
    verify::verify_program,
};
use lang_frontend::{
    diagnostic::{DiagnosticDetail, Severity},
    name_resolution::SymbolKind,
    ownership_checking::{
        LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitExpressionId, UnitTypeKind},
};
use std::collections::BTreeSet;

// These complete documents are identical in the old-API and green payloads.
const SEQUENTIAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> { observe(x) }
    }
    x = replacement
    when (x) {
        null -> {};
        else -> { observe((x)) }
    }
}
"#;
const STALE: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            observe(x)
        }
    }
}
"#;

const FIRST_WHEN: (usize, usize) = (130, 197);
const SECOND_WHEN: (usize, usize) = (222, 291);
const REPLACEMENT: (usize, usize) = (202, 217);

fn assert_span(source: &UnitSource, origin: Span, expected: (usize, usize), text: &str) {
    assert_eq!(origin.source_id(), source.parsed.source_id());
    assert_eq!((origin.start(), origin.end()), expected);
    assert_eq!(source.sources.slice(origin), Ok(text));
}

fn call_argument(
    source: &UnitSource,
    control: &Control,
    text: &str,
) -> (UnitExpressionId, UnitExpressionId) {
    let (call, _) = expression(source, control.span, text);
    let node = source
        .parsed
        .ast()
        .expressions()
        .get(call.expression())
        .unwrap();
    let Expression::Call { arguments, .. } = node.payload() else {
        panic!("one real direct observe call")
    };
    let [argument] = arguments.as_slice() else {
        panic!("one source argument")
    };
    (
        call,
        UnitExpressionId::new(call.source_unit(), argument.value),
    )
}

fn canonical_reference(source: &UnitSource, mut argument: UnitExpressionId) -> UnitExpressionId {
    loop {
        let node = source
            .parsed
            .ast()
            .expressions()
            .get(argument.expression())
            .unwrap();
        match node.payload() {
            Expression::Group { expression } => {
                argument = UnitExpressionId::new(argument.source_unit(), *expression);
            }
            Expression::Name => return argument,
            _ => panic!("only the actual transparent Group and Name"),
        }
    }
}

fn assert_original_products(source: &UnitSource, controls: &[Control]) {
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("real Basic products")
    };
    let typed = typed.types();
    let owned = owned.ownership();
    let symbol = root(source, controls[0].subject);
    assert_eq!(root(source, controls[1].subject), symbol);
    let scope = item_span(source, "entry");
    let (assignment, assignment_span) = expression(source, scope, "x = replacement");
    assert_span(source, assignment_span, REPLACEMENT, "x = replacement");
    let descriptor = typed
        .assignment(assignment)
        .expect("real assignment storage contract");
    assert_eq!(descriptor.expression(), assignment);
    assert_eq!(root(source, descriptor.target()), symbol);
    assert_eq!(Some(descriptor.target_type()), typed.symbol_type(symbol));
    assert!(descriptor.falls_through());
    let declaration = source.names.names().source_units()[symbol.source_unit().index()]
        .resolution()
        .symbols()[symbol.symbol().index()]
    .span();
    assert_eq!(source.sources.slice(declaration), Ok("x"));
    let origins = [declaration, assignment_span];
    let observe = source.declaration("observe", SymbolKind::Function);
    let mut expected_drops = Vec::new();
    for (index, control) in controls.iter().enumerate() {
        let text = if index == 0 {
            "observe(x)"
        } else {
            "observe((x))"
        };
        let (call, argument) = call_argument(source, control, text);
        let actual = typed.call(call).expect("real source call contract");
        assert_eq!(actual.target(), UnitCallTarget::Declaration(observe));
        assert!(actual.receiver().is_none());
        let [mapping] = actual.arguments() else {
            panic!("one real argument mapping")
        };
        assert_eq!(mapping.mode(), ParameterMode::Borrow);
        let proof = typed
            .non_null_use(argument)
            .expect("fresh branch narrowed argument");
        assert_eq!(proof.symbol(), symbol);
        assert_eq!(proof.expression(), argument);
        assert_eq!(Some(proof.declared_type()), typed.symbol_type(symbol));
        assert_eq!(
            typed.types().get(proof.declared_type()),
            Some(&UnitTypeKind::Nullable(proof.narrowed_type()))
        );
        let contracts = owned.call_argument_contracts_for(call).collect::<Vec<_>>();
        let [contract] = contracts.as_slice() else {
            panic!("one actual call argument ownership contract")
        };
        assert_eq!(contract.argument(), argument);
        assert_eq!(contract.kind(), UnitCallArgumentOwnershipKind::SharedLoan);
        assert_eq!(contract.parameter_type(), mapping.parameter_type());
        assert_eq!(contract.argument_span(), span(source, argument));
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == argument)
            .collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            panic!("one real source loan, not a manufactured proof")
        };
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.begin_span(), span(source, argument));
        assert_eq!(loan.end_span(), span(source, call));
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("actual same-symbol whole root")
        };
        assert!(place.is_root());
        assert_eq!(place.root(), symbol);
        assert!(
            !owned
                .value_deliveries()
                .iter()
                .any(|fact| fact.call() == call)
        );
        expected_drops.push((UnitDropPoint::CallReturn(call), origins[index]));
        expected_drops.push((
            UnitDropPoint::BranchExit {
                control: control.id,
                branch: 0,
            },
            origins[index],
        ));
        assert!(
            !owned.drops().iter().any(|fact| {
                fact.target() == UnitDropTarget::Named(symbol)
                    && fact.point() == UnitDropPoint::AfterExpression(control.subject)
            }),
            "observe keeps this generation alive until its real call returns"
        );
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), expected_drops.len());
    for (point, origin) in expected_drops {
        let matching = drops
            .iter()
            .filter(|fact| fact.point() == point && fact.value_origin() == origin)
            .count();
        assert_eq!(matching, 1, "one exact generation/path cleanup fact");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Branch(usize, bool),
    Observe(usize),
    End(usize),
    Drop(usize),
}

struct Paths<'a> {
    function: &'a Function,
    births: [Birth<'a>; 2],
    controls: &'a [Control],
    calls: [Span; 2],
    origins: [Span; 2],
    observe: crate::ssa::model::FunctionId,
}

impl Paths<'_> {
    fn visit(
        &self,
        block: BlockId,
        seen: &mut BTreeSet<BlockId>,
        events: Vec<Event>,
    ) -> Vec<Vec<Event>> {
        assert!(
            seen.insert(block),
            "these exact sources have no loop/backedge"
        );
        let block = self.function.block(block).unwrap();
        let mut events = events;
        for id in &block.instructions {
            let instruction = self.function.instruction(*id).unwrap();
            match &instruction.operation {
                Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } => {
                    let index = self
                        .calls
                        .iter()
                        .position(|origin| instruction.origin == Origin::Source(*origin))
                        .expect("only the two source-qualified observe calls");
                    assert_eq!(*callee, self.observe);
                    assert!(receiver.is_none());
                    assert_eq!(arguments, &[EntityId::Loan(self.births[index].view)]);
                    assert!(self.births[index].views.contains(&arguments[0]));
                    assert!(!self.births[1 - index].views.contains(&arguments[0]));
                    events.push(Event::Observe(index));
                }
                Operation::BorrowEnd { loan } => {
                    let matches = self
                        .births
                        .iter()
                        .enumerate()
                        .filter(|(_, birth)| birth.views.contains(&EntityId::Loan(*loan)))
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();
                    let [index] = matches.as_slice() else {
                        panic!("retire exactly one real generation view")
                    };
                    assert_eq!(instruction.origin, Origin::Source(self.origins[*index]));
                    events.push(Event::End(*index));
                }
                Operation::Drop { owner } => {
                    let matches = self
                        .births
                        .iter()
                        .enumerate()
                        .filter(|(_, birth)| birth.owners.contains(&EntityId::Value(*owner)))
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();
                    let [index] = matches.as_slice() else {
                        panic!("drop exactly one actual generation owner")
                    };
                    assert_eq!(instruction.origin, Origin::Source(self.origins[*index]));
                    events.push(Event::Drop(*index));
                }
                Operation::NullableTake { .. }
                | Operation::SharedRetain { .. }
                | Operation::BorrowBegin { .. } => {
                    panic!("read-only generation proofs never take, retain or mint another loan")
                }
                _ => {}
            }
        }
        let term = block.terminator.as_ref().unwrap();
        match &term.kind {
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => {
                let index = self
                    .births
                    .iter()
                    .position(|birth| birth.header == block.id)
                    .expect("only two actual source When headers");
                let birth = &self.births[index];
                assert_eq!(term.origin, Origin::Source(self.controls[index].span));
                assert_eq!((*owner, *view), (birth.owner, birth.view));
                assert_eq!(when_null, birth.null);
                assert_eq!(when_non_null, birth.present);
                let mut paths = Vec::new();
                for (present, edge) in [(false, when_null), (true, when_non_null)] {
                    let mut next = events.clone();
                    next.push(Event::Branch(index, present));
                    paths.extend(self.visit(edge.target, &mut seen.clone(), next));
                }
                paths
            }
            TerminatorKind::Branch(edge) => {
                assert!(
                    self.controls
                        .iter()
                        .any(|control| { term.origin == Origin::Source(control.span) }),
                    "only real When arm/join edges, not arbitrary branch fallbacks"
                );
                for (index, birth) in self.births.iter().enumerate() {
                    if events.contains(&Event::Drop(index)) {
                        assert!(
                            edge.arguments.iter().all(|entity| {
                                !birth.owners.contains(entity) && !birth.views.contains(entity)
                            }),
                            "retired generation cannot cross its actual outgoing edge"
                        );
                    }
                }
                self.visit(edge.target, seen, events)
            }
            TerminatorKind::Return { values } => {
                assert_eq!(term.origin, self.function.origin);
                assert!(
                    values.is_empty(),
                    "source entry has only implicit Unit return"
                );
                vec![events]
            }
            _ => panic!("exact source has no If, While, Abort or other transfer"),
        }
    }
}

fn assert_ssa_generations(source: &UnitSource, program: &Program, controls: &[Control]) {
    verify_program(program).expect("the actual complete Unit SSA must verify");
    let entry = function(source, program, "entry");
    assert_eq!(entry.origin, Origin::Source(item_span(source, "entry")));
    let births = [birth(entry, &controls[0]), birth(entry, &controls[1])];
    assert_ne!(births[0].owner, births[1].owner);
    assert_ne!(births[0].view, births[1].view);
    assert!(births[0].owners.is_disjoint(&births[1].owners));
    assert!(births[0].views.is_disjoint(&births[1].views));
    let parameters = &entry
        .block(entry.entry_block().unwrap())
        .unwrap()
        .parameters;
    let [input @ EntityId::Value(_), replacement @ EntityId::Value(_)] = parameters.as_slice()
    else {
        panic!("actual two owned nullable parameters in declaration order")
    };
    let old_owner = aliases(entry, *input);
    let new_owner = aliases(entry, *replacement);
    assert!(old_owner.is_disjoint(&new_owner));
    assert!(old_owner.contains(&EntityId::Value(births[0].owner)));
    assert!(new_owner.contains(&EntityId::Value(births[1].owner)));
    assert!(!old_owner.contains(&EntityId::Value(births[1].owner)));
    let symbol = root(source, controls[0].subject);
    let declaration = source.names.names().source_units()[symbol.source_unit().index()]
        .resolution()
        .symbols()[symbol.symbol().index()]
    .span();
    let assignment = expression(source, item_span(source, "entry"), "x = replacement").1;
    let calls = [
        expression(source, controls[0].span, "observe(x)").1,
        expression(source, controls[1].span, "observe((x))").1,
    ];
    let paths = Paths {
        function: entry,
        births,
        controls,
        calls,
        origins: [declaration, assignment],
        observe: function(source, program, "observe").id,
    };
    let actual = paths.visit(
        entry.entry_block().unwrap(),
        &mut BTreeSet::new(),
        Vec::new(),
    );
    let mut expected = Vec::new();
    for first in [false, true] {
        for second in [false, true] {
            let mut events = Vec::new();
            for (index, present) in [(0, first), (1, second)] {
                events.push(Event::Branch(index, present));
                if present {
                    events.push(Event::Observe(index));
                    events.push(Event::End(index));
                }
                events.push(Event::Drop(index));
            }
            expected.push(events);
        }
    }
    assert_eq!(
        actual, expected,
        "all four real paths, exact ordered cleanup once per generation"
    );
}

fn assert_new_generation_products(source: &UnitSource, controls: &[Control]) {
    use lang_frontend::{
        ownership_checking::UnitNullableWhenUseKind,
        type_checking::{NullableWhenSubjectCategory, UnitNullableWhenDomain as Domain},
    };
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("real Basic products")
    };
    let typed = typed.types();
    let owned = owned.ownership();
    assert_eq!(typed.nullable_whens().len(), 2);
    assert_eq!(owned.nullable_whens().len(), 2);
    let symbol = root(source, controls[0].subject);
    let mut generations = Vec::new();
    let mut references = Vec::new();
    let mut witnessed = Vec::new();
    for (index, control) in controls.iter().enumerate() {
        let descriptor = typed
            .nullable_when(control.id)
            .expect("real bounded control");
        assert_eq!(descriptor.expression(), control.id);
        assert_eq!(descriptor.subject(), control.subject);
        assert_eq!(descriptor.subject_reference(), Some(control.subject));
        assert_eq!(descriptor.stable_symbol(), Some(symbol));
        assert_eq!(
            descriptor.source_category(),
            NullableWhenSubjectCategory::OwnedRoot
        );
        assert_eq!(Some(descriptor.nullable_type()), typed.symbol_type(symbol));
        assert_eq!(
            typed.types().get(descriptor.nullable_type()),
            Some(&UnitTypeKind::Nullable(descriptor.inner_type()))
        );
        let generation = descriptor
            .source_generation()
            .expect("real source generation");
        assert_ne!(generation, u64::MAX);
        generations.push(generation);
        let [null, present] = descriptor.entries() else {
            panic!("exactly the source null/else entries")
        };
        assert_eq!(null.input_domain(), Domain::Nullable);
        assert_eq!(null.body_domain(), Domain::Null);
        assert_eq!(null.remaining_domain(), Domain::NonNull);
        assert_eq!(present.input_domain(), Domain::NonNull);
        assert_eq!(present.body_domain(), Domain::NonNull);
        assert_eq!(present.remaining_domain(), Domain::Empty);
        let plan = owned
            .nullable_when(control.id)
            .expect("same-analysis owned plan");
        assert_eq!(plan.descriptor(), descriptor);
        let place = plan
            .source_place()
            .expect("owned local root, never fabricated temporary");
        assert!(place.is_root());
        assert_eq!(place.root(), symbol);
        assert_eq!(plan.entries().len(), 2);
        for (entry_index, (entry, owned_entry)) in
            descriptor.entries().iter().zip(plan.entries()).enumerate()
        {
            assert_eq!(entry.index(), entry_index);
            assert_eq!(entry.body(), controls[index].entries[entry_index].0);
            assert_eq!(entry.span(), controls[index].entries[entry_index].1);
            assert_eq!(
                (owned_entry.index(), owned_entry.body()),
                (entry_index, entry.body())
            );
            assert!(entry.falls_through() && owned_entry.falls_through());
            assert!(!owned_entry.breaks() && !owned_entry.continues());
        }
        let text = if index == 0 {
            "observe(x)"
        } else {
            "observe((x))"
        };
        let (call, argument) = call_argument(source, control, text);
        let reference = canonical_reference(source, argument);
        references.push(reference);
        let [use_fact] = plan.uses() else {
            panic!("one original outer argument use per real generation")
        };
        assert_eq!((use_fact.call(), use_fact.argument()), (call, argument));
        assert_eq!(use_fact.reference(), reference);
        assert_eq!(use_fact.kind(), UnitNullableWhenUseKind::Observe);
        assert!(use_fact.delivery().is_none() && use_fact.moved_span().is_none());
        assert_eq!(owned.nullable_when_use(argument), Some(use_fact));
        let loan = use_fact.loan().expect("actual original shared loan");
        assert!(owned.loans().contains(loan));
        assert_eq!((loan.call(), loan.argument()), (call, argument));
        assert_eq!(loan.kind(), LoanKind::Shared);
        let mut current = argument;
        loop {
            let witness = typed
                .nullable_when_use(current)
                .expect("real Name/Group witness");
            assert_eq!(witness.expression(), current);
            assert_eq!(witness.reference(), reference);
            assert_eq!(witness.control(), control.id);
            assert_eq!(witness.subject(), control.subject);
            assert_eq!(witness.entry_index(), 1);
            assert_eq!(witness.body(), control.entries[1].0);
            assert_eq!(witness.symbol(), symbol);
            assert_eq!(witness.source_generation(), generation);
            assert_eq!(
                (witness.nullable_type(), witness.inner_type()),
                (descriptor.nullable_type(), descriptor.inner_type())
            );
            assert_ne!(witness.control(), controls[1 - index].id);
            if current == argument {
                assert_eq!(witness, use_fact.witness());
            }
            witnessed.push(current);
            if current == reference {
                break;
            }
            let Expression::Group { expression } = source
                .parsed
                .ast()
                .expressions()
                .get(current.expression())
                .unwrap()
                .payload()
            else {
                panic!("canonical reference reached only by actual Group chain")
            };
            current = UnitExpressionId::new(current.source_unit(), *expression);
        }
    }
    assert_ne!(controls[0].id, controls[1].id);
    assert_ne!(controls[0].subject, controls[1].subject);
    assert_ne!(references[0], references[1]);
    assert_eq!(Some(generations[1]), generations[0].checked_add(1));
    let actual = typed
        .nullable_when_uses()
        .iter()
        .map(|witness| witness.expression())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, witnessed.into_iter().collect::<BTreeSet<_>>());
}

#[test]
fn nullable_when_sequential_generations_keep_distinct_real_proofs() {
    let source = UnitSource::analyze("sequential-generations", SEQUENTIAL)
        .unwrap_or_else(|error| panic!("unexpected earlier source rejection: {error:?}"));
    source.assert_frontend_clean();
    assert_eq!(
        source.sources.source_text(source.parsed.source_id()),
        Ok(SEQUENTIAL)
    );
    let controls = controls(&source, "entry");
    assert_eq!(controls.len(), 2);
    for (control, expected) in controls.iter().zip([FIRST_WHEN, SECOND_WHEN]) {
        assert_span(
            &source,
            control.span,
            expected,
            &SEQUENTIAL[expected.0..expected.1],
        );
    }
    assert_original_products(&source, &controls);
    let program = match source.lower_entry(source.declaration("entry", SymbolKind::Function)) {
        Ok((program, entry)) => {
            assert_eq!(entry, function(&source, &program, "entry").id);
            program
        }
        Err(error) => {
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            let origin = error.span.expect("source-qualified old boundary");
            assert_span(
                &source,
                origin,
                FIRST_WHEN,
                &SEQUENTIAL[FIRST_WHEN.0..FIRST_WHEN.1],
            );
            assert_eq!(origin, controls[0].span);
            panic!(
                "expected old-production first When rejection at 130..197; second When, replacement SSA and verifier assertions NOT reached"
            );
        }
    };
    assert_new_generation_products(&source, &controls);
    assert_ssa_generations(&source, &program, &controls);
}

// Type diagnostics intentionally display unit-local type IDs. Derive those IDs from
// the same complete source's public raw type product, never from observed output.
fn assert_label_product_identity(
    sources: &lang_frontend::source::SourceMap,
    original: &lang_frontend::diagnostic::Diagnostic,
    label: &str,
) {
    let primary = original.primary_span();
    use lang_frontend::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let lexed = lex(sources, primary.source_id()).expect("same real document lexing");
    let parsed = parse_file(sources, &lexed).expect("same real document AST");
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::memory_document(
        primary.source_id(),
        &parsed,
    )];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, &inputs).expect("same real source index");
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
        .expect("same real names");
    assert!(names.diagnostics().is_empty());
    let names = names.validate().expect("complete names");
    let typed = check_compilation_unit_types(sources, &inputs, &names, &environment)
        .expect("the actual error is a source diagnostic, not internal failure");
    let [diagnostic] = typed.diagnostics() else {
        panic!("same single real type diagnostic")
    };
    assert_eq!(
        diagnostic, original,
        "the same complete diagnostic is reproduced"
    );
    let callable = |name| {
        let declarations = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.name() == name)
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("one real source declaration")
        };
        typed
            .signatures()
            .declaration(declaration.id())
            .unwrap()
            .callable()
            .unwrap()
    };
    let expected = callable("observe").parameters()[0].ty();
    let replacement = callable("entry").parameters()[1].ty();
    let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(expected) else {
        panic!("observe expects the actual Node declaration")
    };
    assert_eq!(
        names.names().index().declarations()[declaration.index()].name(),
        "Node"
    );
    let arguments = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| node.span() == primary)
        .collect::<Vec<_>>();
    let [(argument, node)] = arguments.as_slice() else {
        panic!("the actual failed argument AST node")
    };
    assert!(matches!(node.payload(), Expression::Name));
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Expression::Call { arguments, .. }
            if arguments.len() == 1 && arguments[0].value == *argument)
        })
        .collect::<Vec<_>>();
    let [(_, call)] = calls.as_slice() else {
        panic!("the exact failed argument belongs to one real call")
    };
    assert_eq!(sources.slice(call.span()), Ok("observe(x)"));
    let [unit] = names.names().source_units() else {
        panic!("one original document")
    };
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == unit.source_unit() && reference.span() == primary
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        panic!("one real failed argument reference")
    };
    let lang_frontend::name_resolution::UnitReferenceTarget::Symbol(symbol) = reference.target()
    else {
        panic!("the actual mutable local root")
    };
    assert_eq!(
        unit.resolution().symbols()[symbol.symbol().index()].name(),
        "x"
    );
    let actual_type = typed.symbol_type(*symbol).unwrap();
    assert_eq!(actual_type, replacement);
    let actual = typed.types().get(actual_type).unwrap();
    assert_eq!(actual, &UnitTypeKind::Nullable(expected));
    let argument = UnitExpressionId::new(unit.source_unit(), *argument);
    assert!(
        typed.non_null_use(argument).is_none(),
        "no old smart cast on failed use"
    );
    assert_eq!(
        label,
        format!(
            "expected declaration#{}, found {actual:?}",
            declaration.index()
        )
    );
}

#[test]
fn nullable_when_replacement_without_reproof_keeps_exact_type_error() {
    let rejection = match UnitSource::analyze("stale-use-after-replacement", STALE) {
        Err(rejection) => rejection,
        Ok(_) => panic!("old non-null knowledge must not survive nullable replacement"),
    };
    assert!(matches!(rejection.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        rejection.report.stages,
        [("parse", 0), ("names", 0), ("types", 1)]
    );
    let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
        panic!("one complete real type diagnostic, no suppressed cascades")
    };
    assert_eq!(*stage, "types");
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert_eq!(diagnostic.code().to_string(), "L0084");
    assert_eq!(
        diagnostic.message(),
        "expression type does not match the expected type"
    );
    let primary = diagnostic.primary_span();
    assert_eq!((primary.start(), primary.end()), (227, 228));
    assert_eq!(
        rejection.sources.source_text(primary.source_id()),
        Ok(STALE)
    );
    assert_eq!(rejection.sources.slice(primary), Ok("x"));
    let [DiagnosticDetail::Label(label)] = diagnostic.details() else {
        panic!("exactly the original expected-parameter label")
    };
    assert_eq!(label.span().source_id(), primary.source_id());
    assert_eq!((label.span().start(), label.span().end()), (26, 37));
    assert_eq!(rejection.sources.slice(label.span()), Ok("value: Node"));
    assert_label_product_identity(&rejection.sources, diagnostic, label.message());
}
