//! 原 q05：真正的 nullable Borrow temporary 必须转移 wrapper owner，而非改写 place 类型。
use lang_frontend::{
    name_resolution::{SourceUnitInput, SymbolKind},
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        owned_compilation_unit_view,
    },
    source::{SourceMap, Span},
    type_checking::{
        Copyability, ExpressionCategory, ParameterMode, UnitCallTarget, UnitTypeKind,
        complete_compilation_unit_types, standard_environments,
    },
};

use crate::ssa::{
    model::{
        Definition, EntityId, EntityType, Function, FunctionId, Instruction, LoanKind, Module,
        Operation, Program, ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
    },
    unit_lower::lower_owned_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
    verify::verify_program,
};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/pr120-verifier-diagnostics/q05_nullable_param.ko"
));
const STANDARD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../lang-std/koven/algorithms/ranges.ko"
));

#[test]
fn original_q05_nullable_temporary_borrow_has_one_wrapped_owner_in_both_layouts() {
    assert_eq!(SOURCE.len(), 163, "完整原源，不替换为更弱的等价例子");
    for root in ["single", "src"] {
        let mut sources = SourceMap::new();
        let path = "q05_nullable_param.ko";
        let (source, user) = parsed(&mut sources, path, SOURCE);
        let standard_path = "koven/algorithms/ranges.ko";
        let (standard, library) = parsed(&mut sources, standard_path, STANDARD);
        let inputs = [
            SourceUnitInput::new(root, path, source, &user),
            SourceUnitInput::new("koven-std", standard_path, standard, &library),
        ];
        let (name_environment, mut environment) = standard_environments();
        environment
            .authorize_range_source(&sources, standard)
            .expect("同一真实标准库来源");
        environment
            .authorize_range_extension_source(&sources, standard)
            .expect("只授权标准库 extension 来源");
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, typed.types())
                .expect("完整类型接纳");
        assert!(complete.is_same_analysis(typed.types()));
        assert!(owned.ownership().deferred().is_empty());
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .expect("同轮 sealed owned view");
        assert!(std::ptr::eq(view.types(), typed.types()));
        assert!(std::ptr::eq(view.ownership(), owned.ownership()));
        let source_units = names
            .names()
            .index()
            .source_units()
            .iter()
            .filter(|unit| unit.source_id() == source)
            .collect::<Vec<_>>();
        let [source_unit] = source_units.as_slice() else {
            panic!("原源应有唯一 source-qualified unit")
        };
        let declaration = |name: &str| {
            let matches = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|item| {
                    item.source_unit() == source_unit.id()
                        && item.kind() == SymbolKind::Function
                        && item.name() == name
                })
                .collect::<Vec<_>>();
            let [item] = matches.as_slice() else {
                panic!("原源应有唯一 {name}")
            };
            *item
        };
        let main = declaration("main");
        let show = declaration("show");
        let signature = view
            .types()
            .signatures()
            .declaration(show.id())
            .and_then(|declaration| declaration.callable())
            .expect("真实 show 签名");
        let [parameter] = signature.parameters() else {
            panic!("show 保持一个 Borrow 参数")
        };
        assert_eq!(parameter.mode(), ParameterMode::Borrow);
        let Some(UnitTypeKind::Nullable(inner)) = view.types().types().get(parameter.ty()) else {
            panic!("声明参数确为 Node?，不是 Node")
        };
        assert_eq!(view.types().copyability(*inner), Copyability::MoveOnly);
        assert_eq!(
            view.types().copyability(parameter.ty()),
            Copyability::MoveOnly
        );
        for (begin, end, text, call_text, non_null) in [
            (140, 147, "Node(1)", "show(Node(1))", true),
            (155, 159, "null", "show(null)", false),
        ] {
            let argument_span = sources.span(source, begin, end).expect("原 source span");
            assert_eq!(sources.slice(argument_span).expect("实参源码"), text);
            let loans = view
                .ownership()
                .loans()
                .iter()
                .filter(|fact| fact.begin_span() == argument_span)
                .collect::<Vec<_>>();
            let [loan] = loans.as_slice() else {
                panic!("每个原实参具有唯一真实 loan fact")
            };
            assert_eq!(loan.kind(), FrontendLoanKind::Shared);
            assert_eq!(loan.argument().source_unit(), source_unit.id());
            assert_eq!(loan.call().source_unit(), source_unit.id());
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(loan.parameter_span(), Some(parameter.span()));
            assert_eq!(sources.slice(loan.end_span()).expect("调用源码"), call_text);
            assert_eq!(
                view.types().expression_category(loan.argument()),
                Some(ExpressionCategory::Temporary)
            );
            assert_eq!(
                view.types().expression_type(loan.argument()),
                Some(if non_null { *inner } else { parameter.ty() })
            );
            let call = view.types().call(loan.call()).expect("真实 typed call");
            assert_eq!(call.target(), UnitCallTarget::Declaration(show.id()));
            let [mapping] = call.arguments() else {
                panic!("原实参必须映射到唯一声明参数")
            };
            assert_eq!(mapping.argument_index(), 0);
            assert_eq!(mapping.parameter_index(), 0);
            assert_eq!(mapping.mode(), ParameterMode::Borrow);
            assert_eq!(mapping.category(), ExpressionCategory::Temporary);
            assert_eq!(mapping.parameter_type(), parameter.ty());
            let drops = view
                .ownership()
                .drops()
                .iter()
                .filter(|fact| fact.target() == UnitDropTarget::Temporary(loan.argument()))
                .collect::<Vec<_>>();
            let [drop] = drops.as_slice() else {
                panic!("temporary 只能有一个 frontend drop obligation")
            };
            assert_eq!(drop.point(), UnitDropPoint::CallReturn(loan.call()));
            assert_eq!(drop.value_origin(), argument_span);
        }
        let (program, entry) = lower_owned_unit_with_entry(&view, main.id())
            .expect("完整原 q05 必须通过 lowering 内部最终 verifier");
        assert_original_chain(&program, entry, show.name_span());
        verify_program(&program).expect("完整 SSA verifier，不筛选错误或跳过 gate");
        crate::llvm::render_verified_program(&program)
            .expect("同一个真实 Program 必须通过完整 verified LLVM");
    }
}

