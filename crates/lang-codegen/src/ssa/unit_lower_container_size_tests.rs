//! SPEC-0274: source length reads preserve container ownership and borrow identity.
use super::{
    model::{Definition, EntityId, EntityType, Function, LoanKind, Operation, Program},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn unit_container_size_reads_all_three_kinds_without_consuming_parameters() {
    let program = fixture(
        r#"package p
        fun arraySize(items: Array<Int>): Int = ((items)).size
        fun listSize(items: List<Int>): Int = ((items)).size
        fun mutableSize(items: MutableList<Int>): Int = ((items)).size
        fun takeArray(own items: Array<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }
        fun takeList(own items: List<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }
        fun takeMutable(own items: MutableList<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }"#,
        r#"package q
        fun entry(): Int {
            val a: Array<Int> = arrayOf()
            val a2 = arrayOf(1, 2)
            val b = listOf(1, 2)
            val b0: List<Int> = listOf()
            val c = mutableListOf(3)
            val c0: MutableList<Int> = mutableListOf()
            val length = ((a)).size + p.arraySize(a) + p.arraySize(a2) + p.listSize(b) + p.listSize(b0) + p.mutableSize(c) + p.mutableSize(c0)
            val arrayLength = p.takeArray(a)
            val arrayLength2 = p.takeArray(a2)
            val listLength = p.takeList(b)
            val listLength0 = p.takeList(b0)
            val mutableLength = p.takeMutable(c)
            val mutableLength0 = p.takeMutable(c0)
            return length + arrayLength + arrayLength2 + listLength + listLength0 + mutableLength + mutableLength0
        }"#,
    );
    for name in ["p.arraySize", "p.listSize", "p.mutableSize"] {
        let function = function(&program, name);
        let parameter = function.blocks[0].parameters[0];
        assert!(matches!(parameter, EntityId::Loan(_)));
        let lengths: Vec<_> = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::ContainerLength { owner } = instruction.operation {
                    Some(owner)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(lengths, [parameter], "length reads the active source loan");
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::Drop { .. } | Operation::BorrowEnd { .. }
            )),
            "the callee must preserve its caller's container"
        );
    }
    for name in ["p.takeArray", "p.takeList", "p.takeMutable"] {
        let take = function(&program, name);
        assert_eq!(
            take.instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::ContainerLength { .. }
                ))
                .count(),
            2,
            "both source reads precede the unique owner cleanup"
        );
        assert_eq!(
            take.instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            1,
            "Value parameter still has exactly one cleanup obligation"
        );
    }
}

#[test]
fn unit_container_size_temporary_receiver_is_evaluated_once_and_cleanup_is_after_read() {
    let program = fixture(
        r#"package p
        fun sourceArray(): Array<Int> = arrayOf(1, 2)
        fun sourceList(): List<Int> = listOf(1, 2)
        fun sourceMutable(): MutableList<Int> = mutableListOf(1, 2)
        fun emptyArray(): Array<Int> = arrayOf()
        fun emptyList(): List<Int> = listOf()
        fun emptyMutable(): MutableList<Int> = mutableListOf()"#,
        r#"package q
        fun entry(): Int {
            val a = ((p.sourceArray())).size
            val b = ((p.sourceList())).size
            val c = ((p.sourceMutable())).size
            val d = ((p.emptyArray())).size
            val e = ((p.emptyList())).size
            val f = ((p.emptyMutable())).size
            return a + b + c + d + e + f
        }"#,
    );
    let entry = function(&program, "q.entry");
    let calls: Vec<_> = entry
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(
        calls.len(),
        6,
        "grouped receivers cannot duplicate source evaluation"
    );
    let lengths: Vec<_> = entry
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| {
            matches!(instruction.operation, Operation::ContainerLength { .. })
        })
        .collect();
    let drops: Vec<_> = entry
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction.operation, Operation::Drop { .. }))
        .collect();
    assert_eq!(lengths.len(), 6);
    assert_eq!(
        drops.len(),
        6,
        "each temporary backing owner must be cleaned up"
    );
    for ((call, length), drop) in calls.iter().zip(&lengths).zip(&drops) {
        assert!(
            call.0 < length.0 && length.0 < drop.0,
            "each read completes before its receiver cleanup"
        );
        let EntityId::Value(owner) = call.1.results[0] else {
            panic!("source returns an owner")
        };
        assert!(
            matches!(drop.1.operation, Operation::Drop { owner: dropped } if dropped == owner),
            "cleanup consumes exactly the owner produced by this source call"
        );
    }
}

#[test]
fn unit_container_size_borrow_uses_current_loan_after_cfg() {
    let program = fixture(
        r#"package p
        fun arraySize(items: Array<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun listSize(items: List<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun mutableSize(items: MutableList<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }"#,
        r#"package q
        fun entry(): Int {
            val a = arrayOf(1, 2)
            val b = listOf(1, 2)
            val c = mutableListOf(1, 2)
            val first = p.arraySize(a, true) + p.arraySize(a, false)
            val second = p.listSize(b, true) + p.listSize(b, false)
            val third = p.mutableSize(c, true) + p.mutableSize(c, false)
            return first + second + third + a[0] + b[0] + c[0]
        }"#,
    );
    for name in ["p.arraySize", "p.listSize", "p.mutableSize"] {
        let function = function(&program, name);
        let owner = function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::ContainerLength { owner } => Some(owner),
                _ => None,
            })
            .expect("size emits a header read");
        assert_ne!(
            owner, function.blocks[0].parameters[0],
            "size must use the loan transported through the conditional"
        );
        let EntityId::Loan(loan) = owner else {
            panic!("size uses Borrow loan")
        };
        let entity = function.entity(owner).unwrap();
        assert_eq!(
            entity.ty,
            function
                .entity(function.blocks[0].parameters[0])
                .unwrap()
                .ty,
            "CFG preserves the concrete container target"
        );
        assert!(matches!(
            entity.ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        assert!(
            matches!(entity.definition, Definition::BlockParameter { block, .. }
            if block != function.blocks[0].id)
        );
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
            )),
            "header read does not end the incoming container loan"
        );
    }
}

fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name}"))
}

fn fixture(provider: &str, consumer: &str) -> Program {
    let mut sources = SourceMap::new();
    let (p_source, p) = parsed(&mut sources, "p/provider.ko", provider);
    let (q_source, q) = parsed(&mut sources, "q/consumer.ko", consumer);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
        SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let entry = declaration(&names, "q", "entry");
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source size is a checked read of an existing container");
    let (permuted, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source identity must survive input permutation");
    assert_eq!(render_program(&program), render_program(&permuted));
    assert_eq!(
        crate::llvm::render_verified_program(&program).unwrap(),
        crate::llvm::render_verified_program(&permuted).unwrap()
    );
    program
}
