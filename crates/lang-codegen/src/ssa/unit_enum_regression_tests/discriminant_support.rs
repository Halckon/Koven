//! 原 e01–e04、p49/u01 完整源：普通 Unit 的 tag 支持类型不能改变 when 结果类型。
//! 不调用 legacy Single lowerer，不执行 native 程序；双 CLI 入口另由集成测试验证。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{Expression, LiteralKind, WhenCondition, parse_file},
    source::SourceMap,
    type_checking::{
        BuiltinType, UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::clean;
use crate::ssa::{
    model::{
        ComparisonOperator, EntityId, EntityType, Operation, Origin, ScalarConstant, SsaTypeKind,
        TerminatorKind,
    },
    unit_lower::lower_owned_unit_with_entry,
    verify::verify_program,
};

fn exercise(path: &str, text: &str, result: BuiltinType, integer_literals: usize, cases: usize) {
    let mut sources = SourceMap::new();
    let source = sources.add_source(path, text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    clean(&sources, "lex", lexed.diagnostics()).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    clean(&sources, "parse", parsed.diagnostics()).unwrap();
    assert_eq!(sources.source_text(source), Ok(text));
    let inputs = [SourceUnitInput::new("src", path, source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    clean(&sources, "index", index.diagnostics()).unwrap();
    let names =
        resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment).unwrap();
    clean(&sources, "names", names.diagnostics()).unwrap();
    let names = names.validate().unwrap();
    let [source_unit] = names.names().index().source_units() else {
        panic!("one unchanged original source is required");
    };
    let source_unit = source_unit.id();
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.source_unit() == source_unit
                && declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && parsed.roots().contains(&declaration.root())
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        panic!("unique original main: {entries:?}")
    };
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    clean(&sources, "raw types", raw.diagnostics()).unwrap();
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw));
    }
    let typed = raw.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed).unwrap();
    clean(&sources, "ownership", owned.diagnostics()).unwrap();
    assert!(owned.deferred().is_empty() && owned.is_compatible_with(&typed));
    let owned = owned.validate().unwrap();
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .unwrap();
    assert!(std::ptr::eq(view.types(), typed.types()));
    let types = typed.types();
    let whens = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::When { .. }))
        .collect::<Vec<_>>();
    let [(when_id, when)] = whens.as_slice() else {
        panic!("one original when")
    };
    let when_type = types
        .expression_type(UnitExpressionId::new(source_unit, *when_id))
        .unwrap();
    assert_eq!(Some(when_type), types.types().builtin(result));
    assert_eq!(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                matches!(node.payload(), Expression::Literal(LiteralKind::Integer(_)))
            })
            .count(),
        integer_literals
    );
    if integer_literals == 0 {
        assert!(
            !types
                .expression_types()
                .values()
                .any(|ty| { Some(*ty) == types.types().builtin(BuiltinType::Int) }),
            "the input must not supply Int expression facts to mask missing tag storage"
        );
    }
    if matches!(result, BuiltinType::Unit | BuiltinType::String) {
        assert!(
            !types
                .expression_types()
                .values()
                .any(|ty| { Some(*ty) == types.types().builtin(BuiltinType::Boolean) }),
            "e02/p49 must not supply Boolean expression facts either"
        );
    }
    let (program, driver) = lower_owned_unit_with_entry(&view, entry.id())
        .unwrap_or_else(|error| panic!("{path}: ordinary Unit lowering: {error:?}"));
    verify_program(&program).expect("ordinary SSA verifier");
    crate::llvm::render_verified_program_with_entry(&program, driver)
        .expect("ordinary LLVM verifier");
    let module = program.module(driver.module()).unwrap();
    let integer = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    let boolean = SsaTypeKind::Boolean;
    assert!(module.types.contains(&integer) && module.types.contains(&boolean));
    let Expression::When { entries, .. } = when.payload() else {
        unreachable!()
    };
    assert_eq!(entries.len(), cases);
    for entry in entries {
        let [WhenCondition::Expression(condition)] = entry.conditions.as_slice() else {
            panic!("the original payloadless expression case must remain intact");
        };
        let condition = UnitExpressionId::new(source_unit, *condition);
        let descriptor = types.construction(condition).unwrap();
        let UnitConstructionTarget::EnumCase(symbol) = descriptor.target() else {
            panic!("the same analysis must select an enum case");
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) =
            types.types().get(descriptor.result_type())
        else {
            panic!("nominal root")
        };
        let nominal = types
            .signatures()
            .declaration(*declaration)
            .unwrap()
            .nominal()
            .unwrap();
        let variant = nominal
            .enum_cases()
            .iter()
            .position(|case| case.value_symbol() == symbol && case.payloads().is_empty())
            .unwrap();
        assert!(descriptor.arguments().is_empty());
        let compares = module
            .functions
            .iter()
            .flat_map(|function| {
                function.instructions.iter().filter_map(move |instruction| {
                    (instruction.origin == Origin::Source(entry.span)
                        && matches!(instruction.operation, Operation::Compare { .. }))
                    .then_some((function, instruction))
                })
            })
            .collect::<Vec<_>>();
        let [(function, compare)] = compares.as_slice() else {
            panic!("one actual tag comparison")
        };
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = compare.operation
        else {
            panic!("tag equality")
        };
        let definition = |value| {
            let crate::ssa::model::Definition::InstructionResult { instruction, .. } =
                function.entity(EntityId::Value(value)).unwrap().definition
            else {
                panic!("real SSA instruction result")
            };
            function.instruction(instruction).unwrap()
        };
        let Operation::TaggedDiscriminant { owner } = definition(left).operation else {
            panic!("the comparison reads the subject tag")
        };
        assert_eq!(
            definition(right).operation,
            Operation::Constant(ScalarConstant::Integer(variant as i128))
        );
        for value in [left, right] {
            let ty = function
                .entity(EntityId::Value(value))
                .unwrap()
                .ty
                .semantic_type();
            assert_eq!(module.type_kind(ty), Some(&integer));
        }
        let [EntityId::Value(result)] = compare.results.as_slice() else {
            panic!("Boolean result")
        };
        let ty = function
            .entity(EntityId::Value(*result))
            .unwrap()
            .ty
            .semantic_type();
        assert_eq!(module.type_kind(ty), Some(&boolean));
        let terminator = function
            .block(compare.block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap();
        assert!(
            matches!(terminator.kind, TerminatorKind::Conditional { condition, .. }
            if condition == *result)
        );
        let tagged = function
            .entity(EntityId::Value(owner))
            .unwrap()
            .ty
            .semantic_type();
        assert!(
            matches!(module.type_kind(tagged), Some(SsaTypeKind::TaggedUnion { variants, .. })
            if variants.len() == nominal.enum_cases().len())
        );
        let root_births = module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|instruction| {
                matches!(instruction.operation,
                Operation::TaggedConstruct { tagged: actual, .. } if actual == tagged)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            root_births.len(),
            1,
            "only the original subject construction, not condition values"
        );
        let Origin::Source(birth_span) = root_births[0].origin else {
            panic!("original subject construction source identity")
        };
        let births = types
            .constructions()
            .iter()
            .filter(|birth| {
                birth.expression().source_unit() == source_unit
                    && birth.result_type() == descriptor.result_type()
                    && parsed
                        .ast()
                        .expressions()
                        .get(birth.expression().expression())
                        .is_ok_and(|node| node.span() == birth_span)
            })
            .collect::<Vec<_>>();
        let [birth] = births.as_slice() else {
            panic!("same original nominal root descriptor")
        };
        let UnitConstructionTarget::EnumCase(birth_symbol) = birth.target() else {
            panic!("original subject case descriptor")
        };
        let birth_variant = nominal
            .enum_cases()
            .iter()
            .position(|case| case.value_symbol() == birth_symbol)
            .unwrap();
        assert!(matches!(root_births[0].operation,
            Operation::TaggedConstruct { variant, .. } if variant == birth_variant));
    }
    let joins = module
        .functions
        .iter()
        .flat_map(|function| {
            function.blocks.iter().filter_map(move |block| {
                (block.origin == Origin::Source(when.span())).then_some((function, block))
            })
        })
        .collect::<Vec<_>>();
    let [(function, join)] = joins.as_slice() else {
        panic!("one actual when merge")
    };
    let parameter_types = join
        .parameters
        .iter()
        .map(|id| function.entity(*id).unwrap().ty)
        .collect::<Vec<_>>();
    // merge_unit_exits puts a non-Unit result first, before value/loan carry columns.
    // The original helper parameters may be Borrow Color; they are not result values.
    match result {
        BuiltinType::Boolean | BuiltinType::Int | BuiltinType::String => {
            let Some(EntityType::Value(ty)) = parameter_types.first() else {
                panic!("the actual when result must be the first value column")
            };
            let expected = match result {
                BuiltinType::Boolean => &boolean,
                BuiltinType::Int => &integer,
                BuiltinType::String => &SsaTypeKind::StringOwner,
                _ => unreachable!(),
            };
            assert_eq!(module.type_kind(*ty), Some(expected));
        }
        BuiltinType::Unit => {}
        _ => unreachable!(),
    }
    if result == BuiltinType::String {
        check_string_result(module, function, join, driver);
    }
    for carried in parameter_types
        .iter()
        .skip(usize::from(result != BuiltinType::Unit))
    {
        assert!(matches!(
            carried,
            EntityType::Value(_) | EntityType::Loan { .. }
        ));
        assert!(
            matches!(
                module.type_kind(carried.semantic_type()),
                Some(SsaTypeKind::TaggedUnion { .. })
            ),
            "the remaining original Color carries are not Int/Boolean result columns"
        );
    }
}

