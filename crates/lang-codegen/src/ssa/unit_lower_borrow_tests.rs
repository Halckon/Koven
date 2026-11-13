use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, PlaceAccess},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_shared_borrows_and_loan_forwarding_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun make(): Resource = Resource()\n\
         fun inspect(resource: Resource): Int = 1\n\
         fun forward(resource: Resource): Int = inspect(resource)\n\
         fun pair(left: Resource, right: Resource, own marker: Int): Int = marker\n\
         fun read(number: Int): Int = number",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val owner = p.make()\n\
             val first = p.inspect(owner)\n\
             val second = p.forward(owner)\n\
             val temporary = p.inspect(p.make())\n\
             val paired = p.pair(right = p.make(), marker = 2, left = owner)\n\
             val copied = p.read(40)\n\
             return first + second + temporary + paired + copied\n\
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
    .expect("shared Borrow parameters lower to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves shared-loan identities");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let inspect = function(module, "p.inspect");
    assert!(matches!(inspect.blocks[0].parameters[0], EntityId::Loan(_)));

    let forwarder = function(module, "p.forward");
    let EntityId::Loan(forwarded) = forwarder.blocks[0].parameters[0] else {
        panic!("Borrow parameter must use a function-scoped loan");
    };
    assert!(forwarder.instructions.iter().any(|instruction| matches!(
        &instruction.operation,
        Operation::DirectCall { arguments, .. }
            if arguments.as_slice() == [EntityId::Loan(forwarded)]
    )));
    assert_eq!(operation_count(forwarder, is_borrow_begin), 0);
    assert_eq!(operation_count(forwarder, is_borrow_end), 0);

    let read = function(module, "p.read");
    let EntityId::Loan(number) = read.blocks[0].parameters[0] else {
        panic!("Copyable Borrow parameter must use a loan");
    };
    assert!(read.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan)
        } if loan == number
    )));

    let pair = function(module, "p.pair");
    assert!(matches!(pair.blocks[0].parameters[0], EntityId::Loan(_)));
    assert!(matches!(pair.blocks[0].parameters[1], EntityId::Loan(_)));
    assert!(matches!(pair.blocks[0].parameters[2], EntityId::Value(_)));

    let entry = function(module, "q.entry");
    let (pair_call_index, left_loan, right_loan) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match &instruction.operation {
            Operation::DirectCall {
                callee, arguments, ..
            } if *callee == pair.id
                && matches!(
                    arguments.as_slice(),
                    [EntityId::Loan(_), EntityId::Loan(_), EntityId::Value(_)]
                ) =>
            {
                let [
                    EntityId::Loan(left),
                    EntityId::Loan(right),
                    EntityId::Value(_),
                ] = arguments.as_slice()
                else {
                    unreachable!("guard fixes the direct-call argument shape")
                };
                Some((index, *left, *right))
            }
            _ => None,
        })
        .expect("mixed Borrow/Value call uses parameter-order slots");
    assert!(matches!(
        entry.instructions[pair_call_index + 1].operation,
        Operation::BorrowEnd { loan } if loan == left_loan
    ));
    assert!(matches!(
        entry.instructions[pair_call_index + 2].operation,
        Operation::BorrowEnd { loan } if loan == right_loan
    ));

    assert_eq!(operation_count(entry, is_borrow_begin), 6);
    assert_eq!(operation_count(entry, is_borrow_end), 6);
    assert_eq!(operation_count(entry, is_root_place), 6);
    assert_eq!(operation_count(entry, is_drop), 3);
}

#[test]
fn inout_and_non_root_borrows_remain_atomic_boundaries() {
    for (path, source) in [
        (
            "test/inout.ko",
            "package test\n\
             class Resource {}\n\
             fun mutate(inout resource: Resource): Unit {}\n\
             fun entry(): Unit {\n\
                 var owner = Resource()\n\
                 val result = mutate(&owner)\n\
             }",
        ),
        (
            "test/projection.ko",
            "package test\n\
             class Resource {}\n\
             value class Holder(val resource: Resource)\n\
             fun inspect(resource: Resource): Unit {}\n\
             fun entry(own holder: Holder): Unit {\n\
                 val result = inspect(holder.resource)\n\
             }",
        ),
        (
            "test/unit-entry.ko",
            "package test\n\
             fun entry(item: Unit): Int = 0",
        ),
        (
            "test/unit-call.ko",
            "package test\n\
             fun produce(): Unit {}\n\
             fun ignore(item: Unit): Int = 0\n\
             fun entry(): Int = ignore(produce())",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source_id, parsed) = parsed(&mut sources, path, source);
        let inputs = [SourceUnitInput::new("root", path, source_id, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let error = match crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "test", "entry"),
            )
        }) {
            Ok(_) => panic!("{path} remains outside the shared-root Borrow core"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{path}");
    }
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
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

fn is_borrow_begin(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowBegin { .. })
}