fn assert_original_chain(program: &Program, entry: FunctionId, show_name: Span) {
    let module = program.module(entry.module()).expect("真实入口 module");
    let main = module.function(entry).expect("真实入口 function");
    assert_eq!(main.blocks.len(), 1);
    assert!(main.blocks[0].parameters.is_empty());
    let [
        integer,
        payload,
        allocation,
        wrap,
        root,
        begin,
        call,
        end,
        drop,
        null,
        null_root,
        null_begin,
        null_call,
        null_end,
        null_drop,
    ] = main.instructions.as_slice()
    else {
        panic!("原 main 只能增加真实 NullableWrap，不得通过删除/改写原操作绕过")
    };
    assert!(matches!(
        integer.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    ));
    assert!(matches!(
        payload.operation,
        Operation::AggregateConstruct { .. }
    ));
    let Operation::HeapAllocate {
        owner: inner,
        payload: allocated_payload,
    } = allocation.operation
    else {
        panic!("Node(1) 必须仍实际分配唯一 HeapOwner")
    };
    assert_eq!(
        EntityId::Value(allocated_payload),
        one_result(main, payload)
    );
    assert!(matches!(
        module.type_kind(inner),
        Some(crate::ssa::model::SsaTypeKind::HeapOwner { .. })
    ));
    let old_owner = value_result(main, allocation, inner);
    let Operation::NullableWrap { nullable, owner } = wrap.operation else {
        panic!("不能仅将 RootPlace 的 result 标成 nullable")
    };
    assert_eq!(owner, old_owner);
    assert_ne!(inner, nullable);
    assert_eq!(
        module.type_kind(nullable),
        Some(&crate::ssa::model::SsaTypeKind::NullableHandle { inner })
    );
    let wrapped = value_result(main, wrap, nullable);
    assert_ne!(old_owner, wrapped);
    assert_eq!(wrap.origin.span(), allocation.origin.span());
    assert_eq!(wrap.origin.span().start(), 140);
    assert_eq!(wrap.origin.span().end(), 147);
    let show = assert_call_lifetime(
        module,
        main,
        wrapped,
        nullable,
        [root, begin, call, end, drop],
    );
    let old_uses = main
        .instructions
        .iter()
        .filter(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Value(old_owner))
        })
        .map(|instruction| instruction.id)
        .collect::<Vec<_>>();
    assert_eq!(
        old_uses,
        [wrap.id],
        "旧 inner 只被 wrap 消费，绝不能再 Drop"
    );
    assert_owner_uses(main, wrapped, root, drop);
    assert!(matches!(null.operation, Operation::NullableNull { nullable: ty } if ty == nullable));
    let null_owner = value_result(main, null, nullable);
    assert_ne!(null_owner, wrapped);
    assert_eq!(
        assert_call_lifetime(
            module,
            main,
            null_owner,
            nullable,
            [null_root, null_begin, null_call, null_end, null_drop],
        ),
        show
    );
    assert_owner_uses(main, null_owner, null_root, null_drop);
    assert_eq!(null.origin.span().start(), 155);
    assert_eq!(null.origin.span().end(), 159);
    let show = module.function(show).expect("实际 show callee");
    assert_eq!(show.origin.span().source_id(), show_name.source_id());
    assert!(show.origin.span().start() <= show_name.start());
    assert!(show_name.end() <= show.origin.span().end());
    let [parameter] = show.blocks[0].parameters.as_slice() else {
        panic!("callee 保持一个 shared nullable loan 参数")
    };
    assert_eq!(
        show.entity(*parameter).expect("callee 参数实体").ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: nullable
        }
    );
    let EntityId::Loan(parameter) = *parameter else {
        panic!("callee 不得把 borrowed nullable 伪装成 owned value")
    };
    assert!(matches!(
        show.instructions[0].operation,
        Operation::NullableLoanIsNull { source } if source == parameter
    ));
    assert!(matches!(
        main.blocks[0].terminator.as_ref().expect("原 main return").kind,
        TerminatorKind::Return { ref values } if values.is_empty()
    ));
}