fn check_string_result(
    module: &crate::ssa::model::Module,
    function: &crate::ssa::model::Function,
    join: &crate::ssa::model::Block,
    driver: crate::ssa::model::FunctionId,
) {
    use crate::ssa::model::{Definition, Function, Instruction, LoanKind, Ownership};

    fn definition(function: &Function, entity: EntityId) -> &Instruction {
        let Definition::InstructionResult { instruction, .. } =
            function.entity(entity).unwrap().definition
        else {
            panic!("real String route instruction result")
        };
        function.instruction(instruction).unwrap()
    }

    let EntityId::Value(result) = join.parameters[0] else {
        panic!("owned result column")
    };
    let ty = function
        .entity(EntityId::Value(result))
        .unwrap()
        .ty
        .semantic_type();
    assert_eq!(module.type_kind(ty), Some(&SsaTypeKind::StringOwner));
    assert_eq!(module.type_ownership(ty), Some(Ownership::MoveOnly));
    assert_eq!(function.return_types, vec![ty]);
    assert!(matches!(&join.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.as_slice() == [result]));

    let literals = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            let Operation::StringLiteral { string, bytes } = &instruction.operation else {
                return None;
            };
            assert_eq!(*string, ty);
            let [EntityId::Value(owner)] = instruction.results.as_slice() else {
                panic!("literal owner")
            };
            Some((*owner, bytes.as_slice()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        literals.iter().map(|(_, bytes)| *bytes).collect::<Vec<_>>(),
        vec![b"red".as_slice(), b"green".as_slice(), b"blue".as_slice()]
    );
    let incoming = function
        .blocks
        .iter()
        .filter_map(|block| {
            let TerminatorKind::Branch(edge) = &block.terminator.as_ref()?.kind else {
                return None;
            };
            (edge.target == join.id).then_some(edge)
        })
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 3);
    for (owner, _) in &literals {
        assert_eq!(
            incoming
                .iter()
                .filter(|edge| { edge.arguments.first() == Some(&EntityId::Value(*owner)) })
                .count(),
            1,
            "each original branch transfers its unique String owner once"
        );
        assert!(!function.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == *owner)
        }), "the returned branch owner is not dropped by its producer");
    }
    assert!(
        !function.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::Drop { owner } if owner == result)
        }),
        "the merged String owner is returned, not consumed locally"
    );

    let caller = module.function(driver).unwrap();
    let calls = caller.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::DirectCall { callee, .. } if callee == function.id)
    }).collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one real main call to name")
    };
    let [EntityId::Value(returned)] = call.results.as_slice() else {
        panic!("one returned owner")
    };
    assert_eq!(
        caller.entity(EntityId::Value(*returned)).unwrap().ty,
        EntityType::Value(ty)
    );
    let prints = caller
        .instructions
        .iter()
        .filter_map(|instruction| {
            let Operation::PrintString { value } = instruction.operation else {
                return None;
            };
            Some((instruction, value))
        })
        .collect::<Vec<_>>();
    let [(print, loan)] = prints.as_slice() else {
        panic!("one real println")
    };
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = definition(caller, EntityId::Loan(*loan)).operation
    else {
        panic!("shared print loan")
    };
    assert!(
        matches!(definition(caller, EntityId::Place(place)).operation,
        Operation::RootPlace { owner } if owner == *returned)
    );
    let ends = caller.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == *loan)
    }).collect::<Vec<_>>();
    let drops = caller.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == *returned)
    }).collect::<Vec<_>>();
    let [end] = ends.as_slice() else {
        panic!("one print-loan end")
    };
    let [drop] = drops.as_slice() else {
        panic!("one drop of the same returned String owner")
    };
    assert!(
        call.id.index() < print.id.index()
            && print.id.index() < end.id.index()
            && end.id.index() < drop.id.index()
    );
}

