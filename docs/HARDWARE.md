# Hardware generations

Which links `tb-rdma` is expected to run on. Status values: **Verified** (read from the Linux `thunderbolt` driver source or Apple's note), **Expected** (follows from the shared design, untested), **Unknown** (needs a measurement on real hardware).

Nothing in this document has been measured on hardware. "Verified" means a fact about the driver code, not a test result.

## What the driver source tells us

- Ring count is not fixed per generation. `nhi_probe()` reads `hop_count` from the low 10 bits of `REG_CAPS`, allocates that many TX and RX ring slots, never uses HopID 0, and auto-allocates from 1 to `hop_count - 1`. **Verified.** Actual counts per controller: **Unknown**.
- A ring with `RING_FLAG_FRAME` uses 4096-byte frames (a frame-size field of 0 means 4096). **Verified.** This matches the 4 KiB frame the wire format assumes.
- `thunderbolt-net` uses 256-entry rings, 4084-byte payloads (4096 minus its 12-byte frame header), and enables `RING_FLAG_E2E` by default when the peer advertises it. **Verified.** So end-to-end flow control is a path the in-tree driver already exercises.
- `QUIRK_E2E` and `QUIRK_AUTO_CLEAR_INT` exist in the NHI driver. Which controllers set `QUIRK_E2E` was **not determined**; read `nhi_check_quirks()` before relying on E2E.
- Connection manager selection: if the OS has native control of any USB4 capability, the software connection manager is used. Otherwise the firmware connection manager (ICM) is tried first, with software as the fallback. **Verified.** The software CM advertises only the `user` security level.
- On the firmware CM paths for Falcon Ridge, Alpine Ridge, Titan Ridge and the Ice Lake `icm_tr_*` handlers, approving an XDomain DMA path is rejected when a tunnel already exists ("only one tunnel is supported by the firmware"). **Verified from a summary of `icm.c`; confirm against the source.** If it holds, `tb_rdma` and `thunderbolt-net` cannot both have a tunnel to the same peer on those hosts.
- Linux host-to-host networking supports only Apple's ThunderboltIP protocol today; `thunderbolt-stream` is configured through ConfigFS. **Verified** (kernel docs).

## Matrix

| Generation | Example controllers (PCI ID macros in `nhi.h`) | Connection manager | Link rate (spec) | `tb-rc` (Linux to Linux) | `tb-uc` with a Thunderbolt 5 Mac | Main risks |
|---|---|---|---|---|---|---|
| Thunderbolt 2 | Falcon Ridge | Firmware (ICM) | 20 Gb/s | Not a target | No | Out of scope |
| **Thunderbolt 3** | Alpine Ridge, Titan Ridge | Firmware (ICM) | 40 Gb/s | **Expected**, slower; single XDomain tunnel limit | **Unknown**: the TB3 host would negotiate a TB3-class link to a TB5 Mac; Apple documents the feature for TB5 Macs only | One tunnel only (cannot share with `thunderbolt-net`); ring count and E2E behavior unknown; security-level approval needed |
| **USB4 Gen 2 / Gen 3** | Ice Lake, Tiger Lake, Alder Lake, Raptor Lake, Meteor Lake, Lunar Lake, Panther Lake (Intel); AMD and other USB4-class hosts | Software when OS-controlled, else ICM | 20 / 40 Gb/s | **Expected** | **Unknown** | Per-controller ring count and E2E; some platforms restrict host-to-host in firmware; non-Intel NHI behavior untested |
| **Thunderbolt 4** | Maple Ridge (discrete), TB4-certified USB4 hosts | Software or ICM | 40 Gb/s | **Expected** | **Unknown** | Same as USB4 Gen 3 |
| **Thunderbolt 5 / USB4 v2** | Barlow Ridge (80G and 40G host IDs) | Not determined | 80 Gb/s symmetric | **Expected**, best case | **Target case**: Apple documents RDMA over Thunderbolt for Apple silicon with Thunderbolt 5 on macOS 26.2 or later | Kernel support for the newest IDs; NHI throughput may cap before the link does |
| Apple Mac, Thunderbolt 5 | (peer only) | Apple's | 80 Gb/s | n/a | **Target**: UC queue pairs, Send/Receive only, 4 KiB frames, credit-based flow control | Wire format unpublished; `tb-uc` is a hypothesis until validated by capture |
| Apple Mac, Intel or earlier / TB3 | (peer only) | Apple's | 40 Gb/s | n/a | **No**: not supported by Apple's feature | n/a |

Link rates are specification figures, not measurements. Earlier prototypes report roughly 16 to 20 Gb/s in practice, which suggests the NHI, not the link, is the ceiling.

## Machines on hand

Filled in from `tools/survey` reports. Values in italics are expectations, not survey results.

| Machine | Arch | USB4/TB host interface | Generation / link | Security level | `hop_count` | `CONFIG_RUST` | Role |
|---|---|---|---|---|---|---|---|
| Ryzen 9 9950X desktop | x86_64 | *Depends on the motherboard; its 20 Gb/s port may be USB 3.2 Gen 2x2, which is not USB4* | | | | | Primary dev and kernel build host; link endpoint if USB4 |
| Orange Pi 6 Plus 32 GB | arm64 | *Probably none* | | | | | ARM64 build and test host |
| Ryzen 9 5900X desktop | x86_64 | *Probably none (AM4 has no USB4)* | | | | | Build host |
| HP Spectre (Intel i7) | x86_64 | *Thunderbolt 3, firmware connection manager* | | | | | First link endpoint; Mac discovery test |
| MacBook Pro, M5 Pro | arm64 | Thunderbolt 5 | | n/a | n/a | n/a | `tb-uc` reference peer |

There is no second Thunderbolt 5 machine, so the first Mac test (`docs/PEER-TEST.md`) checks whether the Mac offers RDMA to a slower Linux peer at all.

## Measurements to take first

On each controller you have, before the protocol work depends on it:

1. `hop_count` from the NHI capabilities register (number of usable rings).
2. Whether `QUIRK_E2E` is set and whether end-to-end flow control actually throttles a sender.
3. Largest frame the ring accepts, and whether header and payload can be split across buffers (decides zero-copy).
4. How many XDomain DMA tunnels the firmware or software CM allows at once, and whether `thunderbolt-net` can coexist with `tb_rdma`.
5. Single-stream throughput and latency with a plain ring loopback between two hosts.
6. For a Mac peer: whether a TB3 or USB4 Linux host can establish the Apple service (AD/FA57 per the reverse-engineering notes) at all.

The results go back into the matrix above, and rows move from **Expected** or **Unknown** to measured values.
