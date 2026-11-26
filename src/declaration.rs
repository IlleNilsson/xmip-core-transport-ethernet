//! What a Location of this technology is configured with: its declaration,
//! which `Configured` reads, built with the crate's `declaration` feature
//! alone and the code behind the default (ADR-0064, amendments 2026-10-22
//! and 2026-11-26), for the library the surfaces load to carry (open
//! problem 48). It is the same on every operating system, those without
//! the kernel's raw socket included: a Location's settings are suggested
//! and checked wherever it is designed.

use std::time::Duration;

use xcore::settings::{Applies, Fixed, Kind, Presence, Setting, Settings};

/// Xmip's own `EtherType`: IEEE 802 local experimental `EtherType` 1.
pub const XMIP_ETHERTYPE: u16 = 0x88b5;

/// How long a receive waits on a quiet link unless a Location says.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

/// Every setting a Location takes beyond its address, which is the link the
/// frames go on, by its name: `loopback`, the in-process link, or an
/// interface, `eth0`, where the build carries the kernel's raw socket.
pub const SETTINGS: &Settings = &Settings {
    technology: env!("CARGO_PKG_NAME"),
    settings: &[
        Setting {
            name: "source",
            kind: Kind::Address,
            presence: Presence::Required,
            meaning: "The MAC address a frame is sent from.",
            applies: Applies::Send,
        },
        Setting {
            name: "destination",
            kind: Kind::Address,
            presence: Presence::Required,
            meaning: "The MAC address a frame is sent to unless the target names another.",
            applies: Applies::Send,
        },
        Setting {
            name: "ethertype",
            kind: Kind::Integer {
                minimum: 0x0600,
                maximum: 0xffff,
            },
            presence: Presence::Default(Fixed::Integer(XMIP_ETHERTYPE as i64)),
            meaning: "The EtherType a frame is sent under, as a protocol above names its own.",
            applies: Applies::Send,
        },
        Setting {
            name: "jumbo",
            kind: Kind::Boolean,
            presence: Presence::Default(Fixed::Boolean(false)),
            meaning: "Whether the link carries jumbo frames, raising the ceiling to 9000 bytes.",
            applies: Applies::Send,
        },
        transport::configured::timeout(
            Applies::Receive,
            Some(DEFAULT_TIMEOUT),
            "How long a receive waits on a quiet link.",
        ),
    ],
};

/// The schemes a Location's address is entered with to choose this technology.
pub const SCHEMES: &[&str] = &["ethernet"];
