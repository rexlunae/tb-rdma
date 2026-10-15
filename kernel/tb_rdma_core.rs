// SPDX-License-Identifier: GPL-2.0-only
//! tb_rdma: RDMA over USB4/Thunderbolt — kernel module core (Rust).
//!
//! Out-of-tree Rust module. The wire codec from `tb-rdma-proto` is included
//! unchanged via `#[path]` (kernel/README.md "Order of work" step 3). C-side
//! Thunderbolt glue lives in tb_rdma_glue.c and calls the `#[no_mangle]`
//! functions below; both objects link into one .ko so no EXPORT_SYMBOL is
//! needed between them.
//!
//! NOTE: kbuild adds `-Zcrate-attr=no_std` at the crate root, so this file
//! must not repeat it. The included proto lib.rs has its own inner
//! `#![no_std]`, which rustc accepts with a warning when included as a
//! module (tracked in kernel/README.md).

#[path = "../crates/tb-rdma-proto/src/lib.rs"]
mod proto;

mod credit;
mod qp;
mod selftest;
use selftest::run_selftest;

use core::ffi::{c_char, c_int, c_void};
use kernel::prelude::*;

module! {
    type: TbRdma,
    name: "tb_rdma",
    authors: ["tb-rdma contributors"],
    description: "RDMA over USB4/Thunderbolt (tb-rc profile, Linux to Linux)",
    license: "GPL",
}

/// Module state. Per-peer state arrives with the xarray-backed table in v0.2
/// (see kernel/README.md).
pub struct TbRdma;

extern "C" {
    /// Register the XDomain property directory and service driver. 0 on
    /// success, negative errno otherwise. Defined in tb_rdma_glue.c.
    fn tb_rdma_glue_init() -> c_int;
    /// Undo `tb_rdma_glue_init`.
    fn tb_rdma_glue_exit();
}

impl kernel::Module for TbRdma {
    fn init(_module: &'static ThisModule) -> Result<Self> {
        pr_info!("tb_rdma: init (proto v{}, header {} bytes, frame {} bytes)\n",
            proto::FORMAT_VERSION, proto::HEADER_LEN, proto::FRAME_LEN);

        match run_selftest() {
            Ok(()) => pr_info!("tb_rdma: codec selftest: all checks passed\n"),
            Err(msg) => {
                pr_err!("tb_rdma: codec selftest FAILED: {}\n", msg);
                return Err(EINVAL);
            }
        }

        let ret = unsafe { tb_rdma_glue_init() };
        if ret != 0 {
            pr_err!("tb_rdma: glue init failed: {}\n", ret);
            return Err(Error::from_errno(ret));
        }

        pr_info!("tb_rdma: registered service driver (key \"tb-rdma\")\n");
        Ok(TbRdma)
    }
}

impl Drop for TbRdma {
    fn drop(&mut self) {
        unsafe { tb_rdma_glue_exit() };
        pr_info!("tb_rdma: unloaded\n");
    }
}

/// C glue reports a probed service. `key` is a NUL-terminated 9-byte buffer
/// max (protocol_key is char[8+1] in struct tb_service_id). Returns 0.
#[no_mangle]
pub extern "C" fn tb_rdma_peer_probe(
    _svc: *const c_void,
    key: *const c_char,
    prtcid: c_int,
    prtcvers: c_int,
    prtcrevs: c_int,
    prtcstns: c_int,
) -> c_int {
    // Copy the key bytes for the log line, bounded, no allocation.
    let mut buf = [0u8; 9];
    let mut i = 0usize;
    unsafe {
        while i < 8 {
            let b = *key.add(i) as u8;
            if b == 0 {
                break;
            }
            buf[i] = b;
            i += 1;
        }
    }
    let name = core::str::from_utf8(&buf[..i]).unwrap_or("tb-rdma?");
    pr_info!(
        "tb_rdma: peer service probed: key={} prtcid={} prtcvers={} prtcrevs={} prtcstns={}\n",
        name, prtcid, prtcvers, prtcrevs, prtcstns
    );
    0
}

/// C glue reports service removal.
#[no_mangle]
pub extern "C" fn tb_rdma_peer_remove(_svc: *const c_void) {
    pr_info!("tb_rdma: peer service removed\n");
}
