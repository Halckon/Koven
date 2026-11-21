//! Same original sources through verified ordinary Unit; no Legacy no-site equivalence.
use std::collections::BTreeSet;

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        LoweringErrorKind,
        model::{Definition, Edge, EntityId, Function, LoanKind, Operation, TerminatorKind},
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitKey, SymbolKind},
    ownership_checking::{UnitCallableOrigin, UnitLoanTarget},
    parser::Expression,
    type_checking::{UnitCallTarget, UnitCallableTarget, UnitExpressionId},
};

const NAME: &str = "single-runtime-intent.ko";
const BARE: &str = "fun identity(index: Int): Int = index\nfun entry(): Int { val items = List<Int>(3, identity)\nreturn items[2] }";
const BREAK: &str = "helper(identity, if (true) { while (true) { break }\n3 } else { 0 })";
const CONTINUE: &str = "helper(identity, if (true) { var step = 0\nwhile (step < 1) { step += 1\ncontinue }\n3 } else { 0 })";
const NESTED: &str = "fun entry(): Unit { val callback: ()->Unit = { val nested: ()->Unit = {}\nnested() }\ncallback() }";

fn strict(text: &str) -> UnitSource {
    let unit = UnitSource::analyze(NAME, text).unwrap();
    unit.assert_frontend_clean();
    let [input] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(input.source_id(), unit.parsed().source_id());
    assert_eq!(unit.sources().source_name(input.source_id()), Ok(NAME));
    assert_eq!(unit.sources().source_text(input.source_id()), Ok(text));
    unit
}

fn expression(unit: &UnitSource, source: &str) -> UnitExpressionId {
    let [input] = unit.names().names().index().source_units() else {
        unreachable!()
    };
    let expressions = unit
        .parsed()
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| unit.sources().slice(node.span()) == Ok(source))
        .collect::<Vec<_>>();
    let [(id, _)] = expressions.as_slice() else {
        panic!("one original expression: {source}");
    };
    UnitExpressionId::new(input.id(), *id)
}

fn known_callback(unit: &UnitSource) -> (DeclarationId, UnitExpressionId) {
    let declaration = unit.declaration("identity", SymbolKind::Function);
    let operand = expression(unit, "identity");
    assert_eq!(
        unit.ownership().callable_origin(operand).unwrap().origin(),
        UnitCallableOrigin::KnownFunction(UnitCallableTarget::Declaration(declaration))
    );
    (declaration, operand)
}

#[test]
fn bare_known_initializer_carries_the_original_temporary_loan_to_list() {
    let unit = strict(BARE);
    let (identity, operand) = known_callback(&unit);
    let loans = unit
        .ownership()
        .loans()
        .iter()
        .filter(|loan| loan.argument() == operand)
        .collect::<Vec<_>>();
    let [loan] = loans.as_slice() else {
        panic!("real initializer loan");
    };
    assert_eq!(loan.target(), &UnitLoanTarget::Temporary(operand));

    let declaration = unit.declaration("entry", SymbolKind::Function);
    let arena = unit.types().types().len();
    let (program, entry) = unit.lower_entry(declaration).unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    assert_eq!(unit.types().types().len(), arena);
    let module = program.module(entry.module()).unwrap();
    let entry = module.function(entry).unwrap();
    assert_eq!(entry.origin.span(), unit.declaration_span(declaration));
    let formed = callback_address(&unit, module, entry, identity);

    let constructors = entry
        .instructions
        .iter()
        .filter(|i| {
            unit.sources().slice(i.origin.span()) == Ok("List<Int>(3, identity)")
                && matches!(i.operation, Operation::ContainerGenerateBorrowed { .. })
        })
        .collect::<Vec<_>>();
    let [constructor] = constructors.as_slice() else {
        panic!("one true original List constructor");
    };
    let Operation::ContainerGenerateBorrowed { initializer, .. } = constructor.operation else {
        unreachable!()
    };
    assert!(transported_from(entry, EntityId::Loan(initializer), formed));
}

#[test]
fn helper_with_inner_break_keeps_its_original_callback_argument() {
    assert_helper(BREAK);
}

#[test]
fn helper_with_inner_continue_keeps_its_original_callback_argument() {
    assert_helper(CONTINUE);
}

