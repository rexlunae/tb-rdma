# Roadmap

Status key: [x] done, [ ] todo.

## Phase 0: protocol model (host, no hardware)
- [x] Wire header, RETH, opcodes, strict decode
- [x] Credits with reserved control class
- [x] Type-state QP with PSN checks
- [x] Router, TTL, link-failure reroute
- [x] Virtual ring and 3-node forwarding test
- [ ] Rendezvous (RTS/CTS) state machine
- [ ] ACK/NAK, cumulative ack, retransmit for multi-hop
- [ ] Virtual channels and dateline deadlock avoidance
- [ ] Link-state advertisement encoding and convergence tests
- [ ] cargo-fuzz targets for `Frame::parse`
- [x] `tb-rdma-proto` is `core`-only with a derive-based oracle test
- [ ] Port `tb-rdma-core` state to `kernel` crate types (`xarray`, `rbtree`, `sync`, fallible `KVec`/`KBox`); see kernel/README.md
- [ ] `FromBytes`/`AsBytes` views for wire structs in the kernel build
- [ ] Check that the proto sources build under kbuild (needs a kernel tree)

## Phase 0.5: hardware measurements
- [x] Read-only survey scripts for Linux and macOS (`tools/survey`)
- [ ] Run the survey on every machine and fill in "Machines on hand" in `docs/HARDWARE.md`
- [ ] Mac peer discovery test (`docs/PEER-TEST.md`): does the Mac offer RDMA to a non-TB5 Linux host?
- [ ] Fill in the measured columns of `docs/HARDWARE.md` on every controller available (rings, E2E, frame limits, tunnel count, throughput)

## Phase 1+: see the design document (bindings, loopback ib device, tb-rc on hardware, Mac conformance, forwarding, hardening)
