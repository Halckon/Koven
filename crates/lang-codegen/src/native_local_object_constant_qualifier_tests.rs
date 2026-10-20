//! Ordinary local constant qualifiers and a same-spelling runtime receiver.
//! Every assertion reads original Unit products; no facts, IDs, or SSA are injected.

use super::{OwnedProducts, UnitSource};
use crate::ssa::{
    model::{EntityType, Operation, Origin, ScalarConstant, SsaTypeKind},
    verify::verify_program,
};
use lang_frontend::{
    name_resolution::{
        Namespace, Symbol, SymbolKind, UnitNameReference, UnitReferenceTarget, UnitSymbolId,
    },
    ownership_checking::ConstantMaterializationKind,
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ConstValue, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitTypeKind,
    },
};

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one real MemoryDocument")
    };
    assert_eq!(unit.source_id(), source.parsed.source_id());
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| source.sources.slice(node.span()) == Ok(text))
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one actual expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn symbol(source: &UnitSource, identity: UnitSymbolId) -> &Symbol {
    let local = source
        .names
        .names()
        .source_units()
        .iter()
        .find(|local| local.source_unit() == identity.source_unit())
        .expect("symbol belongs to the original source unit");
    let symbol = local
        .resolution()
        .symbols()
        .get(identity.symbol().index())
        .expect("original source symbol");
    assert_eq!(symbol.id(), identity.symbol());
    assert_eq!(symbol.span().source_id(), source.parsed.source_id());
    assert!(!symbol.is_synthetic());
    symbol
}

fn role(source: &UnitSource, span: Span, namespace: Namespace) -> &UnitNameReference {
    let references = source
        .names
        .names()
        .references()
        .iter()
        .filter(|reference| reference.span() == span && reference.namespace() == Some(namespace))
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        panic!("exactly one original {namespace:?} reference at {span:?}: {references:?}")
    };
    assert_eq!(
        reference.source_unit(),
        source.names.names().index().source_units()[0].id()
    );
    reference
}

#[test]
fn local_object_and_companion_constants_keep_qualifier_roles_and_real_materializations() {
    let text = r#"object Labels { const val COUNT: Int = 42 }
class Tokens { companion object { const val LETTER: Char = '文' } }
fun count(): Int = Labels.COUNT
fun letter(): Char = Tokens.LETTER"#;
    // Analyze retains the original strict complete gate before either ownership path.
    let source = UnitSource::analyze("local-constant-qualifiers.ko", text)
        .expect("ordinary associated constants pass strict Unit admission and ownership");
    source.assert_frontend_clean();
    let OwnedProducts::Constant(typed, owned) = &source.products else {
        panic!("real associated declarations require Constant capability")
    };
    assert!(owned.is_compatible_with(typed));
    assert_eq!(typed.constants().declarations().len(), 2);
    assert_eq!(typed.constants().uses().len(), 2);
    assert_eq!(owned.materializations().len(), 2);

    for (
        owner,
        member,
        function_name,
        builtin,
        expected_value,
        expected_ssa,
        expected_type,
    ) in [
        (
            "Labels",
            "Labels.COUNT",
            "count",
            BuiltinType::Int,
            ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 42,
            },
            ScalarConstant::Integer(42),
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
        ),
        (
            "Tokens",
            "Tokens.LETTER",
            "letter",
            BuiltinType::Char,
            ConstValue::Char('文'),
            ScalarConstant::Char(u32::from('文')),
            SsaTypeKind::Char,
        ),
    ] {
        let (member_id, member_span) = expression(&source, member);
        let node = source
            .parsed
            .ast()
            .expressions()
            .get(member_id.expression())
            .unwrap();
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = node.payload()
        else {
            panic!("the original source is an ordinary qualified member")
        };
        let receiver = UnitExpressionId::new(member_id.source_unit(), *receiver);
        let receiver_node = source
            .parsed
            .ast()
            .expressions()
            .get(receiver.expression())
            .unwrap();
        assert!(matches!(receiver_node.payload(), Expression::Name));
        assert_eq!(source.sources.slice(receiver_node.span()), Ok(owner));
        let declaration = source.declaration(owner, SymbolKind::Classifier);
        let names = source.names.names();
        let classifier = &names.index().declarations()[declaration.index()];
        assert_eq!(classifier.namespace(), Namespace::Type);
        assert_eq!(
            role(&source, receiver_node.span(), Namespace::Type).target(),
            &UnitReferenceTarget::Declaration(declaration),
            "each qualifier has exactly one real Type declaration role"
        );
        let classifier_symbol = symbol(&source, names.declaration_symbol(declaration).unwrap());
        assert_eq!(classifier_symbol.kind(), SymbolKind::Classifier);
        assert_eq!(classifier_symbol.span(), classifier.name_span());
        if owner == "Labels" {
            let value = source.declaration(owner, SymbolKind::ObjectValue);
            assert_ne!(value, declaration);
            let value_declaration = &names.index().declarations()[value.index()];
            assert_eq!(value_declaration.namespace(), Namespace::Value);
            assert_eq!(value_declaration.name_span(), classifier.name_span());
            assert_eq!(value_declaration.root(), classifier.root());
            assert_eq!(
                role(&source, receiver_node.span(), Namespace::Value).target(),
                &UnitReferenceTarget::Declaration(value),
                "the lexical ObjectValue role is retained, not replaced"
            );
            assert_eq!(
                symbol(&source, names.declaration_symbol(value).unwrap()).kind(),
                SymbolKind::ObjectValue
            );
        }
        assert_eq!(typed.types().expression_type(receiver), None);
        assert!(typed.types().aggregate_projection(member_id).is_none());
        let uses = typed
            .constants()
            .uses()
            .iter()
            .filter(|usage| usage.expression() == member_id)
            .collect::<Vec<_>>();
        let [usage] = uses.as_slice() else {
            panic!("one real constant descriptor for the whole Member expression")
        };
        assert_eq!(
            role(&source, *name_span, Namespace::Value).target(),
            &UnitReferenceTarget::Symbol(usage.target())
        );
        let target = symbol(&source, usage.target());
        assert_eq!(target.kind(), SymbolKind::Constant);
        assert_eq!(source.sources.slice(*name_span), Ok(target.name()));
        let declarations = typed
            .constants()
            .declarations()
            .iter()
            .filter(|constant| constant.symbol() == usage.target())
            .collect::<Vec<_>>();
        let [constant] = declarations.as_slice() else {
            panic!("the use selects one actual constant declaration")
        };
        assert_eq!(constant.declaration_span(), target.span());
        assert_eq!(constant.ty(), usage.ty());
        assert_eq!(constant.value(), &expected_value);
        assert_eq!(usage.value(), constant.value());
        assert_eq!(typed.types().expression_type(member_id), Some(usage.ty()));
        assert_eq!(
            typed.types().types().get(usage.ty()),
            Some(&UnitTypeKind::Builtin(builtin))
        );
        let materialization = owned.materialization_at(member_id).unwrap();
        assert_eq!(materialization.descriptor(), *usage);
        assert_eq!(
            materialization.kind(),
            ConstantMaterializationKind::InlineCopy
        );
        assert!(owned.materialization_at(receiver).is_none());

        let (program, entry) = source
            .lower_entry(source.declaration(function_name, SymbolKind::Function))
            .expect("original Constant products lower to SSA");
        verify_program(&program).expect("real scalar materialization verifies");
        let module = program.module(entry.module()).unwrap();
        assert_eq!(
            module.functions.len(),
            1,
            "no qualifier or initializer runtime function"
        );
        let function = module.function(entry).unwrap();
        let [return_type] = function.return_types.as_slice() else {
            panic!("one real scalar return")
        };
        assert_eq!(module.types[return_type.index()], expected_type);
        let constants = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Constant(_)))
            .collect::<Vec<_>>();
        let [constant] = constants.as_slice() else {
            panic!("one source-produced scalar constant")
        };
        assert_eq!(constant.origin, Origin::Source(member_span));
        assert_eq!(constant.operation, Operation::Constant(expected_ssa));
        let [result] = constant.results.as_slice() else {
            panic!("one real materialized result")
        };
        assert_eq!(
            function.entity(*result).unwrap().ty,
            EntityType::Value(*return_type)
        );
    }
}

