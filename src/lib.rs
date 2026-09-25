#![forbid(unsafe_code)]

//! Streams that arrive as Ethernet frames, below IP. One frame is one
//! Stream: up to the MTU under Xmip's own `EtherType`, the addresses beside it.
//!
//! Most of the plant floor never reaches IP. `EtherCAT`, PROFINET's real-time
//! classes, GOOSE between protection relays — each is a Layer 2 protocol
//! with its own `EtherType`, and each needs the same thing: a frame put on a
//! wire and the frames that come back. This transport is that frame
//! ([`Frame`]) and that wire ([`Link`]), and a transport over both that
//! carries a Stream under [`XMIP_ETHERTYPE`]; the protocols above reuse the
//! frame and the link rather than knowing a wire of their own.
//!
//! Two links. [`Loopback`] is an in-process wire that every test and every
//! box drives, the way can-bus has a loopback bus. The deployment route is
//! the operating system's raw socket — `AF_PACKET` on Linux, a BPF device
//! on the BSDs and macOS, Npcap on Windows — which needs a privilege the
//! build box does not grant and a binding no crate in this estate writes
//! without `unsafe`; a node with the privilege implements [`Link`] over it
//! and everything above is unchanged. A raw socket pads a short payload to
//! [`MIN_PAYLOAD`] and the protocol above knows its own length; the
//! in-process link does not, and a Stream shorter than that comes back
//! whole.
//!
//! The origin URI carries what the header knew:
//! `ethernet://<link>/<source mac>?type=0x88b5`.

pub mod frame;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

pub use frame::{Frame, JUMBO_MTU, MIN_PAYLOAD, MTU, Mac, XMIP_ETHERTYPE};
use transport::ceiling;
use transport::error::{Result, protocol_error};
use transport::held::Held;
use transport::loopback::{FarEnd, LOOPBACK_TIMEOUT};
use transport::{Arrived, Directions, Transport};

/// Where frames go and come from.
pub trait Link: Send + Sync {
    /// The link's name, for the origin URI: `eth0`, `loopback`.
    fn name(&self) -> &str;
    /// The next frame, or `None` when nothing arrived within `timeout`.
    ///
    /// # Errors
    /// Where the link could not be read.
    fn receive(&self, timeout: Duration) -> Result<Option<Frame>>;
    /// Put a frame on the link.
    ///
    /// # Errors
    /// Where the link refused it.
    fn transmit(&self, frame: &Frame) -> Result<()>;
}

/// An in-process wire: what is transmitted is received, in order.
#[derive(Clone, Default)]
pub struct Loopback {
    frames: Arc<Mutex<VecDeque<Frame>>>,
}

impl Loopback {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Link for Loopback {
    fn name(&self) -> &'static str {
        "loopback"
    }

    fn receive(&self, _timeout: Duration) -> Result<Option<Frame>> {
        Ok(self
            .frames
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front())
    }

    fn transmit(&self, frame: &Frame) -> Result<()> {
        self.frames
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(frame.clone());
        Ok(())
    }
}

/// A Stream as one frame under one `EtherType` on one link.
#[derive(Clone)]
pub struct EthernetTransport {
    link: Arc<dyn Link>,
    source: Mac,
    destination: Mac,
    ethertype: u16,
    mtu: usize,
    timeout: Duration,
}

impl EthernetTransport {
    /// A transport on `link`, sending from `source` to `destination` under
    /// [`XMIP_ETHERTYPE`], one standard frame at a time.
    #[must_use]
    pub fn new(link: Arc<dyn Link>, source: Mac, destination: Mac) -> Self {
        Self {
            link,
            source,
            destination,
            ethertype: XMIP_ETHERTYPE,
            mtu: MTU,
            timeout: Duration::from_secs(1),
        }
    }

    /// Send under another `EtherType` — a protocol above naming its own.
    #[must_use]
    pub const fn under(mut self, ethertype: u16) -> Self {
        self.ethertype = ethertype;
        self
    }

    /// The link carries jumbo frames: the ceiling is [`JUMBO_MTU`].
    #[must_use]
    pub const fn jumbo(mut self) -> Self {
        self.mtu = JUMBO_MTU;
        self
    }

    /// Give up waiting on a quiet link after `timeout`.
    #[must_use]
    pub const fn timing_out_after(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The largest payload one frame carries on this link.
    #[must_use]
    pub const fn mtu(&self) -> usize {
        self.mtu
    }

    /// The link the frames go on.
    #[must_use]
    pub fn link(&self) -> &Arc<dyn Link> {
        &self.link
    }

    /// The next frame on the link as a Stream, or `None` when the link is
    /// quiet.
    ///
    /// # Errors
    /// Where the link could not be read.
    pub fn receive_one(&self) -> Result<Option<Arrived>> {
        Ok(self
            .link
            .receive(self.timeout)?
            .map(|frame| Arrived::new(frame.origin(self.link.name()), frame.payload)))
    }

    /// `bytes` as one frame to `destination`.
    ///
    /// # Errors
    /// A payload over the MTU, or a link that refused the frame.
    pub fn transmit(&self, destination: Mac, bytes: &[u8]) -> Result<()> {
        ceiling::within(bytes.len(), self.mtu, "one frame carries on this link")?;
        self.link.transmit(&Frame::new(
            destination,
            self.source,
            self.ethertype,
            bytes,
        )?)
    }

    /// `ethernet://<link>/<mac>`: where the near end sends.
    fn target(&self) -> String {
        format!("ethernet://{}/{}", self.link.name(), self.destination)
    }
}

impl Transport for EthernetTransport {
    fn name(&self) -> &'static str {
        "ethernet"
    }

