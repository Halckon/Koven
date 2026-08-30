//! Koven 语言服务器入口。

mod analysis;
mod definition;
mod diagnostic_adapter;
mod position_adapter;
mod server;
mod source_set;
mod unit_session;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
