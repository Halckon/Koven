//! single/unit 共用普通源码，覆盖借用 storage 与投影的真实执行。
pub(crate) fn cases(callable: &str) -> Vec<(&'static str, String, &'static [u8])> {
    vec![
        (
            "generic-storage",
            format!(
                r#"
value class Packet(val text: String)
class Resource(val text: String) {{ deinit() {{ println("drop") }} }}
fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun observeText(source: String) {{ println(source) }}
fun observePacket(source: Packet) {{ println("packet") }}
fun observeResource(source: Resource) {{ println("resource") }}
fun consumeText(own source: String) {{ println(source) }}
fun consumePacket(own source: Packet) {{ println("packet-owned") }}
fun consumeResource(own source: Resource) {{ println("resource-owned") }}
fun entry() {{
 val text = "kept".clone()
 {{ borrow val item = {callable}(text); observeText(item) }}
 consumeText(text)
 val packet = Packet("payload".clone())
 {{ borrow val item = {callable}(packet); observePacket(item) }}
 consumePacket(packet)
 val resource = Resource("payload".clone())
 {{ borrow val item = {callable}(resource); observeResource(item) }}
 consumeResource(resource)
}}
"#
            ),
            b"kept\nkept\npacket\npacket-owned\nresource\nresource-owned\ndrop\n",
        ),
        (
            "projected-storage",
            format!(
                r#"
value class Packet(val text: String)
class Resource(val text: String) {{ deinit() {{ println("drop") }} }}
fun packetView(source: Packet): borrow String from source = source.text
fun packetWrap(source: Packet): borrow String from source = packetView(source)
fun resourceView(source: Resource): borrow String from source = source.text
fun resourceWrap(source: Resource): borrow String from source = resourceView(source)
fun consumePacket(own source: Packet) {{ println("packet-owned") }}
fun consumeResource(own source: Resource) {{ println("resource-owned") }}
fun entry() {{
 val packet = Packet("inline".clone())
 {{ borrow val item = packet{}(packet); println(item) }}
 consumePacket(packet)
 val resource = Resource("heap".clone())
 {{ borrow val item = resource{}(resource); println(item) }}
 consumeResource(resource)
}}
"#,
                if callable == "view" { "View" } else { "Wrap" },
                if callable == "view" { "View" } else { "Wrap" }
            ),
            b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
        ),
        (
            "nullable-storage",
            format!(
                r#"
class Token(val n: Int)
fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun observe(source: Token?) {{ println("nullable-location") }}
fun consume(own source: Token?) {{
 println("nullable-owned")
}}
fun entry() {{
 val empty: Token? = null
 {{ borrow val item = {callable}(empty); observe(item) }}
 consume(empty)
 val full: Token? = Token(7)
 {{ borrow val item = {callable}(full); observe(item) }}
 consume(full)
}}
"#
            ),
            b"nullable-location\nnullable-owned\nnullable-location\nnullable-owned\n",
        ),
    ]
}
