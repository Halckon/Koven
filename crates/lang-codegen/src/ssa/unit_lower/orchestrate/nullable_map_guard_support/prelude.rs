//! Closed six-source prelude oracle: source binding transitions and actual SSA liveness.
//! This accepts only construction, two C2 puts with literal clones, and a Move initializer.
use super::*;
use crate::ssa::model::{Definition, Instruction};
use lang_frontend::type_checking::StringOperationKind;
use std::collections::BTreeSet;

pub(super) fn recover(
    fixture: &Fixture,
    lowerer: &UnitExpressionLowerer<'_>,
    checkpoint: &Checkpoint,
) -> BTreeMap<UnitSymbolId, LoweredValue> {
    let parsed = fixture.parsed();
    let key = lowerer.source_plan.source(lowerer.source_token).unwrap();
    let UnitCallableTarget::Declaration(target) = key.target() else {
        panic!("ordinary function")
    };
    let declaration = &fixture.names.names().index().declarations()[target.index()];
    let (_, item, _) = unwrap_modified(parsed, declaration.root()).unwrap();
    let Item::Function { form, .. } = item else {
        panic!("source function")
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => body,
        _ => panic!("fixed ordinary block body"),
    };
    let Statement::Block { elements } = parsed.ast().statements().get(body).unwrap().payload()
    else {
        panic!("source block")
    };
    let positions = elements.iter().enumerate().filter_map(|(index, id)| {
        let node = parsed.ast().statements().get(*id).unwrap();
        matches!(node.payload(), Statement::Expression { expression } if *expression == checkpoint.control.expression()).then_some(index)
    }).collect::<Vec<_>>();
    let [position] = positions.as_slice() else {
        panic!("actual direct top-level If")
    };
    let prefix = &elements[..*position];
    assert_eq!(prefix.len(), if fixture.id == "C2" { 4 } else { 1 });
    let function = fixture.function();
    let header = function.block(checkpoint.header).unwrap();
    let instructions = header
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let mut bindings = lowerer.bindings.clone();
    if fixture.id == "C3" {
        assert_eq!(
            bindings,
            BTreeMap::from([(
                fixture.symbol("input"),
                LoweredValue::Value(checkpoint.header_owner)
            )])
        );
    } else {
        assert!(bindings.is_empty());
    }
    let entry_bindings = bindings.clone();
    let mut puts = 0;
    for statement in prefix {
        let node = parsed.ast().statements().get(*statement).unwrap();
        match node.payload() {
            Statement::LocalVariable { declaration } => {
                let (_, item, _) = unwrap_modified(parsed, *declaration).unwrap();
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = item
                else {
                    panic!("actual named local")
                };
                let root = lowerer
                    .declaration_symbol(name, SymbolKind::Variable)
                    .unwrap();
                let declared = lowerer.typed.symbol_type(root).unwrap();
                assert_eq!(
                    lowerer.typed.copyability(declared),
                    Copyability::MoveOnly,
                    "no hidden Copyable bindings in this prelude"
                );
                let id = UnitExpressionId::new(fixture.source, initializer);
                let actual = lowerer.typed.expression_type(id).unwrap();
                assert_eq!(lowerer.typed.copyability(actual), Copyability::MoveOnly);
                let initializer_node = parsed.ast().expressions().get(initializer).unwrap();
                let value = if let Some(descriptor) = lowerer.typed.map_construction(id) {
                    assert_eq!(descriptor.expression(), id);
                    let Expression::Call { arguments, .. } = initializer_node.payload() else {
                        panic!("empty map constructor")
                    };
                    assert!(arguments.is_empty());
                    let instruction = one_at(&instructions, initializer_node.span(), |op| {
                        matches!(op, Operation::MapConstruct { .. })
                    });
                    value(instruction)
                } else {
                    assert!(matches!(initializer_node.payload(), Expression::Name));
                    let source = lowerer.references[&span_key(initializer_node.span())];
                    assert_eq!(
                        source,
                        fixture.symbol(if fixture.id == "C2" { "seed" } else { "input" })
                    );
                    assert_eq!(root, checkpoint.root);
                    // The original lower_local_variable calls transfer_owned_expression
                    // for exact-type Move values and before NullableWrap for adaptation.
                    let Some(LoweredValue::Value(value)) = bindings.remove(&source) else {
                        panic!("Move initializer consumes its live named source")
                    };
                    value
                };
                let delivered = if declared == actual {
                    value
                } else {
                    assert_eq!(declared, checkpoint.storage);
                    assert_eq!(actual, checkpoint.narrowed);
                    let wrap = one_at(&instructions, node.span(), |op| {
                        matches!(op, Operation::NullableWrap { .. })
                    });
                    assert_eq!(
                        wrap.operation,
                        Operation::NullableWrap {
                            nullable: checkpoint.nullable,
                            owner: value
                        }
                    );
                    self::value(wrap)
                };
                assert!(
                    bindings
                        .insert(root, LoweredValue::Value(delivered))
                        .is_none()
                );
            }
            Statement::Expression { expression } => {
                assert_eq!(fixture.id, "C2");
                let id = UnitExpressionId::new(fixture.source, *expression);
                let descriptor = lowerer.typed.map_put(id).unwrap();
                let put_node = parsed.ast().expressions().get(*expression).unwrap();
                let Expression::Call {
                    callee,
                    arguments,
                    type_arguments,
                    ..
                } = put_node.payload()
                else {
                    panic!("actual complete put statement")
                };
                assert!(type_arguments.is_empty());
                assert_eq!(arguments.len(), 2);
                assert_eq!(arguments[0].value, descriptor.key().expression());
                assert_eq!(arguments[1].value, descriptor.value().expression());
                assert!(
                    matches!(parsed.ast().expressions().get(*callee).unwrap().payload(),
                    Expression::Member { receiver, safe: false, .. } if *receiver == descriptor.receiver().expression())
                );
                let receiver_node = parsed
                    .ast()
                    .expressions()
                    .get(descriptor.receiver().expression())
                    .unwrap();
                assert!(matches!(receiver_node.payload(), Expression::Name));
                let seed = lowerer.references[&span_key(receiver_node.span())];
                assert_eq!(seed, fixture.symbol("seed"));
                let LoweredValue::Value(owner) = bindings[&seed] else {
                    panic!("live seed")
                };
                for argument in [descriptor.key(), descriptor.value()] {
                    let clone = lowerer.typed.string_operation(argument).unwrap();
                    assert_eq!(clone.kind(), StringOperationKind::Clone);
                    let call = parsed
                        .ast()
                        .expressions()
                        .get(argument.expression())
                        .unwrap();
                    let Expression::Call { arguments, .. } = call.payload() else {
                        panic!("literal clone call")
                    };
                    assert!(arguments.is_empty());
                    let literal = parsed
                        .ast()
                        .expressions()
                        .get(clone.receiver().expression())
                        .unwrap();
                    assert!(matches!(literal.payload(), Expression::String { .. }));
                    one_at(&instructions, literal.span(), |op| {
                        matches!(op, Operation::StringLiteral { .. })
                    });
                    one_at(&instructions, call.span(), |op| {
                        matches!(op, Operation::StringClone { .. })
                    });
                }
                let put_span = parsed.ast().expressions().get(*expression).unwrap().span();
                let put = one_at(&instructions, put_span, |op| {
                    matches!(op, Operation::MapPut { .. })
                });
                assert!(
                    matches!(put.operation, Operation::MapPut { owner: actual, .. } if actual == owner)
                );
                bindings.insert(seed, LoweredValue::Value(value(put)));
                puts += 1;
            }
            _ => panic!("no loop/capture/receiver/control/outer call in the closed source prelude"),
        }
    }
    assert_eq!(puts, if fixture.id == "C2" { 2 } else { 0 });
    assert_eq!(
        bindings,
        BTreeMap::from([(
            checkpoint.root,
            LoweredValue::Value(checkpoint.header_owner)
        )])
    );
    assert_prefix_liveness(function, &instructions, &entry_bindings, &bindings);
    bindings
}