#[test]
fn shadowing_parameter_keeps_value_role_and_basic_field_projection() {
    let text = r#"object Labels {}
class Holder(val COUNT: Boolean)
fun inspect(Labels: Holder): Boolean = Labels.COUNT"#;
    let source = UnitSource::analyze("shadowed-object-qualifier.ko", text)
        .expect("ordinary parameter field access passes strict Unit admission and ownership");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("no constant declaration or selection may force Constant capability")
    };
    assert!(owned.ownership().diagnostics().is_empty());
    assert!(owned.ownership().deferred().is_empty());
    let typed = typed.types();
    let constants = typed.constants().expect("clean source publishes empty constant facts");
    assert!(constants.declarations().is_empty());
    assert!(constants.uses().is_empty());
    let (receiver, receiver_span) = expression(&source, "Labels");
    let (member, _) = expression(&source, "Labels.COUNT");
    assert!(source.names.names().references().iter().all(|reference| {
        reference.span() != receiver_span || reference.namespace() != Some(Namespace::Type)
    }));
    let UnitReferenceTarget::Symbol(parameter_symbol) =
        role(&source, receiver_span, Namespace::Value).target()
    else {
        panic!("the lexical receiver selects its parameter, not the object declaration")
    };
    let parameter = symbol(&source, *parameter_symbol);
    assert_eq!(parameter.kind(), SymbolKind::ValueParameter);
    assert_eq!(parameter.name(), "Labels");
    let inspect = source.declaration("inspect", SymbolKind::Function);
    let callable = typed
        .signatures()
        .declaration(inspect)
        .unwrap()
        .callable()
        .unwrap();
    let [parameter_signature] = callable.parameters() else {
        panic!("one actual inspect parameter")
    };
    assert_eq!(parameter_signature.symbol(), Some(*parameter_symbol));
    assert_eq!(typed.expression_type(receiver), Some(parameter_signature.ty()));
    let holder = source.declaration("Holder", SymbolKind::Classifier);
    assert!(matches!(
        typed.types().get(parameter_signature.ty()),
        Some(UnitTypeKind::Nominal { declaration, arguments })
            if *declaration == holder && arguments.is_empty()
    ));
    let nominal = typed
        .signatures()
        .declaration(holder)
        .unwrap()
        .nominal()
        .unwrap();
    let [field] = nominal.fields() else {
        panic!("one actual Holder field")
    };
    assert_eq!(field.name(), "COUNT");
    assert_eq!(symbol(&source, field.symbol()).kind(), SymbolKind::Field);
    let projection = typed
        .aggregate_projection(member)
        .expect("ordinary runtime field projection");
    assert_eq!(projection.kind(), UnitAggregateProjectionKind::Field);
    assert_eq!(
        projection.receiver(),
        UnitAggregateProjectionReceiver::Expression(receiver)
    );
    assert_eq!(projection.field(), field.symbol());
    assert_eq!(projection.ty(), field.ty());
    assert_eq!(typed.expression_type(member), Some(field.ty()));
    assert_eq!(
        typed.types().get(field.ty()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
    );
}
