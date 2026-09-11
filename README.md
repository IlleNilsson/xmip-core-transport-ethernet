# xmip-core-transport-ethernet

Ethernet transport: raw IEEE 802.3 frames below IP under Xmip's own EtherType — one frame is one Stream up to the MTU, jumbo where the link allows; an in-process loopback link everywhere, the OS raw socket where it is permitted. A technology of [xmip-core-transport](https://github.com/IlleNilsson/xmip-core-transport).

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
