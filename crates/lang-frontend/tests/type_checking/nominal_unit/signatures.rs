//! 原接口闭包与泛型签名消费者；保持源码与诊断身份。
use super::super::{codes, diagnostic_spans};
use super::checked_entries;
use lang_frontend::type_checking::{UnitExpressionId, UnitTypeKind};

#[test]
fn interface_closure_substitutes_invariant_arguments_and_checks_bounds() {
    let text = "interface Base<T> {}\n\
                interface Mid<U> : Base<U> {}\n\
                class Good : Mid<Int> {}\n\
                class Wrong : Mid<Long> {}\n\
                class Holder<T: Base<Int>> {}\n\
                class Generic<U: Mid<Int>> { fun pass(input: Holder<U>): Unit {} }\n\
                fun inspect(good: Holder<Good>, bad: Holder<Wrong>): Unit";
    let (sources, _, entries) = checked_entries(text);
    for (source, names, typed) in entries {
        let resolution = names.names().source_units()[0].resolution();
        let nominals = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|item| item.nominal())
            .collect::<Vec<_>>();
        assert_eq!(codes(typed.diagnostics()), ["L0093"]);
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Wrong", "T")
        );
        let good = resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "Good")
            .expect("Good");
        let descriptor = nominals
            .iter()
            .find(|descriptor| {
                descriptor.symbol().source_unit() == source
                    && descriptor.symbol().symbol() == good.id()
            })
            .expect("Good descriptor");
        assert_eq!(descriptor.direct_interfaces().len(), 1);
        assert_eq!(descriptor.interfaces().len(), 2);
    }
}

#[test]
fn generic_signatures_and_legal_this_types_are_known() {
    let text = "fun <T> identity(item: T): T = item\n\
                class Sample { fun self(): Sample = this }\n\
                interface Protocol { fun touch(): Unit { val current = this } }";
    let (_, parsed, entries) = checked_entries(text);
    for (source, names, typed) in entries {
        let resolution = names.names().source_units()[0].resolution();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let identity = resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "identity")
            .expect("identity");
        let identity_declaration = typed
            .signatures()
            .declarations()
            .iter()
            .find(|item| {
                item.symbol().source_unit() == source && item.symbol().symbol() == identity.id()
            })
            .expect("identity declaration");
        assert!(matches!(
            typed
                .symbol_type(identity_declaration.symbol())
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Function { .. })
        ));
        let identity_descriptor = identity_declaration
            .callable()
            .expect("identity descriptor");
        assert!(identity_descriptor.receiver().is_none());
        assert_eq!(identity_descriptor.type_parameters().len(), 1);
        assert!(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|item| item.nominal())
                .flat_map(|nominal| nominal.members())
                .any(|callable| callable.receiver().is_some())
        );
        let this_kinds = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), lang_frontend::parser::Expression::This))
            .map(|(id, _)| {
                typed
                    .expression_type(UnitExpressionId::new(source, id))
                    .and_then(|ty| typed.types().get(ty))
                    .expect("typed this")
            })
            .collect::<Vec<_>>();
        assert!(matches!(this_kinds[0], UnitTypeKind::Nominal { .. }));
        assert!(matches!(this_kinds[1], UnitTypeKind::StaticSelf(_)));
    }
}
