# xmip-core-transport-ethernet

Ethernet transport: raw IEEE 802.3 frames below IP under Xmip's own EtherType — one frame is one Stream up to the MTU, jumbo where the link allows; an in-process loopback link everywhere, the OS raw socket where it is permitted. A technology of [xmip-core-transport](https://github.com/IlleNilsson/xmip-core-transport).

A send target is read by `net::Target` in [xmip-core-library-net](https://github.com/IlleNilsson/xmip-core-library-net), the one reading of a URI every technology calls: scheme, authority, path and decoded query. Until 2026-09-28 it was read through the transport capability's `socket::target`, which split it on its first slash and left the query in the path.

A Location's address is its link: `loopback`, the in-process wire, or an interface, `eth0`, the kernel's raw socket on Linux behind the `packet` feature (`src/packet.rs`; the owner, 2026-11-25: *just do your best for now*). It is an `AF_PACKET` socket bound to the interface, every frame read and written whole, its header included, through `nix`'s safe calls: its address is the interface's own `sockaddr_ll` as the kernel lists it (`getifaddrs`), never one built here, so the crate keeps no `unsafe`. It needs `CAP_NET_RAW`, which an administrator grants a node, or a user and network namespace grants over itself, as the root's service test proves it, on a veth pair. Until 2026-11-25 every address but `loopback` was refused. EtherCAT and PROFINET open their links through this one (`open_link`).

## Acknowledgement

Acceptance is at-most-once here. A raw Ethernet frame has no reply: it is off
the wire as it is read, so nobody is left to tell how the receive cycle ended.
Each frame arrives whole.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
