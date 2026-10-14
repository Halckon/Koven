//! Scope controls use only original frontend-produced source facts.
use super::*;

fn rejected(source: &str, fragment: &str) {
    let (sources, result) = lower(source, Pipeline::Single);
    let error = result
        .err()
        .expect("unselected single receiver slice remains rejected");
    assert_eq!(
        error.kind,
        LoweringErrorKind::UnsupportedNode,
        "{source}: {error:?}"
    );
    assert_eq!(
        error.span.and_then(|span| sources.slice(span).ok()),
        Some(fragment),
        "{source}: {error:?}"
    );
}

#[test]
fn readonly_keeps_receiver_and_result_boundaries() {
    for (declaration, call) in [
        (
            "value class Item(val n:Int) { own fun read():Int=this.n }",
            "item.read()",
        ),
        (
            "value class Item(var n:Int) { inout fun read():Int=this.n }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun <T> read(own value:T):T=value }",
            "item.read(7)",
        ),
        (
            "value class Item<T>(val n:T) { fun read():Int=7 }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun read():Long=7 }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun read():Unit {} }",
            "item.read()",
        ),
        (
            "class Item(val n:Int) { fun read():Int=this.n }",
            "item.read()",
        ),
    ] {
        rejected(
            &format!("{declaration}\nfun entry() {{ var item=Item(7); {call} }}"),
            call,
        );
    }
}

#[test]
fn readonly_keeps_string_field_operations_outside_new_this_slice() {
    for body in [
        "println(this.text); return 7",
        "println(this.text.clone()); return 7",
    ] {
        rejected(
            &format!(
                "value class Item(val text:String) {{ fun read():Int {{ {body} }} }}\nfun entry() {{ val item=Item(\"text\"); item.read() }}"
            ),
            "this.text",
        );
    }
}

#[test]
fn readonly_keeps_borrow_result_receiver_boundary() {
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nfun view(item:Item):borrow Item from item=item\nfun entry() { val item=Item(7); borrow val alias=view(item); alias.read() }",
        "alias.read()",
    );
}

#[test]
fn readonly_keeps_grouped_borrow_result_and_field_receiver_boundaries() {
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nfun view(item:Item):borrow Item from item=item\nfun entry() { val item=Item(7); borrow val alias=view(item); ((alias)).read() }",
        "((alias)).read()",
    );
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nvalue class Outer(val item:Item)\nfun entry() { val outer=Outer(Item(7)); outer.item.read() }",
        "outer.item.read()",
    );
}
