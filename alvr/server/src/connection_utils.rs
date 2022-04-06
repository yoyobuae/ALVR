use alvr_common::{prelude::*, ALVR_NAME, ALVR_VERSION};
use alvr_events::EventType;
use alvr_sockets::{ServerHandshakePacket, CONTROL_PORT, HANDSHAKE_PACKET_SIZE_BYTES, LOCAL_IP};
use std::{
    future::Future,
    io::{self, ErrorKind},
    net::{IpAddr, UdpSocket},
};

pub struct HandshakeSocket {
    socket: UdpSocket,
    buffer: [u8; HANDSHAKE_PACKET_SIZE_BYTES],
    expected_name: [u8; 16],
}

impl HandshakeSocket {
    pub fn new() -> StrResult<Self> {
        let socket = UdpSocket::bind((LOCAL_IP, CONTROL_PORT)).map_err(err!())?;
        socket.set_nonblocking(true).map_err(err!())?;

        let mut expected_name = [0; 16];
        expected_name.copy_from_slice(ALVR_NAME.as_bytes());

        Ok(Self {
            socket,
            buffer: [0; HANDSHAKE_PACKET_SIZE_BYTES],
            expected_name,
        })
    }

    pub fn recv_non_blocking(&self) -> StrResult<Option<IpAddr>> {
        let (size, address) = match self.socket.recv_from(&mut self.buffer) {
            Ok(pair) => pair,
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    return Ok(None);
                } else {
                    return fmt_e!("{e}");
                }
            }
        };

        if size == HANDSHAKE_PACKET_SIZE_BYTES && self.buffer[..16] == name {
            let mut protocol_id_bytes = [0; 8];
            protocol_id_bytes.copy_from_slice(&self.buffer[16..24]);
            let received_protocol_id = u64::from_le_bytes(protocol_id_bytes);

            if received_protocol_id == alvr_common::protocol_id() {
                Ok(Some(address))
            } else {
                alvr_events::send_event(EventType::ClientFoundWrongVersion(format!(
                    "Expected protocol ID {}, Found {received_protocol_id}",
                    alvr_common::protocol_id()
                )));
                Ok(None)
            }
        } else if self.buffer[0..4] == 0_u32.to_le_bytes()
            && self.buffer[4..12] == 4_u64.to_le_bytes()
            && self.buffer[12..16] == b"ALVR"
        {
            alvr_events::send_event(EventType::ClientFoundWrongVersion("v14 to v18".into()));
        } else if self.buffer[..5] = b"\x01ALVR" {
            // People might still download the client from the polygraphene reposiory
            alvr_events::send_event(EventType::ClientFoundWrongVersion("v11 or previous".into()));
        } else {
            // Unexpected packet.
            // Note: no need to check for v12 and v13, not found in the wild anymore
            Ok(None)
        }
    }
}