fn one_at<'a>(
    instructions: &[&'a Instruction],
    span: Span,
    operation: impl Fn(&Operation) -> bool,
) -> &'a Instruction {
    let matches = instructions
        .iter()
        .copied()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span) && operation(&instruction.operation)
        })
        .collect::<Vec<_>>();
    let [instruction] = matches.as_slice() else {
        panic!("unique actual prelude instruction at source span")
    };
    instruction
}
fn value(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("one real owner result")
    };
    *value
}

fn assert_prefix_liveness(
    function: &Function,
    instructions: &[&Instruction],
    entry: &BTreeMap<UnitSymbolId, LoweredValue>,
    bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
) {
    let owners = |bindings: &BTreeMap<UnitSymbolId, LoweredValue>| {
        bindings
            .values()
            .map(|binding| {
                let LoweredValue::Value(value) = binding else {
                    panic!("only Move owner bindings")
                };
                *value
            })
            .collect::<BTreeSet<_>>()
    };
    let mut live = owners(entry);
    let mut loans = BTreeSet::new();
    for instruction in instructions {
        match instruction.operation {
            Operation::MapConstruct { .. } | Operation::StringLiteral { .. } => {
                assert!(live.insert(value(instruction)));
            }
            Operation::RootPlace { owner } => {
                assert!(live.contains(&owner));
                assert!(matches!(
                    instruction.results.as_slice(),
                    [EntityId::Place(_)]
                ));
            }
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } => {
                let Definition::InstructionResult {
                    instruction: root,
                    index: 0,
                } = function.entity(EntityId::Place(place)).unwrap().definition
                else {
                    panic!("actual literal root place")
                };
                assert!(
                    matches!(function.instruction(root).unwrap().operation, Operation::RootPlace { owner } if live.contains(&owner))
                );
                let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                    panic!("actual shared clone loan")
                };
                assert!(loans.insert(*loan));
            }
            Operation::StringClone { source } => {
                assert!(loans.contains(&source));
                assert!(live.insert(value(instruction)));
            }
            Operation::BorrowEnd { loan } => {
                assert!(
                    loans.remove(&loan),
                    "every loan ends exactly once inside completed prelude"
                );
            }
            Operation::Drop { owner } => {
                assert!(live.remove(&owner));
            }
            Operation::MapPut {
                owner,
                key,
                value: element,
            } => {
                for moved in [owner, key, element] {
                    assert!(
                        live.remove(&moved),
                        "exact evaluated put owner/key/value delivery"
                    );
                }
                assert!(live.insert(value(instruction)));
            }
            Operation::NullableWrap { owner, .. } => {
                assert!(live.remove(&owner));
                assert!(live.insert(value(instruction)));
            }
            _ => panic!(
                "no unaccounted call/carrier/capture/range/control operation in the fixed prefix"
            ),
        }
    }
    assert!(
        loans.is_empty(),
        "all actual header loans retired before producer"
    );
    assert_eq!(
        live,
        owners(bindings),
        "no dangling temporary or pending owner remains"
    );
    // The accepted AST contains only completed intrinsic statements. Neither source
    // functions nor closures, receiver methods, loops, or argument-control contexts
    // occur here; production lower_map_put truncates its pending operand and clone
    // ends its local created loans before returning. All their owners/loans were
    // independently accounted for above, so no frame or carrier is silently dropped.
}