#[test]
fn enum_boolean_return_keeps_boolean_result_without_int_source_facts() {
    exercise(
        "e01_enum_when_bool_return.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/e01_enum_when_bool_return.ko"
        ),
        BuiltinType::Boolean,
        0,
        2,
    );
}

#[test]
fn enum_statement_registers_support_types_without_scalar_source_facts() {
    exercise(
        "e02_enum_when_stmt_println.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/e02_enum_when_stmt_println.ko"
        ),
        BuiltinType::Unit,
        0,
        2,
    );
}

#[test]
fn enum_boolean_local_keeps_boolean_result_without_int_source_facts() {
    exercise(
        "e03_enum_when_bool_val.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/e03_enum_when_bool_val.ko"
        ),
        BuiltinType::Boolean,
        0,
        2,
    );
}

#[test]
fn enum_int_return_remains_the_original_control() {
    exercise(
        "e04_enum_when_int_return.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/e04_enum_when_int_return.ko"
        ),
        BuiltinType::Int,
        3,
        2,
    );
}

#[test]
fn enum_string_expression_body_transfers_the_original_result_owner() {
    exercise(
        "p49_when_enum_no_payload.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/p49_when_enum_no_payload.ko"
        ),
        BuiltinType::String,
        0,
        3,
    );
}

#[test]
fn enum_boolean_expression_body_keeps_the_original_borrow_argument() {
    exercise(
        "u01_enum_case_borrow_arg.ko",
        include_str!(
            "../../../tests/fixtures/unit-enum-discriminant-0341/u01_enum_case_borrow_arg.ko"
        ),
        BuiltinType::Boolean,
        0,
        2,
    );
}
