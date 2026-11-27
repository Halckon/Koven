//! Koven 语言服务器入口。

mod analysis;
mod definition;
mod diagnostic_adapter;
mod formatting;
mod ide_features;
mod position_adapter;
mod rename;
mod server;
mod source_set;
mod unit_analysis_snapshot;
mod unit_session;
mod workspace_manifest;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut options = server::ServerOptions::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--workspace-manifest" => options.workspace_manifest = true,
            "--diagnostic-context" => options.diagnostic_context = true,
            other => {
                return Err(format!("unsupported lang-lsp argument: {other}").into());
            }
        }
    }
    let (connection, io_threads) = lsp_server::Connection::stdio();
    server::run_with_options(connection, options)?;
    io_threads.join()?;
    Ok(())
}

#[cfg(test)]
mod single_file_oracle;
