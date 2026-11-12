//! SPEC-0299: ordinary-source non-consuming Map result Borrow, without IR mutation.
use lang_frontend::{
    name_resolution::{SourceUnitInput, SymbolKind},
    source::SourceMap,
    type_checking::standard_environments,
};

fn check(payload: &str, grouped: bool, compilation_unit: bool) {
    let source = crate::native_owned_map_borrow_cases::core(payload, grouped);
    let program = lower(&source, compilation_unit);
    assert_core(&program);
}

pub(super) fn lower(source: &str, compilation_unit: bool) -> super::model::Program {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if compilation_unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let (p, provider) = super::unit_lower_test_support::parsed(
            &mut sources,
            "provider.ko",
            &format!("package p\n{provider}"),
        );
        let (q, entry) = super::unit_lower_test_support::parsed(
            &mut sources,
            "entry.ko",
            &format!("package q\nimport p.*\nfun entry(){entry}"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p, &provider),
            SourceUnitInput::new("root", "q/entry.ko", q, &entry),
        ];
        let (names, typed, owned) =
            super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        super::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            super::unit_lower_test_support::declaration(&names, "q", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

        // Keep the original one-file bytes and display path; only the phase entry changes.
        let analysis = UnitSource::analyze("result.ko", source).unwrap();
        analysis.assert_frontend_clean();
        let declaration = analysis.declaration("entry", SymbolKind::Function);
        let (program, entry) = analysis.lower_entry(declaration).unwrap_or_else(|error| {
            let origin = error.span.map(|span| {
                (
                    analysis.sources().source_name(span.source_id()).unwrap(),
                    analysis.sources().slice(span).unwrap(),
                )
            });
            panic!("unit={compilation_unit}: {error:?}; origin={origin:?}")
        });
        assert_eq!(
            program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap()
                .origin,
            super::model::Origin::Source(analysis.declaration_span(declaration))
        );
        Ok(program)
    };
    let program = result.unwrap_or_else(|error| {
        let origin = error.span.map(|span| {
            (
                sources.source_name(span.source_id()).unwrap(),
                sources.slice(span).unwrap(),
            )
        });
        panic!("unit={compilation_unit}: {error:?}; origin={origin:?}")
    });
    super::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    program
}

macro_rules! case {
    ($name:ident, $payload:literal, $grouped:literal, $unit:literal) => {
        #[test]
        fn $name() {
            check($payload, $grouped, $unit);
        }
    };
}
case!(owned_map_borrow_single_string_plain, "String", false, false);
case!(owned_map_borrow_single_string_group, "String", true, false);
case!(owned_map_borrow_unit_string_plain, "String", false, true);
case!(owned_map_borrow_unit_string_group, "String", true, true);
case!(owned_map_borrow_single_inline_plain, "Text", false, false);
case!(owned_map_borrow_single_inline_group, "Text", true, false);
case!(owned_map_borrow_unit_inline_plain, "Text", false, true);
case!(owned_map_borrow_unit_inline_group, "Text", true, true);
case!(
    owned_map_borrow_single_resource_plain,
    "Resource",
    false,
    false
);
case!(
    owned_map_borrow_single_resource_group,
    "Resource",
    true,
    false
);
case!(
    owned_map_borrow_unit_resource_plain,
    "Resource",
    false,
    true
);
case!(owned_map_borrow_unit_resource_group, "Resource", true, true);

