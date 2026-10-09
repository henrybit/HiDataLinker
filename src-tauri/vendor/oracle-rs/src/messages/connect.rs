//! CONNECT message
//!
//! The CONNECT packet is sent by the client to initiate a connection to the Oracle server.
//!
//! Packet structure (after 8-byte TNS header):
//! ```text
//! Offset | Size | Description
//! -------+------+------------------
//!      0 |    2 | Protocol version (desired)
//!      2 |    2 | Protocol version (minimum)
//!      4 |    2 | Service options
//!      6 |    2 | SDU size
//!      8 |    2 | TDU size
//!     10 |    2 | Protocol characteristics
//!     12 |    2 | Line turnaround (0)
//!     14 |    2 | Value of 1
//!     16 |    2 | Connect data length
//!     18 |    2 | Connect data offset
//!     20 |    4 | Max receivable data (0)
//!     24 |    1 | NSI flags 1
//!     25 |    1 | NSI flags 2
//!     26 |   24 | Obsolete/reserved (zeros)
//!     50 |    4 | SDU (large)
//!     54 |    4 | TDU (large)
//!     58 |    4 | Connect flags 1
//!     62 |    4 | Connect flags 2
//!     66 |    8 | Cross facility item 1 (0)
//!     74 |    n | Connect data (TNS connect descriptor)
//! ```

use bytes::Bytes;

use crate::buffer::WriteBuffer;
use crate::config::Config;
use crate::constants::{
    connection, nsi_flags, service_options, version, PacketType, PACKET_HEADER_SIZE,
};
use crate::error::Result;
use crate::packet::PacketHeader;

/// Connect message sent to initiate a connection
#[derive(Debug)]
pub struct ConnectMessage {
    /// Desired protocol version
    pub version_desired: u16,
    /// Minimum acceptable protocol version
    pub version_minimum: u16,
    /// Service options flags
    pub service_options: u16,
    /// Session Data Unit size
    pub sdu: u32,
    /// Transport Data Unit size
    pub tdu: u32,
    /// Protocol characteristics
    pub protocol_characteristics: u16,
    /// NSI flags
    pub nsi_flags: u8,
    /// Connect flags 1
    pub connect_flags_1: u32,
    /// Connect flags 2
    pub connect_flags_2: u32,
    /// Connect data (TNS descriptor)
    pub connect_data: String,
    /// Whether OOB (Out of Band) is supported
    pub supports_oob: bool,
}

impl ConnectMessage {
    /// Create a new CONNECT message from configuration
    pub fn from_config(config: &Config) -> Self {
        let connect_data = config.build_connect_string();

        let desired = desired_protocol(config);
        // 11g listeners reset the socket on the 12c layout (offset 74, DISABLE_NA)
        // and also on the 34-byte 10g packet. They accept the 70-byte form:
        // service options 0x0C01, NSI 0x01, connect data at offset 70.
        let legacy = desired < version::MIN_LARGE_SDU;
        let (service_opts, connect_flags_2, nsi_flags) = if legacy {
            (
                service_options::DONT_CARE
                    | service_options::CAN_RECV_ATTENTION
                    | service_options::CAN_SEND_ATTENTION,
                0u32,
                0x01u8,
            )
        } else {
            (
                service_options::DONT_CARE | service_options::CAN_RECV_ATTENTION,
                connection::CHECK_OOB,
                nsi_flags::SUPPORT_SECURITY_RENEG | nsi_flags::DISABLE_NA,
            )
        };

        Self {
            version_desired: desired,
            version_minimum: if legacy {
                version::MINIMUM
            } else {
                version::MIN_ACCEPTED
            },
            service_options: service_opts,
            sdu: config.sdu,
            tdu: connection::DEFAULT_TDU as u32,
            protocol_characteristics: connection::PROTOCOL_CHARACTERISTICS,
            nsi_flags,
            connect_flags_1: 0,
            connect_flags_2,
            connect_data,
            supports_oob: !legacy,
        }
    }
}

fn desired_protocol(config: &Config) -> u16 {
    if config.protocol_desired >= version::MIN_ACCEPTED {
        config.protocol_desired
    } else {
        version::DESIRED
    }
}

impl ConnectMessage {
    fn legacy_packet(&self) -> bool {
        self.version_desired < version::MIN_LARGE_SDU
    }

    /// Build the CONNECT packet bytes
    pub fn build(&self) -> Result<Bytes> {
        self.build_packet(true)
    }

    fn build_packet(&self, include_data: bool) -> Result<Bytes> {
        if self.legacy_packet() {
            self.build_legacy(include_data)
        } else {
            self.build_modern(include_data)
        }
    }