fn assert_call_lifetime(
    module: &Module,
    function: &Function,
    owner: ValueId,
    target: SsaTypeId,
    instructions: [&Instruction; 5],
) -> FunctionId {
    let [root, begin, call, end, drop] = instructions;
    assert!(matches!(root.operation, Operation::RootPlace { owner: actual } if actual == owner));
    let EntityId::Place(place) = one_result(function, root) else {
        panic!("RootPlace 必须产生 place")
    };
    assert_eq!(
        function.entity(EntityId::Place(place)).expect("place").ty,
        EntityType::Place(target)
    );
    assert!(
        matches!(begin.operation, Operation::BorrowBegin { place: actual, kind: LoanKind::Shared } if actual == place)
    );
    let EntityId::Loan(loan) = one_result(function, begin) else {
        panic!("BorrowBegin 必须产生 loan")
    };
    assert_eq!(
        function.entity(EntityId::Loan(loan)).expect("loan").ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target
        }
    );
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        panic!("保持真实 show direct call")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments, &[EntityId::Loan(loan)]);
    assert!(module.function(*callee).is_some());
    assert!(matches!(end.operation, Operation::BorrowEnd { loan: actual } if actual == loan));
    assert!(matches!(drop.operation, Operation::Drop { owner: actual } if actual == owner));
    assert_eq!(begin.origin.span(), root.origin.span());
    assert_eq!(end.origin.span(), call.origin.span());
    assert_eq!(drop.origin.span(), root.origin.span());
    for instruction in [call, end, drop] {
        assert!(instruction.results.is_empty());
    }
    *callee
}

fn one_result(function: &Function, instruction: &Instruction) -> EntityId {
    let [entity] = instruction.results.as_slice() else {
        panic!("单一真实 SSA result")
    };
    assert_eq!(
        function.entity(*entity).expect("result entity").definition,
        Definition::InstructionResult {
            instruction: instruction.id,
            index: 0
        }
    );
    *entity
}

fn value_result(function: &Function, instruction: &Instruction, ty: SsaTypeId) -> ValueId {
    let entity = one_result(function, instruction);
    assert_eq!(
        function.entity(entity).expect("owner entity").ty,
        EntityType::Value(ty)
    );
    let EntityId::Value(value) = entity else {
        panic!("owner 必须为 Value，不能是 place/loan")
    };
    value
}

fn assert_owner_uses(function: &Function, owner: ValueId, root: &Instruction, drop: &Instruction) {
    let uses = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Value(owner))
        })
        .map(|instruction| instruction.id)
        .collect::<Vec<_>>();
    assert_eq!(uses, [root.id, drop.id], "owner 仅借用与唯一最终 Drop");
}
