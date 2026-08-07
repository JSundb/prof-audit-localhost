use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Instant;

use crate::core::{Request};

#[derive(Debug)]
pub struct ClientConnection {
    pub stream: TcpStream,
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
    pub read_buffer: Vec<u8>,
    pub write_buffer: Vec<u8>,
    pub write_registered: bool,
    pub request_at: Option<Instant>,
    pub should_close: bool,
}

impl ClientConnection {
    pub fn new(stream: TcpStream, peer_addr: SocketAddr) -> std::io::Result<Self> {
        let local_addr = stream.local_addr()?;

        stream.set_nonblocking(true)?;

        Ok(Self {
            stream,
            peer_addr,
            local_addr,
            read_buffer: Vec::with_capacity(4096),
            write_buffer: Vec::new(),
            write_registered: false,
            request_at: None,
            should_close: false,
        })
    }

    /// Attempt to read from the stream and append to the buffer.
    pub fn read_into_buffer(&mut self) -> std::io::Result<usize> {
        let mut temp_buf = [0u8; 4096];
        match self.stream.read(&mut temp_buf) {
            Ok(0) => Ok(0), // Connection closed
            Ok(n) => {
                self.read_buffer.extend_from_slice(&temp_buf[..n]);
                
                if self.request_at.is_none() {
                    self.request_at = Some(Instant::now());
                }
                Ok(n)
            }
            Err(e) => Err(e),
        }
    }

    pub fn parse_request(&mut self) -> Option<Request> {
        if let Some((request, consumed)) = Request::parse(&self.read_buffer) {
            self.read_buffer.drain(0..consumed);
            self.request_at = None;
            return Some(request);
        } 
        None
    }

    pub fn queue_response(&mut self, data: &[u8]) {
        self.write_buffer.extend_from_slice(data);
    }

    pub fn flush_write_buffer(&mut self) -> std::io::Result<bool> {
        while !self.write_buffer.is_empty() {
            match self.stream.write(&self.write_buffer) {
                Ok(0) => {
                    // Connection closed
                    return Ok(false);
                }
                Ok(n) => {
                    self.write_buffer.drain(0..n);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // Can't write more now, try again later
                    return Ok(true);
                }
                Err(e) => {
                    return Err(e)
                },
            }
        }

        Ok(true)
    }

    pub fn has_pending_write(&self) -> bool {
        !self.write_buffer.is_empty()
    }

}