    /// 10g/11g CONNECT packet. Connect data starts at byte 70.
    ///
    /// A 34-byte packet (data immediately after the NSI flags) makes 11.1 and
    /// 11.2 listeners close the socket. The 70-byte header keeps the 32-bit
    /// SDU/TDU fields and leaves Native Network Encryption enabled.
    fn build_legacy(&self, include_data: bool) -> Result<Bytes> {
        const DATA_OFFSET: u16 = 70;
        let connect_data_bytes = self.connect_data.as_bytes();
        let mut buf = WriteBuffer::with_capacity(256);
        buf.write_zeros(PACKET_HEADER_SIZE)?;
        buf.write_u16_be(self.version_desired)?;
        buf.write_u16_be(self.version_minimum)?;
        buf.write_u16_be(self.service_options)?;
        buf.write_u16_be(self.sdu.min(65535) as u16)?;
        buf.write_u16_be(self.tdu.min(65535) as u16)?;
        buf.write_u16_be(self.protocol_characteristics)?;
        buf.write_u16_be(0)?;
        buf.write_u16_be(1)?;
        buf.write_u16_be(connect_data_bytes.len() as u16)?;
        buf.write_u16_be(DATA_OFFSET)?;
        buf.write_u32_be(0)?;
        buf.write_u8(self.nsi_flags)?;
        buf.write_u8(self.nsi_flags)?;
        buf.write_zeros(24)?;
        buf.write_u32_be(self.sdu)?;
        buf.write_u32_be(self.tdu)?;
        buf.write_u32_be(0)?;
        if include_data {
            buf.write_bytes(connect_data_bytes)?;
        }
        Self::finish_packet(buf, false)
    }

    /// 12c and later CONNECT packet. Connect data starts at byte 74.
    fn build_modern(&self, include_data: bool) -> Result<Bytes> {
        let connect_data_bytes = self.connect_data.as_bytes();
        let connect_data_len = connect_data_bytes.len();

        // Build the main CONNECT packet
        let mut buf = WriteBuffer::with_capacity(512);

        // Reserve space for header (will be written at the end)
        buf.write_zeros(PACKET_HEADER_SIZE)?;

        // Protocol versions
        buf.write_u16_be(self.version_desired)?;
        buf.write_u16_be(self.version_minimum)?;

        // Service options
        buf.write_u16_be(self.service_options)?;

        // SDU/TDU (16-bit for compatibility)
        buf.write_u16_be(self.sdu.min(65535) as u16)?;
        buf.write_u16_be(self.tdu.min(65535) as u16)?;

        // Protocol characteristics
        buf.write_u16_be(self.protocol_characteristics)?;

        // Line turnaround (unused)
        buf.write_u16_be(0)?;

        // Value of 1 (required)
        buf.write_u16_be(1)?;

        // Connect data length
        buf.write_u16_be(connect_data_len as u16)?;

        // Connect data offset (from start of packet)
        // Fixed at 74 bytes for modern protocol
        buf.write_u16_be(74)?;

        // Max receivable data (unused, 0)
        buf.write_u32_be(0)?;

        // NSI flags (connect flags 0 and 1)
        buf.write_u8(self.nsi_flags)?;
        buf.write_u8(self.nsi_flags)?;

        // Obsolete bytes (24 bytes of zeros)
        buf.write_zeros(24)?;

        // SDU (32-bit)
        buf.write_u32_be(self.sdu)?;

        // TDU (32-bit)
        buf.write_u32_be(self.tdu)?;

        // Connect flags
        buf.write_u32_be(self.connect_flags_1)?;
        buf.write_u32_be(self.connect_flags_2)?;

        // Now we're at offset 74
        if include_data {
            buf.write_bytes(connect_data_bytes)?;
        }
        Self::finish_packet(buf, false)
    }

    fn finish_packet(buf: WriteBuffer, large_sdu: bool) -> Result<Bytes> {
        let total_len = buf.len() as u32;
        let header = PacketHeader::new(PacketType::Connect, total_len);
        let mut header_buf = WriteBuffer::with_capacity(PACKET_HEADER_SIZE);
        header.write(&mut header_buf, large_sdu)?;
        let mut result = buf.into_inner();
        result[..PACKET_HEADER_SIZE].copy_from_slice(header_buf.as_slice());
        Ok(result.freeze())
    }

    fn data_packet(connect_data: &[u8]) -> Result<Bytes> {
        let mut data_buf = WriteBuffer::with_capacity(PACKET_HEADER_SIZE + 2 + connect_data.len());
        data_buf.write_zeros(PACKET_HEADER_SIZE)?;
        data_buf.write_u16_be(0)?;
        data_buf.write_bytes(connect_data)?;
        let data_len = data_buf.len() as u32;
        let data_header = PacketHeader::new(PacketType::Data, data_len);
        let mut data_header_buf = WriteBuffer::with_capacity(PACKET_HEADER_SIZE);
        data_header.write(&mut data_header_buf, false)?;
        let mut data_result = data_buf.into_inner();
        data_result[..PACKET_HEADER_SIZE].copy_from_slice(data_header_buf.as_slice());
        Ok(data_result.freeze())
    }

