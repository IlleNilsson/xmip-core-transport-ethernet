//! One Ethernet II frame: destination, source, an 802.1Q tag where the
//! switch put one, an `EtherType`, and the payload. No preamble and no frame
//! check sequence — the adapter adds and strips those, and a raw socket
//! never sees them.
//!
//! The `EtherType` is what tells a frame from IP: `0x0800` is IPv4, `0x88a4`
//! is `EtherCAT`, `0x8892` is PROFINET, and Xmip's own is [`XMIP_ETHERTYPE`],
//! the first of the two IEEE 802 reserves for local experiments.

use std::fmt;
use std::str::FromStr;

use transport::error::{Result, TransportError, protocol_error};

/// The payload one standard frame carries: the IEEE 802.3 maximum
/// transmission unit.
pub const MTU: usize = 1500;

/// The payload a jumbo frame carries where the link allows it — no standard
/// says 9000, but every switch that does jumbo does at least this.
pub const JUMBO_MTU: usize = 9000;

/// The smallest payload a wire carries: a shorter one is padded to it by the
/// adapter, and a protocol above must know its own length. The in-process
/// link does not pad, so a Stream shorter than this comes back whole.
pub const MIN_PAYLOAD: usize = 46;

/// Xmip's own `EtherType`: IEEE 802 local experimental `EtherType` 1.
pub const XMIP_ETHERTYPE: u16 = 0x88b5;

/// The tag protocol identifier that says an 802.1Q tag follows.
pub const TAG_PROTOCOL: u16 = 0x8100;

/// The first value that is an `EtherType` rather than an 802.3 length.
const FIRST_ETHERTYPE: u16 = 0x0600;

/// The untagged header: two addresses and an `EtherType`.
const HEADER: usize = 14;

/// A 48-bit media access control address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mac(pub [u8; 6]);

impl Mac {
    /// The broadcast address, every bit set.
    pub const BROADCAST: Self = Self([0xff; 6]);

