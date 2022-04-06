//! Stream-ID-aware TCP socket with packet interface. The stream ID is used to select the correct
//! buffer pool for the receive end, to reduce unnecessarily large allocations.

use alvr_common::{parking_lot::Mutex, prelude::*};
use alvr_common::{RelaxedAtomic, StrResult};
use std::{
    collections::{HashMap, VecDeque},
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    sync::Arc,
};

// Writes all buffer bytes into the socket. In case the socket returns early, retry, in which case
// the socket could be temporarily locked by the read thread.
// Return Ok(true) if success, Ok(false) if running, in which case the socket SHOULD be closed
// because the packet delimiters are out of sync.
fn interruptible_write_all(
    socket: &Mutex<TcpStream>,
    mut buffer: &[u8],
    running: &RelaxedAtomic,
) -> StrResult<bool> {
    loop {
        let res = socket.lock().write(buffer);

        if !running.value() {
            return Ok(false);
        }

        match res {
            Ok(size) => {
                if size == buffer.len() {
                    return Ok(true);
                } else {
                    buffer = &buffer[..size];
                }
            }
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted {
                    continue;
                } else {
                    return fmt_e!("{e}");
                }
            }
        }
    }
}

fn interruptible_read_all(
    socket: &Mutex<TcpStream>,
    mut buffer: &mut [u8],
    running: &RelaxedAtomic,
) -> StrResult<bool> {
    loop {
        let res = socket.lock().read(buffer);

        if !running.value() {
            return Ok(false);
        }

        match res {
            Ok(size) => {
                if size == buffer.len() {
                    return Ok(true);
                } else {
                    buffer = &mut buffer[..size];
                }
            }
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted {
                    continue;
                } else {
                    return fmt_e!("{e}");
                }
            }
        }
    }
}

// Length-delimited-coded TCP send wrapper
pub struct LdcTcpSender {
    pub socket: Arc<Mutex<TcpStream>>,

    // The stream cursor is in a valid position. Becomes false if the send operation is interrupted.
    pub valid: bool,
}

impl LdcTcpSender {
    pub fn new(socket: Arc<Mutex<TcpStream>>) -> Self {
        Self {
            socket,
            valid: true,
        }
    }

    // Note: send() takes mut self because it cannot have concurrent send actions
    pub fn send(
        &mut self,
        stream_id: u8,
        buffer: &[u8],
        running: &RelaxedAtomic,
    ) -> StrResult<bool> {
        if !self.valid {
            return Ok(false);
        }

        let mut prefix = [0; 9];
        prefix[0] = stream_id;
        prefix.copy_from_slice(&(buffer.len() as u64).to_le_bytes());

        if !interruptible_write_all(&self.socket, &prefix, running).map_err(err!())? {
            self.valid = false;
            return Ok(false);
        }

        if !interruptible_write_all(&self.socket, &buffer, running).map_err(err!())? {
            self.valid = false;
            return Ok(false);
        }

        Ok(true)
    }
}

// Length-delimited-coded TCP receive wrapper
// This is optimized with the assumption that packets from the same stream ID are similar in size.
pub struct LdcTcpReceiver {
    socket: Arc<Mutex<TcpStream>>,
    buffers: HashMap<u8, VecDeque<Vec<u8>>>,
    valid: bool,
}

impl LdcTcpReceiver {
    pub fn new(socket: Arc<Mutex<TcpStream>>) -> Self {
        Self {
            socket,
            buffers: HashMap::new(),
            valid: true,
        }
    }

    // Return a buffer for a specific stream ID.
    // Why not providing the buffer directly in rcev()? At the time of receive we don't know what
    // type of packet we get and the buffer should be selected from the correct pool for the
    // specific stream ID.
    pub fn push_buffer(&mut self, stream_id: u8, buffer: Vec<u8>) {
        self.buffers.entry(stream_id).or_default().push_back(buffer);
    }

    // Receive a packet. If there are no available buffers for a specific stream ID pool, or the
    // available buffers are too small, a new buffer is allocated.
    // Note: recv() takes mut self because it cannot have concurrent send actions
    pub fn recv(&mut self, running: &RelaxedAtomic) -> StrResult<Option<(u8, Vec<u8>)>> {
        if !self.valid {
            return Ok(None);
        }

        let mut prefix = [0; 9];
        if !interruptible_read_all(&self.socket, &mut prefix, running).map_err(err!())? {
            self.valid = false;
            return Ok(None);
        }

        let stream_id = prefix[0];

        let mut buffer_size_buffer = [0; 8];
        buffer_size_buffer.copy_from_slice(&prefix[1..9]);
        let buffer_size = u64::from_le_bytes(buffer_size_buffer) as usize;

        let mut buffer = self
            .buffers
            .entry(stream_id)
            .or_default()
            .pop_front()
            .unwrap_or_default();

        // Note: it performs a reallocation if necessary
        buffer.resize(buffer_size, 0);

        if !interruptible_read_all(&self.socket, &mut buffer, running).map_err(err!())? {
            self.valid = false;
            return Ok(None);
        }

        Ok(Some((stream_id, buffer)))
    }
}