    /// Build the CONNECT packet and optional DATA packet for large connect strings
    ///
    /// Returns a tuple of (CONNECT packet, optional DATA packet)
    pub fn build_with_continuation(&self) -> Result<(Bytes, Option<Bytes>)> {
        let connect_data_bytes = self.connect_data.as_bytes();
        let needs_split = connect_data_bytes.len() > connection::MAX_CONNECT_DATA as usize;
        if !needs_split {
            return Ok((self.build()?, None));
        }
        Ok((
            self.build_packet(false)?,
            Some(Self::data_packet(connect_data_bytes)?),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_message_from_config() {
        let config = Config::new("localhost", 1521, "FREEPDB1", "user", "pass");
        let msg = ConnectMessage::from_config(&config);

        assert_eq!(msg.version_desired, version::DESIRED);
        assert_eq!(msg.version_minimum, version::MIN_ACCEPTED);
        assert_eq!(msg.sdu, config.sdu);
        assert!(msg.connect_data.contains("FREEPDB1"));
        assert!(msg.connect_data.contains("localhost"));
    }

    #[test]
    fn test_connect_message_build() {
        let config = Config::new("localhost", 1521, "FREEPDB1", "user", "pass");
        let msg = ConnectMessage::from_config(&config);
        let packet = msg.build().unwrap();

        // Check packet header
        assert!(packet.len() > PACKET_HEADER_SIZE);
        assert_eq!(packet[4], PacketType::Connect as u8);

        // Check version in packet
        assert_eq!(packet[8], (version::DESIRED >> 8) as u8);
        assert_eq!(packet[9], (version::DESIRED & 0xff) as u8);
        assert_eq!(u16::from_be_bytes([packet[26], packet[27]]), 74);
    }

    #[test]
    fn test_connect_message_small_data() {
        let config = Config::new("localhost", 1521, "SVC", "u", "p");
        let msg = ConnectMessage::from_config(&config);
        let (connect, data) = msg.build_with_continuation().unwrap();

        // Should fit in single packet
        assert!(data.is_none());
        assert!(connect.len() > PACKET_HEADER_SIZE + 66);
    }

    #[test]
    fn test_connect_message_large_data() {
        // Create a config with a very long service name
        let long_service = "A".repeat(300);
        let config = Config::new("localhost", 1521, &long_service, "u", "p");
        let msg = ConnectMessage::from_config(&config);
        let (_connect, data) = msg.build_with_continuation().unwrap();

        // Should need separate DATA packet
        assert!(data.is_some());

        let data_packet = data.unwrap();
        // DATA packet should have header + data flags + connect string
        assert!(data_packet.len() > PACKET_HEADER_SIZE + 2);
        assert_eq!(data_packet[4], PacketType::Data as u8);
    }

    #[test]
    fn test_connect_message_offers_11g() {
        let config =
            Config::with_sid("172.19.3.141", 7026, "DB11G", "user", "pass").protocol_version(314);
        let msg = ConnectMessage::from_config(&config);
        assert_eq!(msg.version_desired, 314);
        assert_eq!(msg.version_minimum, version::MINIMUM);
        assert_eq!(msg.nsi_flags, 0x01);
        assert_eq!(msg.service_options, 0x0C01);

        let packet = msg.build().unwrap();
        assert_eq!(packet[4], PacketType::Connect as u8);
        assert_eq!(u16::from_be_bytes([packet[8], packet[9]]), 314);
        assert_eq!(
            u16::from_be_bytes([packet[10], packet[11]]),
            version::MINIMUM
        );
        assert_eq!(u16::from_be_bytes([packet[12], packet[13]]), 0x0C01);
        assert_eq!(u16::from_be_bytes([packet[26], packet[27]]), 70);
        assert_eq!(packet[32], 0x01);
        assert_ne!(packet[32] & 0x04, 0x04);
        assert_eq!(u32::from_be_bytes(packet[58..62].try_into().unwrap()), 8192);
        let descriptor = std::str::from_utf8(&packet[70..]).unwrap();
        assert!(descriptor.contains("(SID=DB11G)"));
        assert!(descriptor.contains("(PORT=7026)"));
        assert!(!descriptor.contains("SERVICE_NAME"));
        // Same shape as JDBC thin `@host:port:SID`
        assert!(!descriptor.contains("SERVER="));
    }
}
