//! 真实 stdio 进程的格式化协议回归；不复刻 framing 或 frontend 算法。

use std::{
    io::BufReader,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use lang_frontend::{formatting::format_source, source::SourceMap};
use lsp_server::{Message, Notification, Request, RequestId};
use lsp_types::TextEdit;
use serde_json::{Value, json};

struct Server {
    child: Child,
    messages: Receiver<Message>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lang-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start actual LSP");
        let mut output = BufReader::new(child.stdout.take().expect("stdout"));
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            while let Some(message) = Message::read(&mut output).expect("LSP framing") {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self { child, messages }
    }

    fn send(&mut self, message: Message) {
        message
            .write(self.child.stdin.as_mut().expect("stdin"))
            .expect("write message");
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(Message::Notification(Notification::new(
            method.to_owned(),
            params,
        )));
    }

    fn request(&mut self, id: i32, method: &str, params: Value) -> Value {
        self.send(Message::Request(Request::new(
            RequestId::from(id),
            method.to_owned(),
            params,
        )));
        loop {
            let message = self
                .messages
                .recv_timeout(Duration::from_secs(10))
                .expect("LSP response");
            match message {
                Message::Response(response) => {
                    assert_eq!(response.id, RequestId::from(id));
                    return response.response_result.expect("successful request");
                }
                Message::Notification(notification) => {
                    assert_eq!(notification.method, "textDocument/publishDiagnostics");
                }
                Message::Request(_) => panic!("unexpected server request"),
            }
        }
    }

    fn finish(mut self) {
        self.request(100, "shutdown", Value::Null);
        self.notify("exit", Value::Null);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().expect("process status") {
                assert!(status.success(), "LSP exit: {status}");
                break;
            }
            assert!(Instant::now() < deadline, "LSP did not exit after shutdown");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // 断言失败时也释放本测试自己的进程，正常路径已完成 shutdown/exit。
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn real_stdio_formatting_matches_frontend_and_tracks_open_change_close() {
    for project in [false, true] {
        let mut server = Server::start();
        let uri = "file:///not-read/main.ko";
        let mut params = json!({"capabilities": {}});
        if project {
            params["initializationOptions"] = json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set", "version": 1, "roots": ["main"],
                "sources": [{"root": "main", "logicalPath": "main.ko", "uri": uri, "text": "val base=0"}]
            }}});
        }
        let initialized = server.request(1, "initialize", params);
        assert_eq!(
            initialized["capabilities"]["documentFormattingProvider"],
            true
        );
        server.notify("initialized", json!({}));
        let formatting =
            json!({"textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": true}});
        assert!(
            server
                .request(2, "textDocument/formatting", formatting.clone())
                .is_null()
        );
        let text = "fun f():Unit {\r\n// keep 😀\r\nval text=\"${1+2}\"\r\n}\r\n";
        server.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri, "languageId": "koven", "version": 1, "text": text
            }}),
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("oracle.ko", text).expect("source");
        let expected = format_source(&sources, source).expect("frontend format");
        let edits: Vec<TextEdit> = serde_json::from_value(server.request(
            3,
            "textDocument/formatting",
            formatting.clone(),
        ))
        .expect("edits");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, expected);
        assert_eq!(edits[0].range.start, lsp_types::Position::new(0, 0));
        assert_eq!(edits[0].range.end, lsp_types::Position::new(4, 0));
        for (id, version, text) in [(4, 2, "val broken ="), (5, 3, expected.as_str())] {
            server.notify("textDocument/didChange", json!({
                "textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]
            }));
            let result = server.request(id, "textDocument/formatting", formatting.clone());
            if version == 2 {
                assert!(result.is_null());
            } else {
                assert_eq!(result, json!([]));
            }
        }
        server.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri}}),
        );
        assert!(
            server
                .request(6, "textDocument/formatting", formatting)
                .is_null()
        );
        server.finish();
    }
}