use super::model::*;
fn entry(program: &Program) -> &Function {
    program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name.contains("entry"))
        .expect("ordinary entry function")
}
fn assert_core(program: &Program) {
    let entry = entry(program);
    let calls: Vec<_> = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect();
    assert!(calls.len() >= 2);
    for call in &calls[..2] {
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("non-consuming loan argument: {arguments:?}")
        };
        assert!(matches!(
            entry.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        assert!(entry.blocks.iter().any(|block| matches!(block.terminator.as_ref().map(|term| &term.kind), Some(TerminatorKind::NullableBranch { view, .. }) if view == loan)), "caller loan comes from nullable proof, not copied payload");
    }
    let roots: Vec<_> = calls[..2]
        .iter()
        .map(|call| {
            let Operation::DirectCall { arguments, .. } = &call.operation else {
                unreachable!()
            };
            remove_roots(entry, arguments[0])
        })
        .collect();
    assert_eq!(roots[0].len(), 1);
    assert_eq!(
        roots[0], roots[1],
        "both loans depend on the same removed owner"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::StringClone { .. }))
            .count(),
        1,
        "only the explicit source payload clone exists"
    );
    assert!(
        !entry
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::MapResultUnwrap { .. })),
        "MoveOnly Borrow does not use the Copyable bridge"
    );
    let first = calls[0].id.index();
    let second = calls[1].id.index();
    let takes: Vec<_> = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
        .collect();
    assert_eq!(
        takes.len(),
        1,
        "only explicit post-Borrow !! consumes owner"
    );
    assert!(takes[0].id.index() > second);
    assert!(
        !entry.instructions[first..=second]
            .iter()
            .any(|instruction| matches!(
                instruction.operation,
                Operation::Copy { .. }
                    | Operation::StringClone { .. }
                    | Operation::MapResultUnwrap { .. }
                    | Operation::NullableTake { .. }
            )),
        "Borrow bridge does not copy/clone/take"
    );
}

#[test]
fn owned_map_borrow_scope_prefix_and_last_use_ssa() {
    use crate::native_owned_map_borrow_cases as cases;
    for unit in [false, true] {
        for source in [
            cases::PAIR,
            cases::BORROWED_PREFIX,
            cases::SCOPES,
            cases::MISSING,
            cases::RESOURCE_SCOPE,
            cases::ABORT,
        ] {
            lower(source, unit);
        }
        for payload in ["String", "Text", "Resource"] {
            let program = lower(&cases::last_use(payload), unit);
            assert!(
                !entry(&program)
                    .instructions
                    .iter()
                    .any(|instruction| matches!(
                        instruction.operation,
                        Operation::NullableTake { .. }
                    ))
            );
        }
        lower(&cases::independent_map(), unit);
        for payload in ["String", "Text", "Resource"] {
            lower(&cases::missing(payload, false), unit);
            lower(&cases::missing(payload, true), unit);
        }
        lower(&cases::inline_field(unit), unit);
        lower(&cases::inline_explicit_clone(), unit);
    }
}

fn remove_roots(function: &Function, start: EntityId) -> std::collections::BTreeSet<InstructionId> {
    let mut pending = vec![start];
    let mut seen = std::collections::BTreeSet::new();
    let mut roots = std::collections::BTreeSet::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        match function.entity(entity).unwrap().definition {
            Definition::InstructionResult { instruction, index } => {
                assert_eq!(index, 1, "the removed payload is result 1");
                assert!(matches!(
                    function.instruction(instruction).unwrap().operation,
                    Operation::MapRemove { .. }
                ));
                roots.insert(instruction);
            }
            Definition::BlockParameter { block, index } => {
                let mut found = false;
                for predecessor in &function.blocks {
                    let Some(term) = &predecessor.terminator else {
                        continue;
                    };
                    if let TerminatorKind::NullableBranch { owner, view, .. } = term.kind
                        && entity == EntityId::Loan(view)
                    {
                        pending.push(EntityId::Value(owner));
                        found = true;
                        continue;
                    }
                    let edges = match &term.kind {
                        TerminatorKind::Branch(edge) => vec![edge],
                        TerminatorKind::Conditional {
                            when_true,
                            when_false,
                            ..
                        } => vec![when_true, when_false],
                        TerminatorKind::NullableBranch {
                            when_null,
                            when_non_null,
                            ..
                        } => vec![when_null, when_non_null],
                        _ => vec![],
                    };
                    for edge in edges {
                        if edge.target == block
                            && let Some(incoming) = edge.arguments.get(index)
                        {
                            pending.push(*incoming);
                            found = true;
                        }
                    }
                }
                assert!(found, "proof/owner must have actual source transport");
            }
        }
    }
    roots
}

