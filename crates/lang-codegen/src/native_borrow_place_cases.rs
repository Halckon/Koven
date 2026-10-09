//! Approved stable initializer sources; normal compilation and exact cleanup only.
use crate::native_borrow_last_use_cases::Case;

pub(crate) fn cases() -> [Case; 5] {
    [
        Case {
            name: "stable-owned-root",
            source: r#"fun view(source: String): borrow String from source = source
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "root".clone()
 borrow val parent = (source)
 borrow val alias = parent
 borrow val child = view(alias)
 println(child)
 println(parent)
 consume(source)
}"#,
            stdout: b"root\nroot\nroot\n",
            allocations: 1,
        },
        Case {
            name: "stable-inline-fields",
            source: r#"value class Packet(val text: String)
value class Envelope(val packet: Packet)
fun consume(own source: Envelope) { println("owned") }
fun entry() {
 val source = Envelope(Packet("inline".clone()))
 borrow val text = (source.packet.text)
 borrow val alias = text
 println(alias)
 consume(source)
}"#,
            stdout: b"inline\nowned\n",
            allocations: 1,
        },
        Case {
            name: "stable-heap-fields",
            source: r#"value class Packet(val text: String)
class Resource(val packet: Packet) { deinit() { println("drop") } }
fun consume(own source: Resource) { println("owned") }
fun entry() {
 val source = Resource(Packet("heap".clone()))
 borrow val text = source.packet.text
 borrow val alias = (text)
 println(alias)
 consume(source)
}"#,
            stdout: b"heap\nowned\ndrop\n",
            allocations: 2,
        },
        Case {
            name: "stable-parent-field",
            source: r#"value class Packet(val text: String)
fun view(source: Packet): borrow Packet from source = source
fun observe(source: Packet) { println("parent") }
fun consume(own source: Packet) { println("owned") }
fun entry() {
 val source = Packet("projected".clone())
 borrow val parent = view(source)
 borrow val text = parent.text
 borrow val alias = text
 println(alias)
 observe(parent)
 consume(source)
}"#,
            stdout: b"projected\nparent\nowned\n",
            allocations: 1,
        },
        Case {
            name: "stable-map-slot-field",
            source: r#"value class Packet(val text: String)
fun entry() {
 var source = mutableMapOf<String, Packet>()
 source.put("key", Packet("slot".clone()))
 borrow val parent = source.requireValue("key")
 borrow val text = parent.text
 borrow val alias = text
 println(alias)
 source.put("key", Packet("changed".clone()))
 println("restored")
}"#,
            stdout: b"slot\nrestored\n",
            // One table buffer and two explicit String clones; the header is an SSA aggregate.
            allocations: 3,
        },
    ]
}
