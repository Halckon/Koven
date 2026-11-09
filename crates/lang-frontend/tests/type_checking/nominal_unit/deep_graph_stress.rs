//! 原深泛型与 interface 图的双入口压力消费者。
use super::checked_entries;

#[test]
fn deep_generic_and_long_interface_graphs_stay_deterministic() {
    let mut text = String::from("class Box<T> {}\ninterface I0 {}\n");
    for index in 1..64 {
        text.push_str(&format!("interface I{index} : I{} {{}}\n", index - 1));
    }
    text.push_str("class Leaf : I63 {}\nfun deep(input: ");
    for _ in 0..96 {
        text.push_str("Box<");
    }
    text.push_str("Int");
    for _ in 0..96 {
        text.push('>');
    }
    text.push_str("): Unit {}\n");
    let (_, _, entries) = checked_entries(&text);
    for (source, names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [local] = names.names().source_units() else {
            panic!("one original source")
        };
        let leaf = local
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "Leaf")
            .expect("Leaf");
        let descriptor = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .find(|descriptor| descriptor.symbol().symbol() == leaf.id())
            .expect("Leaf descriptor");
        assert_eq!(descriptor.symbol().source_unit(), source);
        assert_eq!(descriptor.interfaces().len(), 64);
    }
}