    /// True for a group address: multicast, or broadcast.
    #[must_use]
    pub const fn is_group(self) -> bool {
        self.0[0] & 0x01 != 0
    }
}

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, byte) in self.0.iter().enumerate() {
            if at > 0 {
                f.write_str(":")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Mac {
    type Err = TransportError;

    /// `aa:bb:cc:dd:ee:ff`, or with dashes.
    fn from_str(text: &str) -> Result<Self> {
        let bad = || protocol_error(format!("{text:?} is not a MAC address"));
        let mut bytes = [0u8; 6];
        let mut parts = text.split([':', '-']);
        for byte in &mut bytes {
            let part = parts.next().ok_or_else(bad)?;
            *byte = u8::from_str_radix(part, 16).map_err(|_| bad())?;
        }
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(Self(bytes))
    }
}

/// One frame as the link carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub destination: Mac,
    pub source: Mac,
    /// The 802.1Q tag control information — priority, drop eligible, VLAN
    /// — where the frame carries a tag.
    pub tag: Option<u16>,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

impl Frame {
    /// A frame, refusing what no link carries: a payload over
    /// [`JUMBO_MTU`], or a length in place of an `EtherType`.
    ///
    /// # Errors
    /// Outside those bounds.
    pub fn new(destination: Mac, source: Mac, ethertype: u16, payload: &[u8]) -> Result<Self> {
        if payload.len() > JUMBO_MTU {
            return Err(protocol_error(format!(
                "{} bytes is over the {JUMBO_MTU} a jumbo frame carries",
                payload.len()
            )));
        }
        if ethertype < FIRST_ETHERTYPE {
            return Err(protocol_error("an 802.3 length where an EtherType belongs"));
        }
        Ok(Self {
            destination,
            source,
            tag: None,
            ethertype,
            payload: payload.to_vec(),
        })
    }

    /// The same frame under an 802.1Q tag.
    #[must_use]
    pub const fn tagged(mut self, tag: u16) -> Self {
        self.tag = Some(tag);
        self
    }

    /// The VLAN the tag names, where there is one.
    #[must_use]
    pub fn vlan(&self) -> Option<u16> {
        self.tag.map(|tag| tag & 0x0fff)
    }

    /// The frame as bytes on the wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + 4 + self.payload.len());
        out.extend_from_slice(&self.destination.0);
        out.extend_from_slice(&self.source.0);
        if let Some(tag) = self.tag {
            out.extend_from_slice(&TAG_PROTOCOL.to_be_bytes());
            out.extend_from_slice(&tag.to_be_bytes());
        }
        out.extend_from_slice(&self.ethertype.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// The frame `bytes` carry.
    ///
    /// # Errors
    /// Shorter than a header, a tag with nothing after it, or an 802.3
    /// length where an `EtherType` belongs.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER {
            return Err(protocol_error("shorter than an Ethernet header"));
        }
        let mut destination = [0u8; 6];
        let mut source = [0u8; 6];
        destination.copy_from_slice(&bytes[..6]);
        source.copy_from_slice(&bytes[6..12]);
        let mut at = 12;
        let mut tag = None;
        let mut ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);
        if ethertype == TAG_PROTOCOL {
            let tagged = bytes
                .get(14..18)
                .ok_or_else(|| protocol_error("a tag with nothing after it"))?;
            tag = Some(u16::from_be_bytes([tagged[0], tagged[1]]));
            ethertype = u16::from_be_bytes([tagged[2], tagged[3]]);
            at = 16;
        }
        if ethertype < FIRST_ETHERTYPE {
            return Err(protocol_error("an 802.3 length where an EtherType belongs"));
        }
        Ok(Self {
            destination: Mac(destination),
            source: Mac(source),
            tag,
            ethertype,
            payload: bytes[at + 2..].to_vec(),
        })
    }

    /// `ethernet://<link>/<source>?type=0x<ethertype>`: where a frame came
    /// from.
    #[must_use]
    pub fn origin(&self, link: &str) -> String {
        format!(
            "ethernet://{link}/{}?type={:#06x}",
            self.source, self.ethertype
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(last: u8) -> Mac {
        Mac([0x02, 0, 0, 0, 0, last])
    }

    #[test]
    fn a_frame_encodes_its_header_and_decodes_back() {
        let frame = Frame::new(mac(2), mac(1), XMIP_ETHERTYPE, b"hello").expect("frame");
        let wire = frame.encode();
        assert_eq!(&wire[..6], &[0x02, 0, 0, 0, 0, 2]);
        assert_eq!(&wire[12..14], &[0x88, 0xb5]);
        assert_eq!(&wire[14..], b"hello");
        assert_eq!(Frame::decode(&wire).expect("decode"), frame);
        assert_eq!(
            frame.origin("loopback"),
            "ethernet://loopback/02:00:00:00:00:01?type=0x88b5"
        );
        assert!(frame.vlan().is_none());
    }

    #[test]
    fn a_tag_is_read_and_the_vlan_taken_from_it() {
        let frame = Frame::new(Mac::BROADCAST, mac(1), 0x0800, &[0x45])
            .expect("frame")
            .tagged(0x6007);
        let wire = frame.encode();
        assert_eq!(&wire[12..18], &[0x81, 0x00, 0x60, 0x07, 0x08, 0x00]);
        let back = Frame::decode(&wire).expect("decode");
        assert_eq!(back.vlan(), Some(7));
        assert_eq!(back.ethertype, 0x0800);
        assert!(back.destination.is_group());
        assert!(!back.source.is_group());
    }

    #[test]
    fn what_is_not_an_ethernet_frame_is_refused() {
        assert!(Frame::decode(&[0; 13]).is_err(), "short");
        let mut length = [0u8; 14];
        length[12..].copy_from_slice(&[0x00, 0x2e]);
        assert!(Frame::decode(&length).is_err(), "an 802.3 length");
        let mut tagged = [0u8; 16];
        tagged[12..14].copy_from_slice(&[0x81, 0x00]);
        assert!(Frame::decode(&tagged).is_err(), "a tag and nothing after");
        assert!(Frame::new(mac(1), mac(2), 0x0100, &[]).is_err());
        assert!(Frame::new(mac(1), mac(2), XMIP_ETHERTYPE, &[0; JUMBO_MTU + 1]).is_err());
    }

    #[test]
    fn a_mac_reads_and_writes_as_colon_separated_hex() {
        let mac: Mac = "02:00:00:00:00:ff".parse().expect("mac");
        assert_eq!(mac, Mac([2, 0, 0, 0, 0, 0xff]));
        assert_eq!(mac.to_string(), "02:00:00:00:00:ff");
        assert_eq!("02-00-00-00-00-FF".parse::<Mac>().expect("dashes"), mac);
        assert!("02:00:00:00:00".parse::<Mac>().is_err(), "short");
        assert!("02:00:00:00:00:ff:00".parse::<Mac>().is_err(), "long");
        assert!("02:00:00:00:00:zz".parse::<Mac>().is_err(), "not hex");
    }
}
