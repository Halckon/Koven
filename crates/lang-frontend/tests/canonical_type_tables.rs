//! 两种 Unit 入口的 signature/raw 类型表共享固定规范种子顺序。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, IntegerConstraint, UnitTypeKind, UnitTypeTable, check_compilation_unit_types,
        collect_compilation_unit_signatures, standard_environments,
    },
};

// 固定期望不从生产 ALL 或另一轮分析生成，避免两个入口一起换序仍然通过。
fn assert_seed_order(table: &UnitTypeTable) {
    let builtins = [
        BuiltinType::Byte,
        BuiltinType::Short,
        BuiltinType::Int,
        BuiltinType::Long,
        BuiltinType::UByte,
        BuiltinType::UShort,
        BuiltinType::UInt,
        BuiltinType::ULong,
        BuiltinType::Float,
        BuiltinType::Double,
        BuiltinType::Boolean,
        BuiltinType::Char,
        BuiltinType::String,
        BuiltinType::Unit,
        BuiltinType::Nothing,
        BuiltinType::Any,
        BuiltinType::IntRange,
        BuiltinType::LongRange,
    ];
    assert_eq!(BuiltinType::ALL, builtins);
    let kinds = builtins.into_iter().map(UnitTypeKind::Builtin).chain([
        UnitTypeKind::IntegerLiteral(IntegerConstraint::Signed),
        UnitTypeKind::IntegerLiteral(IntegerConstraint::Unsigned),
        UnitTypeKind::Error,
    ]);
    assert_eq!(table.len(), 21);
    for (index, kind) in kinds.enumerate() {
        let id = table.find(&kind).expect("all original seed kinds exist");
        assert_eq!(id.index(), index, "canonical seed {kind:?}");
        assert_eq!(table.get(id), Some(&kind));
        if let UnitTypeKind::Builtin(builtin) = kind {
            assert_eq!(table.builtin(builtin), Some(id));
        }
    }
}

#[test]
fn unit_entries_and_signature_tables_preserve_exact_seed_order() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("empty.ko", "package sample")
        .expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing succeeds internally");
    assert!(parsed.diagnostics().is_empty());
    let (name_environment, type_environment) = standard_environments();

    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "sample/empty.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        }];
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
        let [indexed] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(indexed.source_id(), source);
        match (rooted, indexed.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "sample/empty.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("unit names resolve");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("unit signatures collect");
        assert!(signatures.diagnostics().is_empty());
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("raw unit types check");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_seed_order(signatures.types());
        assert_seed_order(typed.types());
        assert_eq!(typed.types(), signatures.types());
    }
}