#[test]
fn owned_map_borrow_last_use_drop_points_and_pending_loan_transport() {
    use crate::native_owned_map_borrow_cases as cases;
    for unit in [false, true] {
        let pair = lower(cases::PAIR, unit);
        let entry = entry(&pair);
        let call = entry
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.operation {
                Operation::DirectCall { arguments, .. } if arguments.len() == 3 => Some(arguments),
                _ => None,
            })
            .unwrap();
        assert!(matches!(call[0], EntityId::Value(_)));
        assert!(matches!(call[1], EntityId::Loan(_)) && matches!(call[2], EntityId::Loan(_)));
        assert_eq!(
            remove_roots(entry, call[1]),
            remove_roots(entry, call[2]),
            "first pending proof survives the second proof branch"
        );
        for payload in ["String", "Text", "Resource"] {
            let program = lower(&cases::last_use(payload), unit);
            let function = self::entry(&program);
            let result = function
                .instructions
                .iter()
                .find(|i| matches!(i.operation, Operation::MapRemove { .. }))
                .unwrap()
                .results[1];
            let ty = function.entity(result).unwrap().ty;
            let drops: Vec<_> = function
                .instructions
                .iter()
                .filter_map(|instruction| match instruction.operation {
                    Operation::Drop { owner }
                        if function.entity(EntityId::Value(owner)).unwrap().ty == ty =>
                    {
                        assert_eq!(
                            remove_roots(function, EntityId::Value(owner)),
                            remove_roots(function, result)
                        );
                        Some(instruction.id.index())
                    }
                    _ => None,
                })
                .collect();
            let end = function
                .instructions
                .iter()
                .find(|i| {
                    matches!(&i.operation, Operation::PrintLiteral { bytes } if bytes == b"end\n")
                        || matches!(i.operation, Operation::PrintString { .. })
                })
                .unwrap()
                .id
                .index();
            assert!(!drops.is_empty());
            assert!(
                drops.iter().all(|drop| if payload == "Resource" {
                    *drop > end
                } else {
                    *drop < end
                }),
                "unit={unit}/{payload}: preserve original drop point; drops={drops:?}, end={end}"
            );
        }
    }
}

