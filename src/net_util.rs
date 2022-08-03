#[allow(unused_imports)] use crate::prelude::*;

use std::ops::Range;
use tokio::net::TcpListener;

pub async fn tcp_port_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

#[derive(Debug)]
pub struct TcpPortIssuer {
    range: Range<u16>,
}

impl TcpPortIssuer {
    pub fn new(range: Range<u16>) -> Self {
        TcpPortIssuer { range }
    }

    pub async fn get_port(&mut self) -> Result<u16> {
        // TODO do better
        for port in self.range.by_ref() {
            if tcp_port_available(port).await {
                self.range = (port + 1) .. self.range.end;
                return Ok(port)
            }
        }
        bail!("no ports available!")
    }
}