fn assert_helper(call_source: &str) {
    let text = format!(
        "fun identity(index: Int): Int = index\nfun helper(callback: (Int)->Int, count: Int): Int {{ val items = List<Int>(count, callback)\nreturn items[2] }}\nfun entry(): Int = {call_source}"
    );
    let unit = strict(&text);
    let (identity, _) = known_callback(&unit);
    let helper = unit.declaration("helper", SymbolKind::Function);
    let call_expression = expression(&unit, call_source);
    let selected = unit
        .types()
        .calls()
        .iter()
        .filter(|call| call.expression() == call_expression)
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        panic!("the original call has one selected target");
    };
    assert_eq!(selected.target(), UnitCallTarget::Declaration(helper));
    let declaration = unit.declaration("entry", SymbolKind::Function);
    let arena = unit.types().types().len();
    let (program, entry) = unit.lower_entry(declaration).unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    assert_eq!(unit.types().types().len(), arena);
    let module = program.module(entry.module()).unwrap();
    let entry = module.function(entry).unwrap();
    assert_eq!(entry.origin.span(), unit.declaration_span(declaration));
    let formed = callback_address(&unit, module, entry, identity);
    let calls = entry
        .instructions
        .iter()
        .filter(|i| {
            unit.sources().slice(i.origin.span()) == Ok(call_source)
                && matches!(i.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one direct call from this exact original expression");
    };
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        panic!("normal named helper");
    };
    let [callback @ EntityId::Loan(_), count @ EntityId::Loan(_)] = arguments.as_slice() else {
        panic!("both original parameters are loan ABI arguments");
    };
    assert_ne!(callback, count);
    assert!(transported_from(entry, *callback, formed));
    let helper_function = module.function(*callee).unwrap();
    assert_eq!(helper_function.origin.span(), unit.declaration_span(helper));
    let helper_entry = helper_function
        .block(helper_function.entry_block().unwrap())
        .unwrap();
    let [callback_param, _] = helper_entry.parameters.as_slice() else {
        panic!("original helper parameter order");
    };
    let generators = helper_function
        .instructions
        .iter()
        .filter_map(|i| {
            let Operation::ContainerGenerateBorrowed { initializer, .. } = i.operation else {
                return None;
            };
            (unit.sources().slice(i.origin.span()) == Ok("List<Int>(count, callback)"))
                .then_some(EntityId::Loan(initializer))
        })
        .collect::<Vec<_>>();
    let [initializer] = generators.as_slice() else {
        panic!("actual helper List construction");
    };
    assert!(transported_from(
        helper_function,
        *initializer,
        *callback_param
    ));
}

fn callback_address(
    unit: &UnitSource,
    module: &crate::ssa::model::Module,
    function: &Function,
    declaration: DeclarationId,
) -> EntityId {
    let addresses = function
        .instructions
        .iter()
        .filter_map(|i| {
            let Operation::FunctionAddress { target } = i.operation else {
                return None;
            };
            (unit.sources().slice(i.origin.span()) == Ok("identity")).then_some((i, target))
        })
        .collect::<Vec<_>>();
    let [(address, target)] = addresses.as_slice() else {
        panic!("one original source address");
    };
    assert_eq!(
        module.function(*target).unwrap().origin.span(),
        unit.declaration_span(declaration)
    );
    let [result @ EntityId::Value(_)] = address.results.as_slice() else {
        panic!("one actual pointer");
    };
    *result
}

fn transported_from(function: &Function, value: EntityId, source: EntityId) -> bool {
    fn visit(
        function: &Function,
        value: EntityId,
        source: EntityId,
        visited: &mut BTreeSet<EntityId>,
        found: &mut bool,
    ) -> bool {
        if value == source {
            *found = true;
            return true;
        }
        if !visited.insert(value) {
            return true;
        }
        let Some(entity) = function.entity(value) else {
            return false;
        };
        match entity.definition {
            Definition::InstructionResult { instruction, .. } => {
                let dependency = match function.instruction(instruction).unwrap().operation {
                    Operation::CallableViewOf { source } | Operation::SharedReborrow { source } => {
                        EntityId::Loan(source)
                    }
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    } => EntityId::Place(place),
                    Operation::RootPlace { owner } | Operation::Copy { source: owner } => {
                        EntityId::Value(owner)
                    }
                    _ => return false,
                };
                visit(function, dependency, source, visited, found)
            }
            Definition::BlockParameter { block, index } => {
                let edges = function
                    .blocks
                    .iter()
                    .flat_map(
                        |incoming| match &incoming.terminator.as_ref().unwrap().kind {
                            TerminatorKind::Branch(edge) => vec![edge],
                            TerminatorKind::Conditional {
                                when_true,
                                when_false,
                                ..
                            } => vec![when_true, when_false],
                            _ => Vec::<&Edge>::new(),
                        },
                    )
                    .filter(|edge| edge.target == block)
                    .collect::<Vec<_>>();
                !edges.is_empty()
                    && edges.into_iter().all(|edge| {
                        edge.arguments
                            .get(index)
                            .is_some_and(|id| visit(function, *id, source, visited, found))
                    })
            }
        }
    }
    let mut found = false;
    visit(function, value, source, &mut BTreeSet::new(), &mut found) && found
}

#[test]
fn evaluated_nested_lambda_rejects_at_its_real_inner_literal_after_frontend() {
    let unit = strict(NESTED);
    let [document] = unit.names().names().index().source_units() else {
        unreachable!();
    };
    let lambdas = unit
        .parsed()
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 2);
    for (id, _) in lambdas {
        let id = UnitExpressionId::new(document.id(), id);
        assert_eq!(
            unit.ownership().callable_origin(id).unwrap().origin(),
            UnitCallableOrigin::Lambda(id)
        );
    }
    let nested = expression(&unit, "{}");
    let span = unit
        .parsed()
        .ast()
        .expressions()
        .get(nested.expression())
        .unwrap()
        .span();
    assert_eq!((span.start(), span.end()), (70, 72));
    let entry = unit.declaration("entry", SymbolKind::Function);
    let Err(error) = unit.lower_entry(entry) else {
        panic!("the unsupported evaluated nested closure must return no Program");
    };
    assert_eq!(
        (error.kind, error.span),
        (LoweringErrorKind::UnsupportedNode, Some(span))
    );
}