pub(in crate::ssa) mod unit_lower_container_element_tests {
use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, PlaceAccess, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_container_element_read_borrow_and_replacement_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun make(): Resource = Resource()\n\
         fun inspect(item: Resource): Int = 1\n\
         fun takeInt(own item: Int): Int = item\n\
         fun forward(items: List<Resource>, index: Int): Int = inspect(items[index])",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val resources = listOf(p.make(), p.make())\n\
             val numbers = mutableListOf(1, 2)\n\
             val first = p.inspect(resources[0])\n\
             val second = p.forward(resources, 1)\n\
             val copied = p.takeInt(numbers[0])\n\
             val write = (numbers[0] = 3)\n\
             val compound = (numbers[1] += 4)\n\
             val array = arrayOf(5, 6)\n\
             val arrayWrite = (array[0] = 7)\n\
             val replaced = mutableListOf(p.make())\n\
             val moveWrite = (replaced[0] = p.make())\n\
             val after = p.inspect(replaced[0])\n\
             val temporary = p.inspect(listOf(p.make())[0])\n\
             val groupedTemporary = p.inspect((listOf(p.make()))[0])\n\
             return first + second + copied + numbers[0] + array[0] + after + temporary + groupedTemporary\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("container element core lowers to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves container element identities");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let forwarder = function(module, "p.forward");
    let EntityId::Loan(items) = forwarder.blocks[0].parameters[0] else {
        panic!("Borrow container parameter must be a function-scoped loan");
    };
    assert!(forwarder.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(owner),
            ..
        } if owner == items
    )));

    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_element_place), 8);
    assert_eq!(operation_count(entry, is_replace), 4);
    assert_eq!(operation_count(entry, is_place_read), 4);
    assert_eq!(operation_count(entry, is_borrow_begin), 6);
    assert!(entry.instructions.windows(2).any(|instructions| matches!(
        (&instructions[0].operation, &instructions[1].operation),
        (
            Operation::DirectCall { .. },
            Operation::ContainerReplace { .. }
        )
    )));
    let (pred_block_idx, failure, success) = entry
        .blocks
        .iter()
        .enumerate()
        .find_map(|(idx, block)| {
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref()?.kind
            else {
                return None;
            };
            let success = entry.block(when_false.target)?;
            success
                .instructions
                .iter()
                .filter_map(|instruction| entry.instruction(*instruction))
                .any(|instruction| {
                    matches!(instruction.operation, Operation::ContainerReplace { .. })
                })
                .then_some((idx, when_true, when_false))
        })
        .expect("compound replacement has one checked success edge");
    assert!(!failure.arguments.is_empty());
    assert_eq!(failure.arguments.len(), success.arguments.len());
    let success_block = entry.block(success.target).expect("success block exists");
    let failure_block = entry.block(failure.target).expect("failure block exists");
    assert_eq!(success.arguments.len(), success_block.parameters.len());
    assert_eq!(failure.arguments.len(), failure_block.parameters.len());
    for (arg, param) in success.arguments.iter().zip(&success_block.parameters) {
        assert_eq!(
            entry.entity(*arg).unwrap().ty,
            entry.entity(*param).unwrap().ty
        );
    }
    for (arg, param) in failure.arguments.iter().zip(&failure_block.parameters) {
        assert_eq!(
            entry.entity(*arg).unwrap().ty,
            entry.entity(*param).unwrap().ty
        );
    }
    let (carried_param_idx, carried_owner, replace_value) = success_block
        .parameters
        .iter()
        .enumerate()
        .find_map(|(idx, param)| {
            let EntityId::Value(val) = param else {
                return None;
            };
            success_block
                .instructions
                .iter()
                .filter_map(|instruction| entry.instruction(*instruction))
                .find_map(|instruction| match instruction.operation {
                    Operation::ContainerReplace { owner, value, .. } if owner == *val => {
                        Some((idx, *val, value))
                    }
                    _ => None,
                })
        })
        .expect("checked success carries the live MoveOnly container owner");
    assert!(
        success_block
            .instructions
            .iter()
            .filter_map(|instruction| entry.instruction(*instruction))
            .any(|instruction| matches!(
                instruction.operation,
                Operation::ContainerReplace { owner, value, .. }
                    if owner == carried_owner && value == replace_value
            ))
    );

    // 逐边绑定 carried owner 与前驱实参，验证身份与未提前消费
    let pred_arg_success = success.arguments[carried_param_idx];
    let pred_arg_failure = failure.arguments[carried_param_idx];
    assert_eq!(
        pred_arg_success, pred_arg_failure,
        "carried owner edge argument must match across both branches"
    );
    let EntityId::Value(pred_owner_val) = pred_arg_success else {
        panic!("carried owner edge argument must be a value");
    };
    assert!(
        entry.entity(EntityId::Value(pred_owner_val)).is_some(),
        "carried owner must be a valid live entity in entry"
    );

    // 验证前驱块在跳转前未提前 drop 或 consume 该 owner
    let pred_block = &entry.blocks[pred_block_idx];
    for inst_id in &pred_block.instructions {
        if let Some(inst) = entry.instruction(*inst_id) {
            match inst.operation {
                Operation::Drop { owner } => {
                    assert_ne!(
                        owner, pred_owner_val,
                        "carried owner was dropped prematurely in predecessor block"
                    );
                }
                Operation::Consume { owner } => {
                    assert_ne!(
                        owner, pred_owner_val,
                        "carried owner was consumed prematurely in predecessor block"
                    );
                }
                _ => {}
            }
        }
    }

    // 反例验证：若将 carried owner 替换为同类型但已消费/错误的 owner，SSA 验证必定失败
    let mut bad_program = backward;
    let bad_module = &mut bad_program.modules[0];
    let bad_entry = bad_module
        .functions
        .iter_mut()
        .find(|f| f.name == entry.name)
        .expect("entry found in backward");
    let bad_pred_block = &mut bad_entry.blocks[pred_block_idx];
    if let Some(terminator) = &mut bad_pred_block.terminator
        && let TerminatorKind::Conditional { when_false, .. } = &mut terminator.kind
    {
        when_false.arguments[carried_param_idx] = EntityId::Value(crate::ssa::model::ValueId {
            function: bad_entry.id,
            index: usize::MAX,
        });
    }
    let bad_verify = crate::ssa::verify::verify_program(&bad_program);
    assert!(
        bad_verify.is_err(),
        "passing an invalid owner across replacement edge must fail SSA verification"
    );
}

