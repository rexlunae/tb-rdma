# Kernel module

Status: v0.1 loads, runs the codec selftest and registers the `tb-rdma` XDomain service. Build and load steps are in [BUILD.md](BUILD.md).

Layering: `tb-rdma-proto` stays a pure `core` crate (no kernel dependency) so the same source is host-testable, fuzzable and usable by Mac capture tooling. Everything stateful lives in the kernel layer and should be written against the in-tree Rust `kernel` crate, not `std` or hand-rolled equivalents.

## What the `kernel` crate provides (checked against the current rustdoc module list)

| Need in tb_rdma | Use | Notes |
|---|---|---|
| Errors | `kernel::error` | `From<proto::Error>` mapping (Short/BadOpcode -> `EINVAL`/`EPROTO`) |
| Allocation | `kernel::alloc` (`KBox`, `KVec`, GFP flags) | Fallible only; no infallible `Vec`/`Box`. `tb-rdma-core` currently uses `std` collections and must move off them |
| QP table by QPN, MR table by rkey | `xarray` | rkey/QPN allocation via `id_pool` |
| Peer/route tables | `rbtree`, `list`, `maple_tree` | replaces `BTreeMap` in `route.rs` |
| Locks and shared ownership | `sync` (`SpinLock`, `Mutex`, `Arc`), `init` (pin-init) | in-place construction of QPs and rings |
| TX/RX processing, retransmit and ACK timers | `workqueue`, `time` | |
| Wire structs | `transmute::{FromBytes, AsBytes}` | for zero-copy views of header/RETH. The crate has no endian helpers, so field order stays explicit (`to_le_bytes`) in `proto` |
| Memory registration | `dma`, `iommu`, `scatterlist`, `page`, `mm` | IOMMU is required by the security model |
| Userspace ABI plumbing | `uaccess`, `ioctl` | uverbs itself comes from ib_core |
| Size and layout asserts | `build_assert` | replaces ad hoc const asserts in the kernel build |
| Diagnostics | `debugfs`, `tracepoint`, `module_param`, `print` | |
| Tests | `kunit` | port of the host-side model tests |

## What does not exist (confirmed absent from the module list)

- Thunderbolt/USB4 abstractions: `tb_rdma_sys` bindings plus safe wrappers for `ServiceDriver`, `Xdomain`, rings and frames are still ours to write. (`usb` and `pci` modules exist but do not cover the NHI or XDomain.)
- InfiniBand/RDMA abstractions: same, for `ib_device`, PD/CQ/QP/MR and the ops table.
- Endianness types.

## Order of work

1. Kernel tree (6.14+ suggested by the existing prototype) with `CONFIG_RUST=y`, `CONFIG_THUNDERBOLT`, `CONFIG_INFINIBAND`.
2. `tb_rdma_sys` bindings, then `tb` wrappers, then `ib` wrappers.
3. Include `tb-rdma-proto` sources unchanged (out-of-tree modules are one crate, so as a `mod` via `#[path]`).
4. Rewrite `tb-rdma-core` state (peers, QPs, MRs, credits, routes) on `kernel` types; keep the host model as a test oracle.

Open hardware questions that gate design choices: NHI max frame size, ring count, per-frame buffer splitting (zero-copy), and E2E flow-control behavior on TB3 vs USB4 vs TB4/TB5 controllers.
