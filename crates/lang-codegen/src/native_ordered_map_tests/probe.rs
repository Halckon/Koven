//! Single-source identity checks and one-call-only instrumentation of its checkpoint.

use inkwell::{
    context::Context,
    memory_buffer::MemoryBuffer,
    targets::TargetData,
    values::{AnyValue, InstructionOpcode},
};
use lang_frontend::name_resolution::SymbolKind;

use crate::{
    llvm::ordered_map_test_support::{self, OrderedMapLayout},
    ssa::model::{
        EntityType, FunctionId, LoanKind, MapContainerKind, Operation, Origin, Program, SsaTypeId,
        SsaTypeKind,
    },
};

use super::fixtures::Fixture;

const OBSERVER: &str = "koven_ordered_map_observe";

pub(super) struct Product {
    program: Program,
    checkpoint: FunctionId,
    map_type: SsaTypeId,
    pub(super) raw: String,
}

pub(super) struct Probe {
    pub(super) llvm: String,
    pub(super) layout: OrderedMapLayout,
    pub(super) identity: String,
}

pub(super) fn lower(fixture: &Fixture) -> Product {
    let analysis = super::super::analyze(fixture.name, fixture.source);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let checkpoint_symbols: Vec<_> = analysis
        .names
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "checkpoint" && symbol.kind() == SymbolKind::Function)
        .collect();
    assert_eq!(
        checkpoint_symbols.len(),
        1,
        "fixture declaration must be unique"
    );
    let declaration = checkpoint_symbols[0].span();
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        super::super::symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("ordinary checkpoint source must lower without bypassing SSA verification");
    crate::ssa::verify::verify_program(&program).expect("real source SSA must verify");
    assert_eq!(program.modules.len(), 1, "single-source route only");
    let module = program.module(entry.module()).unwrap();
    let checkpoints: Vec<_> = module
        .functions
        .iter()
        .filter(|function| {
            let span = function.origin.span();
            span.source_id() == declaration.source_id()
                && span.start() <= declaration.start()
                && declaration.end() <= span.end()
        })
        .collect();
    assert_eq!(checkpoints.len(), 1, "unique source-bound FunctionId");
    let checkpoint = checkpoints[0];
    assert_eq!(checkpoint.name, "checkpoint");
    let Origin::Source(span) = &checkpoint.origin else {
        panic!("checkpoint must be source-authored")
    };
    assert_eq!(
        analysis.sources.source_name(span.source_id()).unwrap(),
        fixture.name
    );
    assert!(
        analysis
            .sources
            .slice(*span)
            .unwrap()
            .starts_with("fun checkpoint(")
    );
    assert!(checkpoint.return_types.is_empty());
    assert_eq!(checkpoint.borrow_return, None);
    assert_eq!(checkpoint.carrier_return, None);
    assert_eq!(checkpoint.receiver, None);
    let parameters = &checkpoint.blocks[0].parameters;
    assert_eq!(parameters.len(), 2);
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    } = checkpoint.entity(parameters[0]).unwrap().ty
    else {
        panic!("checkpoint's first parameter must be a shared Map loan")
    };
    let EntityType::Value(step_type) = checkpoint.entity(parameters[1]).unwrap().ty else {
        panic!("checkpoint's explicit own step must be a Value")
    };
    let integer = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    assert_eq!(module.type_kind(step_type), Some(&integer));
    let (kind, key, value) = module.map_container(map_type).unwrap();
    assert_eq!(
        kind,
        if fixture.readonly {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        }
    );
    assert_eq!(module.type_kind(value), Some(&integer));
    assert_eq!(
        module.type_kind(key),
        Some(if fixture.string_keys {
            &SsaTypeKind::StringOwner
        } else {
            &integer
        })
    );
    let sizes: Vec<_> = checkpoint
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::MapSize { owner } = &instruction.operation {
                Some(*owner)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        sizes,
        [parameters[0]],
        "the checkpoint must read precisely its borrowed Map"
    );
    let calls = module.functions.iter().flat_map(|function| &function.instructions).filter(|instruction| {
        matches!(&instruction.operation, Operation::DirectCall { callee, .. } if *callee == checkpoint.id)
    }).count();
    assert_eq!(
        calls,
        fixture.snapshots.len(),
        "every declared checkpoint must have one source call"
    );
    let checkpoint_id = checkpoint.id;
    let raw = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("untouched production LLVM must verify");
    Product {
        program,
        checkpoint: checkpoint_id,
        map_type,
        raw,
    }
}