#[test]
fn field_backed_elements_and_temporary_compound_remain_atomic_boundaries() {
    for (path, source) in [
        (
            "test/field.ko",
            "package test\n\
             class Resource {}\n\
             class Holder(val items: List<Resource>)\n\
             fun inspect(item: Resource): Int = 1\n\
             fun entry(own holder: Holder): Int = inspect(holder.items[0])",
        ),
        (
            "test/temporary-compound.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val result = (mutableListOf(1)[0] += 2)\n\
             }",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source_id, parsed) = parsed(&mut sources, path, source);
        let inputs = [SourceUnitInput::new("root", path, source_id, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let error = match lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        ) {
            Ok(_) => panic!("{path} remains outside the root container element core"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{path}");
    }
}

#[test]
fn named_nested_element_borrow_passes_p3_but_keeps_the_p4_receiver_boundary() {
    use lang_frontend::{
        ownership_checking::{ElementIndexIdentity, LoanKind, UnitLoanTarget},
        type_checking::complete_compilation_unit_types,
    };

    let mut sources = SourceMap::new();
    let text = "package test\n\
                fun inspect(n: Int): Int = 1\n\
                fun entry(own xs: List<List<Int>>): Int = inspect(xs[0][1])";
    let (source, parsed) = parsed(&mut sources, "test/nested.ko", text);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/nested.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, typed.types())
        .expect("legal nested Borrow also passes the final type gate");
    let [loan] = owned.ownership().loans() else {
        panic!("actual P3 publishes exactly the nested Borrow");
    };
    assert_eq!(loan.kind(), LoanKind::Shared);
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("named container keeps its place");
    };
    assert_eq!(place.root().source_unit(), loan.argument().source_unit());
    assert_eq!(
        place.elements(),
        &[
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(1)
        ]
    );
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("P3 path support does not grant a nested P4 container owner"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let receiver = text.find("xs[0]").unwrap();
    assert_eq!(
        error.span,
        Some(
            sources
                .span(source, receiver, receiver + "xs[0]".len())
                .unwrap()
        )
    );
}

fn function<'a>(module: &'a super::super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name} exists"))
}

fn operation_count(function: &Function, predicate: fn(&Operation) -> bool) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .count()
}

fn is_element_place(operation: &Operation) -> bool {
    matches!(operation, Operation::ContainerElementPlace { .. })
}

fn is_replace(operation: &Operation) -> bool {
    matches!(operation, Operation::ContainerReplace { .. })
}

fn is_place_read(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Read {
            source: PlaceAccess::Place(_)
        }
    )
}

fn is_borrow_begin(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowBegin { .. })
}
}