    fn directions(&self) -> Directions {
        Directions::BOTH
    }

    /// Nothing on the link is not an error: an empty vector.
    fn receive(&self) -> Result<Vec<Arrived>> {
        Ok(self.receive_one()?.into_iter().collect())
    }

    /// `target` may name a destination, `ethernet://eth0/02:00:00:00:00:02`,
    /// overriding the transport's.
    fn send(&self, target: &str, bytes: &[u8]) -> Result<()> {
        let destination = match transport::socket::target("ethernet", target) {
            Some((_, mac)) if !mac.is_empty() => mac.parse()?,
            _ => self.destination,
        };
        self.transmit(destination, bytes)
    }
}

impl EthernetTransport {
    /// Both ends on one in-process link, two locally administered
    /// addresses, the loopback timeout standing where an adapter would wait
    /// for quiet.
    #[must_use]
    pub fn loopback() -> Self {
        Self::new(
            Arc::new(Loopback::new()),
            Mac([0x02, 0, 0, 0, 0, 1]),
            Mac([0x02, 0, 0, 0, 0, 2]),
        )
        .timing_out_after(LOOPBACK_TIMEOUT)
    }
}

/// One frame is one Stream: the MTU is the ceiling, and a payload over it
/// is refused rather than fragmented — fragmentation is IP's, above.
impl transport::loopback::Loopback for EthernetTransport {
    fn ceiling(&self) -> Option<usize> {
        Some(self.mtu)
    }

    /// The link the frame went on. Nothing waits: the round is in order.
    fn far_end(&self) -> Result<Box<dyn FarEnd>> {
        let transport = self.clone();
        Ok(Box::new(Held::new(self.target(), move || {
            transport
                .receive_one()?
                .ok_or_else(|| protocol_error("nothing came over the link"))
        })))
    }

    fn send_to(&self, address: &str, payload: &[u8]) -> Result<()> {
        self.send(address, payload)
    }

    /// In order on one thread: a link does not listen, so the frame goes on
    /// first and the read-back takes it off.
    fn exchanges_in_order(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transport::loopback::Loopback as _;
    use transport::payload::edge_payloads;

    /// The shapes a protocol breaks on, as the Playground lists them.
    fn payloads() -> Vec<(&'static str, Vec<u8>)> {
        let mut payloads = edge_payloads();
        payloads.extend([(
            "the brim",
            (0..MTU)
                .map(|n| u8::try_from(n % 251).unwrap_or(0))
                .collect(),
        )]);
        payloads
    }

    #[test]
    fn a_loopback_round_carries_a_stream_as_one_frame() {
        let loopback = EthernetTransport::loopback();
        let arrived = loopback.round(b"one frame").expect("round");
        assert_eq!(arrived.bytes, b"one frame");
        assert_eq!(
            arrived.origin_uri,
            "ethernet://loopback/02:00:00:00:00:01?type=0x88b5"
        );
        assert_eq!(loopback.ceiling(), Some(MTU));
        assert!(loopback.refuses(b"one frame").is_none());
        assert_eq!(loopback.name(), "ethernet");
        assert!(loopback.claims().is_none());
    }

    #[test]
    fn the_loopback_returns_the_edges_whole_and_refuses_over_the_brim() {
        let loopback = EthernetTransport::loopback();
        for (name, bytes) in payloads() {
            let arrived = loopback
                .round(&bytes)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(arrived.bytes, bytes, "{name}");
        }
        let error = loopback.round(&[0; MTU + 1]).expect_err("over the brim");
        assert!(error.message.contains("over the 1500"), "{error}");
        let jumbo = EthernetTransport::loopback().jumbo();
        assert_eq!(jumbo.ceiling(), Some(JUMBO_MTU));
        assert_eq!(
            jumbo.round(&[7; MTU + 1]).expect("jumbo").bytes,
            [7; MTU + 1]
        );
        assert!(jumbo.round(&[0; JUMBO_MTU + 1]).is_err());
    }

    #[test]
    fn a_target_names_the_destination_and_the_link_carries_frames_in_order() {
        let link: Arc<dyn Link> = Arc::new(Loopback::new());
        let transport = EthernetTransport::new(
            Arc::clone(&link),
            Mac([2, 0, 0, 0, 0, 1]),
            Mac([2, 0, 0, 0, 0, 2]),
        )
        .under(0x88b6);
        transport.send("", &[0xaa]).expect("default destination");
        transport
            .send("ethernet://loopback/ff:ff:ff:ff:ff:ff", &[0xbb])
            .expect("broadcast");
        let first = link
            .receive(Duration::ZERO)
            .expect("read")
            .expect("a frame");
        assert_eq!(first.destination, Mac([2, 0, 0, 0, 0, 2]));
        assert_eq!(first.ethertype, 0x88b6);
        assert_eq!(first.payload, [0xaa]);
        let second = transport.receive_one().expect("read").expect("a frame");
        assert_eq!(
            second.origin_uri,
            "ethernet://loopback/02:00:00:00:00:01?type=0x88b6"
        );
        assert_eq!(second.bytes, [0xbb]);
        assert!(
            transport.receive().expect("quiet").is_empty(),
            "nothing is not an error"
        );
        assert!(
            transport
                .send("ethernet://loopback/not-a-mac", &[])
                .is_err()
        );
        assert_eq!(transport.mtu(), MTU);
    }
}
