// SPDX-License-Identifier: GPL-2.0-only
//! Host-testable core model of the tb_rdma driver (design doc section 5).
//!
//! This crate is plain `std` Rust so the protocol logic can be unit-tested and
//! fuzzed off-kernel. The kernel module will wrap the same logic once it is
//! made `no_std + alloc` (tracked in docs/ROADMAP.md).
pub mod credit;
pub mod qp;
pub mod route;
pub mod vring;
