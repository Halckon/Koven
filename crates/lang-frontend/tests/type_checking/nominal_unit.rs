//! 同一重复解析 AST、自定义环境和真实 source 身份的双 raw type 入口。
#[path = "nominal_unit/flow.rs"]
mod flow;

#[path = "nominal_unit/constructions.rs"]
mod constructions;

#[path = "nominal_unit/identity.rs"]
mod identity;

#[path = "nominal_unit/signatures.rs"]
mod signatures;

#[path = "nominal_unit/interface_flow.rs"]
mod interface_flow;

#[path = "nominal_unit/stress.rs"]
mod stress;

use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, ParameterMode, UnitAggregateProjectionReceiver,
        UnitCallReceiverOrigin, UnitExpressionId, check_compilation_unit_types,
    },
};

type CheckedEntry = (
    SourceUnitId,
    ValidatedCompilationUnitNames,
    CompilationUnitTypes,
);

fn checked_entries(text: &str) -> (SourceMap, ParsedFile, [CheckedEntry; 2]) {
    let (sources, file) = super::parse(text);
    let (environment, types) = super::environments();
    let entries = [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", file.source_id(), &file)
        } else {
            SourceUnitInput::memory_document(file.source_id(), &file)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), file.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "types.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("raw unit types");
        assert!(
            typed
                .expression_types()
                .keys()
                .all(|id| id.source_unit() == source.id())
        );
        for construction in typed.constructions() {
            assert_eq!(construction.expression().source_unit(), source.id());
        }
        (source.id(), names, typed)
    });
    (sources, file, entries)
}

#[test]
fn member_calls_publish_receiver_contracts_in_both_unit_entries() {
    let (sources, parsed, entries) = checked_entries(
        "class Holder<T>(val item: T) {\n\
             fun echo(input: T): T = input\n\
             borrow fun same(input: T): T = echo(input)\n\
             inout fun replace(own input: T): T = input\n\
             fun current(): T = item\n\
         }\n\
         fun use(holder: Holder<Int>): Int = holder.same(1)",
    );
    let expression = |text| {
        let mut matches = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text));
        let (id, _) = matches.next().expect("expression in original source");
        assert!(matches.next().is_none(), "unique expression: {text}");
        id
    };
    for (unit, names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let declaration = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.source_unit() == unit && declaration.name() == name)
                .expect("declaration from original source")
                .id()
        };
        let holder = declaration("Holder");
        assert!(
            typed
                .signatures()
                .declaration(declaration("use"))
                .and_then(|signature| signature.callable())
                .expect("top-level use signature")
                .receiver()
                .is_none()
        );
        let signature = typed
            .signatures()
            .declaration(holder)
            .and_then(|signature| signature.nominal())
            .expect("Holder signature");
        assert_eq!(
            signature
                .members()
                .iter()
                .map(|member| (
                    member.name(),
                    member.receiver().expect("member receiver").mode()
                ))
                .collect::<Vec<_>>(),
            [
                ("echo", ParameterMode::Borrow),
                ("same", ParameterMode::Borrow),
                ("replace", ParameterMode::Inout),
                ("current", ParameterMode::Borrow),
            ]
        );
        let unit_expression = |text| UnitExpressionId::new(unit, expression(text));
        assert_eq!(
            typed
                .aggregate_projection(unit_expression("item"))
                .expect("bare field projection")
                .receiver(),
            UnitAggregateProjectionReceiver::This(holder)
        );
        let receiver_origin = |text| {
            typed
                .call(unit_expression(text))
                .and_then(|call| call.receiver())
                .expect("receiver call fact")
                .origin()
        };
        assert_eq!(
            receiver_origin("echo(input)"),
            UnitCallReceiverOrigin::ImplicitThis(holder)
        );
        assert_eq!(
            receiver_origin("holder.same(1)"),
            UnitCallReceiverOrigin::Expression(unit_expression("holder"))
        );
    }
}
