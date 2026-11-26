# xmip-core-transport-ethernet

Ethernet transport: raw IEEE 802.3 frames below IP under Xmip's own EtherType — one frame is one Stream up to the MTU, jumbo where the link allows; an in-process loopback link everywhere, the OS raw socket where it is permitted. A technology of [xmip-core-transport](https://github.com/IlleNilsson/xmip-core-transport).

A send target is read by `net::Target` in [xmip-core-library-net](https://github.com/IlleNilsson/xmip-core-library-net), the one reading of a URI every technology calls: scheme, authority, path and decoded query. Until 2026-09-28 it was read through the transport capability's `socket::target`, which split it on its first slash and left the query in the path.

A Location's address is its link: `loopback`, the in-process wire, or an interface, `eth0`, the kernel's raw socket on Linux behind the `packet` feature (`src/packet.rs`; the owner, 2026-11-25: *just do your best for now*). It is an `AF_PACKET` socket bound to the interface, every frame read and written whole, its header included, through `nix`'s safe calls: its address is the interface's own `sockaddr_ll` as the kernel lists it (`getifaddrs`), never one built here, so the crate keeps no `unsafe`. It needs `CAP_NET_RAW`, which an administrator grants a node, or a user and network namespace grants over itself, as the root's service test proves it, on a veth pair. Until 2026-11-25 every address but `loopback` was refused. EtherCAT and PROFINET open their links through this one (`open_link`).

## Acknowledgement

Acceptance is at-most-once here. A raw Ethernet frame has no reply: it is off
the wire as it is read, so nobody is left to tell how the receive cycle ended.
Each frame arrives whole.

## The declaration

What a Location of this technology is configured with is two `const`s in its
`declaration` module, `src/declaration.rs`, beside the code, which
`Configured` reads (ADR-0064, amendment 2026-11-26):
`declaration::SETTINGS`, `source`, `destination`, `ethertype`, `jumbo` and
`timeout`, and `SCHEMES`, `ethernet`. Xmip's own `EtherType` and the wait
`timeout` defaults to, `XMIP_ETHERTYPE` and `DEFAULT_TIMEOUT`, are there
too, and the code reads them from there; `XMIP_ETHERTYPE` is at the crate's
root as well, for the protocols riding on the frame. It is the same on every
operating system, those without the kernel's raw socket included. Built with
its `declaration` feature alone (`--no-default-features --features
declaration`), the crate is those and nothing more: its code and every crate
the code is built on are behind the default feature, and `packet`, the
kernel's link on Linux, which is code, turns the default on with it. So the
library the surfaces load carries it without the code, through the gathering
crate xmip-core-declaration (open problem 48), and the language server and
the desktop suggest and check an Ethernet Location's settings though no node
in their process loaded the technology. `test/Declaration.Test.ps1` in the
estate holds that the declaration builds nothing the transport capability
does not.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
