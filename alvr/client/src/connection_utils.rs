use alvr_common::{prelude::*, ALVR_NAME};
use alvr_sockets::{CONTROL_PORT, HANDSHAKE_PACKET_SIZE_BYTES, LOCAL_IP};
use std::{
    net::{Ipv4Addr, UdpSocket},
    time::Duration,
};

const CLIENT_HANDSHAKE_RESEND_INTERVAL: Duration = Duration::from_secs(1);

struct HandshakeSocket {
    socket: UdpSocket,
    packet: [u8; HANDSHAKE_PACKET_SIZE_BYTES],
}

impl HandshakeSocket {
    pub fn new() -> StrResult<Self> {
        let socket = UdpSocket::bind((LOCAL_IP, CONTROL_PORT)).map_err(err!())?;
        socket.set_broadcast(true).map_err(err!())?;

        let mut packet = [0; 24];
        packet[0..ALVR_NAME.len()].copy_from_slice(ALVR_NAME.as_bytes());
        packet[16..24].copy_from_slice(&alvr_common::protocol_id().to_le_bytes());

        Ok(Self { socket, packet })
    }

    pub fn broadcast(&self) -> StrResult {
        self.socket
            .send_to(&self.packet, (Ipv4Addr::BROADCAST, CONTROL_PORT))
            .map_err(err!())?;
        Ok(())
    }
}
