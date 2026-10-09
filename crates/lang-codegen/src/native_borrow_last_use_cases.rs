//! Normal source inventories for restricted last-use; no fault/IR generation.
pub(crate) struct Case {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
    pub(crate) stdout: &'static [u8],
    pub(crate) allocations: usize,
}

pub(crate) fn cases() -> [Case; 5] {
    [
        Case {
            name: "last-use-chain",
            source: r#"fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "kept".clone()
 borrow val parent = view(source)
 borrow val child = wrap(parent)
 borrow val leaf = view(child)
 println(leaf)
 consume(source)
}"#,
            stdout: b"kept\nkept\n",
            allocations: 1,
        },
        Case {
            name: "last-use-projections",
            source: r#"value class Packet(val text: String)
class Resource(val text: String) { deinit() { println("drop") } }
fun <T> view(source: T): borrow T from source = source
fun packetView(source: Packet): borrow String from source = source.text
fun resourceView(source: Resource): borrow String from source = source.text
fun consumePacket(own source: Packet) { println("packet-owned") }
fun consumeResource(own source: Resource) { println("resource-owned") }
fun entry() {
 val packet = Packet("inline".clone())
 borrow val parent = view(packet)
 borrow val text = packetView(parent)
 println(text)
 consumePacket(packet)
 val resource = Resource("heap".clone())
 borrow val resourceParent = view(resource)
 borrow val resourceText = resourceView(resourceParent)
 println(resourceText)
 consumeResource(resource)
}"#,
            stdout: b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
            allocations: 3,
        },
        Case {
            name: "last-use-resource-lexical",
            source: r#"class Resource(val n: Int) { deinit() { println("drop") } }
fun view(source: Resource): borrow Resource from source = source
fun observe(item: Resource) { println("seen") }
fun entry() {
 val source = Resource(7)
 borrow val item = view(source)
 observe(item)
 println("after")
}"#,
            stdout: b"seen\nafter\ndrop\n",
            allocations: 1,
        },
        Case {
            name: "last-use-map-resource",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observeFirst(item: Resource) { if (item.n == 1) { println("first") } else { error("wrong first") } }
fun observeSecond(item: Resource) { if (item.n == 2) { println("second") } else { error("wrong second") } }
fun entry() {
 var source = mutableMapOf<String, Resource>()
 source.put("key".clone(), Resource(1))
 borrow val first = source.requireValue("key")
 observeFirst(first)
 source.put("key".clone(), Resource(2))
 borrow val second = source.requireValue("key")
 observeSecond(second)
}"#,
            stdout: b"first\ndrop-one\nsecond\ndrop-two\n",
            allocations: 5,
        },
        Case {
            name: "last-use-nullable-slot",
            source: r#"class Token(val n: Int)
fun observe(item: Token?) { if (item == null) { println("null") } else { println("full") } }
fun entry() {
 var source = mutableMapOf<String, Token?>()
 source.put("key".clone(), null)
 borrow val empty = source.requireValue("key")
 observe(empty)
 source.put("key".clone(), Token(7))
 borrow val full = source.requireValue("key")
 observe(full)
 source.put("after".clone(), null)
}"#,
            stdout: b"null\nfull\n",
            allocations: 5,
        },
    ]
}

pub(crate) fn alias_cases() -> [Case; 5] {
    [
        Case {
            name: "alias-multiple-and-child",
            source: r#"fun <T> view(source: T): borrow T from source = source
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "kept".clone()
 borrow val parent = view(source)
 borrow val alias = parent
 borrow val second = (alias)
 borrow val child = view(alias)
 println(child)
 println(parent)
 println(second)
 consume(source)
}"#,
            stdout: b"kept\nkept\nkept\nkept\n",
            allocations: 1,
        },
        Case {
            name: "alias-real-projection-storage",
            source: r#"value class Packet(val text: String)
class Resource(val text: String) { deinit() { println("drop") } }
fun packetView(source: Packet): borrow String from source = source.text
fun resourceView(source: Resource): borrow String from source = source.text
fun consumePacket(own source: Packet) { println("packet-owned") }
fun consumeResource(own source: Resource) { println("resource-owned") }
fun entry() {
 val packet = Packet("inline".clone())
 borrow val parent = packetView(packet)
 borrow val alias = parent
 println(alias)
 consumePacket(packet)
 val resource = Resource("heap".clone())
 borrow val resourceParent = resourceView(resource)
 borrow val resourceAlias = resourceParent
 println(resourceAlias)
 consumeResource(resource)
}"#,
            stdout: b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
            allocations: 3,
        },
        Case {
            name: "alias-resource-lexical",
            source: r#"class Resource(val n: Int) { deinit() { println("drop") } }
fun view(source: Resource): borrow Resource from source = source
fun observe(item: Resource) { println("seen") }
fun entry() {
 val source = Resource(7)
 borrow val parent = view(source)
 borrow val alias = parent
 observe(alias)
 println("after")
}"#,
            stdout: b"seen\nafter\ndrop\n",
            allocations: 1,
        },
        Case {
            name: "alias-map-resource",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observe(item: Resource) { if (item.n == 1) { println("first") } else { println("second") } }
fun entry() {
 var source = mutableMapOf<String, Resource>()
 source.put("key".clone(), Resource(1))
 borrow val parent = source.requireValue("key")
 borrow val alias = parent
 borrow val sibling = parent
 observe(alias)
 observe(sibling)
 source.put("key".clone(), Resource(2))
 borrow val next = source.requireValue("key")
 borrow val nextAlias = next
 observe(nextAlias)
}"#,
            stdout: b"first\nfirst\ndrop-one\nsecond\ndrop-two\n",
            allocations: 5,
        },
        Case {
            name: "alias-nullable-map-storage",
            source: r#"class Token(val n: Int)
fun observe(item: Token?) { if (item == null) { println("null") } else { println("full") } }
fun entry() {
 var source = mutableMapOf<String, Token?>()
 source.put("key".clone(), null)
 borrow val empty = source.requireValue("key")
 borrow val emptyAlias = empty
 observe(emptyAlias)
 source.put("key".clone(), Token(7))
 borrow val full = source.requireValue("key")
 borrow val fullAlias = full
 observe(fullAlias)
 source.put("after".clone(), null)
}"#,
            stdout: b"null\nfull\n",
            allocations: 5,
        },
    ]
}