fn is_borrow_end(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowEnd { .. })
}

fn is_root_place(operation: &Operation) -> bool {
    matches!(operation, Operation::RootPlace { .. })
}

fn is_drop(operation: &Operation) -> bool {
    matches!(operation, Operation::Drop { .. })
}

mod nullable_temporary_tests {
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
    fn copyable_nullable_borrow_runs_with_or_without_a_refinement_anchor() {
        let source = r#"fun read(value: Int?): Int = if (value == null) 0 else value
    fun relay(value: Int): Int { /*route*/ return read((value)) }
    fun flag(value: Boolean?): Unit { if (value != null && value) { println("flag") } }
    fun main(): Unit { /*route*/
        val n: Int = 8
        if (read(7) == 7 && read((n)) == 8 && relay(9) == 9 && read(null) == 0) {
            println("injected")
        } else { error("wrong nullable value") }
        flag(true)
    }"#;
        for (anchor, text) in [
            (false, source.to_owned()),
            (
                true,
                source.replace("/*route*/", "val anchor: Int? = null;"),
            ),
        ] {
            let (output, routes) = crate::ssa::refine::observe_routing(|| {
                crate::native_tests::emit_link_and_run("copyable_nullable.ko", &text, "main")
            });
            crate::native_tests::boxed_enum_tests::assert_success(&output, b"injected\nflag\n");
            assert_eq!(
                routes
                    .iter()
                    .any(|name| name.split('.').any(|part| part == "relay")),
                anchor,
                "{routes:?}"
            );
        }
    }

    #[test]
    fn copyable_nullable_borrow_snapshots_before_a_later_inout_argument() {
        let source = r#"fun capture(value: Int?, inout changed: Int): Unit {
        changed = 9
        if (value != null) {
            if (value == 7) { println("snapshot") }
        }
    }
    fun main(): Unit {
        var n: Int = 7
        capture(value = (n), changed = &n)
        if (n == 9) { println("changed") }
    }"#;
        let output = crate::native_tests::emit_link_and_run("nullable_snapshot.ko", source, "main");
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"snapshot\nchanged\n");
    }

    #[test]
    fn copyable_nullable_borrow_ends_on_later_return_and_call_return() {
        let source = r#"fun accept(value: Int?, own marker: Int): Unit {
        if (value != null) {
            if (value == 7 && marker == 8) { println("accepted") }
        }
    }
    fun exercise(own flag: Boolean): Unit {
        /*route*/
        val n: Int = 7
        accept((n), if (flag) { return } else 8)
        println("finished")
    }
    fun main(): Unit { exercise(true); exercise(false) }"#;
        for (anchor, text) in [
            (false, source.to_owned()),
            (true, source.replace("/*route*/", "val anchor: Int? = null")),
        ] {
            let (output, routes) = crate::ssa::refine::observe_routing(|| {
                crate::native_tests::emit_link_and_run("nullable_early_return.ko", &text, "main")
            });
            crate::native_tests::boxed_enum_tests::assert_success(&output, b"accepted\nfinished\n");
            assert_eq!(
                routes
                    .iter()
                    .any(|name| name.split('.').any(|part| part == "exercise")),
                anchor,
                "{routes:?}"
            );
        }
    }

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
            let complete = complete_compilation_unit_types(
                &sources,
                &inputs,
                &names,
                &environment,
                typed.types(),
            )
            .expect("完整类型接纳");
            assert!(complete.is_same_analysis(typed.types()));
            assert!(owned.ownership().deferred().is_empty());
            let view = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
            )
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
            let Some(UnitTypeKind::Nullable(inner)) = view.types().types().get(parameter.ty())
            else {
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
        assert!(
            matches!(null.operation, Operation::NullableNull { nullable: ty } if ty == nullable)
        );
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
        assert!(
            matches!(root.operation, Operation::RootPlace { owner: actual } if actual == owner)
        );
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

    fn assert_owner_uses(
        function: &Function,
        owner: ValueId,
        root: &Instruction,
        drop: &Instruction,
    ) {
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
}