pub(super) fn instrument(product: &Product) -> Probe {
    let context = Context::create();
    let llvm = context
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            product.raw.as_bytes(),
            "ordered-map.raw.ll",
        ))
        .expect("parse untouched production LLVM");
    llvm.verify().expect("raw LLVM verification");
    assert!(
        llvm.get_function(OBSERVER).is_none(),
        "observer symbol must not preexist"
    );
    assert!(
        llvm.get_global(OBSERVER).is_none(),
        "observer global must not preexist"
    );
    let module = product.program.module(product.checkpoint.module()).unwrap();
    let checkpoint = module.function(product.checkpoint).unwrap();
    let symbol = format!("f{}.{}", checkpoint.id.index(), checkpoint.name);
    let target = llvm
        .get_function(&symbol)
        .expect("exact real FunctionId/name LLVM symbol");
    assert_eq!(target.count_params(), 2);
    assert!(target.get_type().get_return_type().is_none());
    assert!(!target.get_type().is_var_arg());
    let header_pointer = target.get_nth_param(0).unwrap();
    assert!(header_pointer.is_pointer_value());
    let step = target.get_nth_param(1).unwrap();
    assert_eq!(step.into_int_value().get_type().get_bit_width(), 32);
    let data_layout = llvm.get_data_layout().as_str().to_str().unwrap().to_owned();
    assert!(!data_layout.is_empty());
    let layout = ordered_map_test_support::describe(module, product.map_type, &data_layout);
    let header = context
        .get_struct_type(&layout.header_name)
        .expect("bound named header in raw LLVM");
    let data = TargetData::create(&data_layout);
    assert_eq!(
        llvm.get_triple().as_str().to_str().unwrap(),
        crate::llvm::native_target_triple().unwrap()
    );
    assert_eq!(data.get_pointer_byte_size(None) * 8, layout.pointer_bits);
    assert_eq!(header.count_fields(), 6);
    assert!(header.get_field_type_at_index(0).unwrap().is_pointer_type());
    for index in 1..6 {
        assert_eq!(
            header
                .get_field_type_at_index(index)
                .unwrap()
                .into_int_type()
                .get_bit_width(),
            layout.pointer_bits
        );
    }
    assert_eq!(data.get_abi_size(&header), layout.header_size);
    assert_eq!(data.get_abi_alignment(&header), layout.header_alignment);
    for (index, offset) in layout.header_offsets.iter().enumerate() {
        assert_eq!(data.offset_of_element(&header, index as u32), Some(*offset));
    }
    let loads: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| {
            instruction.get_opcode() == InstructionOpcode::Load
                && instruction.get_type().is_struct_type()
                && instruction.get_type().into_struct_type() == header
        })
        .collect();
    assert_eq!(loads.len(), 1, "one actual header load from MapSize");
    assert_eq!(loads[0].get_type().into_struct_type(), header);
    assert_eq!(
        loads[0].get_operand(0).unwrap().unwrap_value(),
        header_pointer
    );
    let slots: Vec<_> = llvm
        .get_functions()
        .flat_map(|function| function.get_basic_blocks())
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| instruction.get_opcode() == InstructionOpcode::GetElementPtr)
        .filter_map(|instruction| {
            let ty = instruction.get_gep_source_element_type().unwrap();
            (ty.print_to_string().to_string() == layout.slot_type).then(|| ty.into_struct_type())
        })
        .collect();
    assert!(
        !slots.is_empty(),
        "actual raw Map slot GEP must match the real production slot type"
    );
    for slot in slots {
        if let Some(offsets) = layout.string_offsets {
            let string = slot.get_field_type_at_index(1).unwrap().into_struct_type();
            assert_eq!(string.count_fields(), 3);
            assert!(string.get_field_type_at_index(0).unwrap().is_pointer_type());
            for index in [1, 2] {
                assert_eq!(
                    string
                        .get_field_type_at_index(index)
                        .unwrap()
                        .into_int_type()
                        .get_bit_width(),
                    layout.pointer_bits
                );
            }
            for (index, offset) in offsets.iter().enumerate() {
                assert_eq!(data.offset_of_element(&string, index as u32), Some(*offset));
            }
        }
        assert_eq!(data.get_abi_size(&slot), layout.slot_stride);
        assert_eq!(data.get_abi_alignment(&slot), layout.slot_alignment);
        for (index, offset) in layout.slot_offsets.iter().enumerate() {
            assert_eq!(data.offset_of_element(&slot, index as u32), Some(*offset));
        }
    }
    let before = llvm.print_to_string().to_string();
    let original_functions: Vec<_> = llvm
        .get_functions()
        .map(|function| {
            (
                function.get_name().to_str().unwrap().to_owned(),
                function.print_to_string().to_string(),
            )
        })
        .collect();
    let original_blocks: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .map(|block| {
            (
                block.get_name().to_str().unwrap().to_owned(),
                block
                    .get_instructions()
                    .map(|instruction| instruction.print_to_string().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let observer = llvm.add_function(OBSERVER, target.get_type(), None);
    let first = target
        .get_first_basic_block()
        .unwrap()
        .get_instructions()
        .find(|instruction| instruction.get_opcode() != InstructionOpcode::Phi)
        .unwrap();
    let builder = context.create_builder();
    builder.position_before(&first);
    let inserted = builder
        .build_call(observer, &[header_pointer.into(), step.into()], "")
        .expect("insert exactly one void observer call")
        .try_as_basic_value()
        .unwrap_instruction();
    llvm.verify()
        .expect("one-call checkpoint instrumentation must verify");
    let after_blocks: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .map(|block| {
            (
                block.get_name().to_str().unwrap().to_owned(),
                block
                    .get_instructions()
                    .filter(|instruction| *instruction != inserted)
                    .map(|instruction| instruction.print_to_string().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        original_blocks, after_blocks,
        "original checkpoint instructions and CFG must remain byte-identical"
    );
    for (name, original) in &original_functions {
        if name != &symbol {
            assert_eq!(
                &llvm
                    .get_function(name)
                    .unwrap()
                    .print_to_string()
                    .to_string(),
                original,
                "unrelated original function changed: {name}"
            );
        }
    }
    assert_eq!(llvm.get_functions().count(), original_functions.len() + 1);
    let instrumented = llvm.print_to_string().to_string();
    let added_call = inserted.print_to_string().to_string();
    let declaration = observer.print_to_string().to_string();
    let normalize = |text: &str| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let after_without_additions: Vec<_> = instrumented
        .lines()
        .filter(|line| {
            !line.trim().is_empty()
                && line.trim() != added_call.trim()
                && line.trim() != declaration.trim()
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(
        normalize(&before),
        after_without_additions,
        "globals, types, attributes and all other original module text must remain unchanged"
    );
    let contract = module.map_container(product.map_type).unwrap();
    let identity = format!(
        "checkpoint_id={:?}\nsource checkpoint={:?}\nLLVM symbol={symbol}\nmap={:?}\nMapKind/K/V={contract:?}\ntriple={}\ndata_layout={data_layout}\nlayout={layout:#?}\n",
        checkpoint.id,
        checkpoint.origin,
        product.map_type,
        llvm.get_triple().as_str().to_str().unwrap()
    );
    Probe {
        llvm: instrumented,
        layout,
        identity,
    }
}
