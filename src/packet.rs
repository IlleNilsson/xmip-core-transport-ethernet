//! The kernel's link on Linux: a raw `AF_PACKET` socket bound to one
//! interface, every frame on it read whole and every frame written to it as
//! it is, its header included (packet(7)). Nothing here is `unsafe`: the
//! socket, its bind, its reads and writes are `nix`'s safe calls, and the
//! address it is bound with is the interface's own `sockaddr_ll`, as the
//! kernel lists it (`getifaddrs`), never one built here.
//!
//! Bound with `ETH_P_ALL`, the socket reads every frame the interface
//! carries but its own, whatever its `EtherType`: what is the protocol's is
//! the protocol's to tell, as on the in-process link. It needs
//! `CAP_NET_RAW` in the user namespace that owns the interface's network
//! namespace: an administrator's grant on a box, or a namespace of the
//! test's own (the owner, 2026-11-25: *just do your best for now*).

use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::socket::{self, AddressFamily, MsgFlags, SockFlag, SockProtocol, SockType, sockopt};
use nix::sys::time::{TimeVal, TimeValLike};
use transport::error::{Result, TransportError, classify, protocol_error};

use crate::{Frame, JUMBO_MTU, Link};

/// The header room a frame is read with beside its payload: two addresses,
/// an 802.1Q tag and the `EtherType`.
const HEADER_ROOM: usize = 18;

/// A raw socket on one interface.
pub struct PacketLink {
    interface: String,
    socket: OwnedFd,
}

impl PacketLink {
    /// The interface called `interface`, its raw socket bound to it.
    ///
    /// # Errors
    /// No interface of that name, or a socket the kernel refuses: without
    /// `CAP_NET_RAW`, refused as not permitted.
    pub fn open(interface: &str) -> Result<Self> {
        let address = nix::ifaddrs::getifaddrs()
            .map_err(|e| failed("listing the interfaces", e))?
            .filter(|found| found.interface_name == interface)
            .find_map(|found| found.address.and_then(|at| at.as_link_addr().copied()))
            .ok_or_else(|| protocol_error(format!("no interface {interface:?} on this machine")))?;
        let socket = socket::socket(
            AddressFamily::Packet,
            SockType::Raw,
            SockFlag::SOCK_CLOEXEC,
            SockProtocol::EthAll,
        )
        .map_err(|e| failed("opening a raw socket", e))?;
        socket::bind(socket.as_raw_fd(), &address)
            .map_err(|e| failed(&format!("binding to {interface}"), e))?;
        Ok(Self {
            interface: interface.to_string(),
            socket,
        })
    }
}

impl Link for PacketLink {
    fn name(&self) -> &str {
        &self.interface
    }

    /// The next frame, waiting at most `timeout`; `None` where none came.
    /// A wait of zero looks once.
    fn receive(&self, timeout: Duration) -> Result<Option<Frame>> {
        let flags = if timeout.is_zero() {
            MsgFlags::MSG_DONTWAIT
        } else {
            let micros = i64::try_from(timeout.as_micros()).unwrap_or(i64::MAX);
            socket::setsockopt(
                &self.socket,
                sockopt::ReceiveTimeout,
                &TimeVal::microseconds(micros),
            )
            .map_err(|e| failed("setting the wait", e))?;
            MsgFlags::empty()
        };
        let mut buffer = vec![0u8; JUMBO_MTU + HEADER_ROOM];
        match socket::recv(self.socket.as_raw_fd(), &mut buffer, flags) {
            Ok(read) => Frame::decode(&buffer[..read]).map(Some),
            Err(Errno::EAGAIN | Errno::EINTR) => Ok(None),
            Err(e) => Err(failed("reading the link", e)),
        }
    }

    fn transmit(&self, frame: &Frame) -> Result<()> {
        socket::send(self.socket.as_raw_fd(), &frame.encode(), MsgFlags::empty())
            .map(drop)
            .map_err(|e| failed("writing the link", e))
    }
}

/// `errno` from `doing`, classified as every transport's is.
fn failed(doing: &str, errno: Errno) -> TransportError {
    classify(doing, &std::io::Error::from(errno))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Location naming an interface this machine has not is refused,
    /// naming it, before any socket is asked for: until 2026-11-25 every
    /// name but `loopback` was refused as a link this build does not carry.
    #[test]
    fn an_interface_not_on_this_machine_is_refused_by_name() {
        let Err(refused) = PacketLink::open("xmip-absent") else {
            panic!("no such interface");
        };
        assert!(refused.message.contains("\"xmip-absent\""), "{refused}");
    }
}
