// SPDX-License-Identifier: GPL-2.0-only
//! In-kernel selftest of the included `tb-rdma-proto` codec, run at module
//! init. This is the v0.1 stand-in for the KUnit port (kernel/README.md):
//! every check mirrors one `cargo test` case from the crates, allocation
//! free, `core` only.

use crate::credit::{Class, CreditPool};
use crate::proto as p;
use crate::qp::{PeerAddr, Qp, RxVerdict};
use kernel::prelude::*;

type R = Result<(), &'static str>;

fn check(name: &'static str, ok: bool) -> R {
    if ok {
        Ok(())
    } else {
        Err(name)
    }
}

fn hdr(op: p::Opcode) -> p::Header {
    p::Header {
        opcode: op,
        flags: p::flags::FIRST | p::flags::LAST,
        dst: 3,
        src: 7,
        ttl: 8,
        vc: 1,
        qpn: 0xAABBCC,
        psn: 42,
        ack_psn: 41,
        credits: 16,
    }
}

/// Sizes and payload limits from the design document.
fn sizes() -> R {
    check("sizes:header", p::HEADER_LEN == 26)?;
    check("sizes:reth", p::RETH_LEN == 16)?;
    check("sizes:frame", p::FRAME_LEN == 4096)?;
    check(
        "sizes:max_send",
        p::Frame::max_payload(p::Opcode::Send) == 4070,
    )?;
    check(
        "sizes:max_write",
        p::Frame::max_payload(p::Opcode::Write) == 4054,
    )
}

/// Little-endian byte order spot checks (host crates: byte_order_is_le).
fn byte_order() -> R {
    let mut b = [0u8; p::HEADER_LEN];
    let mut h = hdr(p::Opcode::Send);
    h.dst = 0x0102;
    h.qpn = 0x0A0B_0C0D;
    h.psn = 0x1122_3344;
    h.encode(&mut b).map_err(|_| "encode:header")?;
    if !(b[4] == 0x02 && b[5] == 0x01) {
        return Err("le:dst");
    }
    if !(b[10] == 0x0D && b[11] == 0x0C && b[12] == 0x0B && b[13] == 0x0A) {
        return Err("le:qpn");
    }
    if !(b[14] == 0x44 && b[15] == 0x33 && b[16] == 0x22 && b[17] == 0x11) {
        return Err("le:psn");
    }
    let r = p::Reth {
        rkey: 1,
        offset: 0x0102_0304_0506_0708,
        length: 2,
    };
    let mut rb = [0u8; p::RETH_LEN];
    r.encode(&mut rb).map_err(|_| "encode:reth")?;
    check("le:reth.offset", rb[4..12] == [8, 7, 6, 5, 4, 3, 2, 1])
}

/// Send roundtrip through Frame::write/parse.
fn roundtrip_send() -> R {
    let mut buf = [0u8; p::FRAME_LEN];
    let n = p::Frame::write(&hdr(p::Opcode::Send), None, b"hello", &mut buf)
        .map_err(|_| "write:send")?;
    check("write:send.len", n == p::HEADER_LEN + 5)?;
    let f = p::Frame::parse(&buf[..n]).map_err(|_| "parse:send")?;
    check("parse:send.hdr", f.header == hdr(p::Opcode::Send))?;
    check("parse:send.payload", f.payload == b"hello")?;
    check("parse:send.no_reth", f.reth.is_none())
}

/// Write roundtrip with a RETH present.
fn roundtrip_write() -> R {
    let r = p::Reth {
        rkey: 0x1234_5678,
        offset: 1 << 40,
        length: 4054,
    };
    let mut buf = [0u8; p::FRAME_LEN];
    let n = p::Frame::write(&hdr(p::Opcode::Write), Some(&r), &[9u8; 100], &mut buf)
        .map_err(|_| "write:write")?;
    let f = p::Frame::parse(&buf[..n]).map_err(|_| "parse:write")?;
    check("parse:write.reth", f.reth == Some(r))?;
    check("parse:write.payload", f.payload.len() == 100)
}

/// RETH presence must match the opcode (host crates: reth_mismatch_rejected).
fn reth_mismatch() -> R {
    let mut buf = [0u8; 64];
    check(
        "mismatch:write_no_reth",
        p::Frame::write(&hdr(p::Opcode::Write), None, &[], &mut buf)
            == Err(p::Error::RethMismatch),
    )?;
    let r = p::Reth {
        rkey: 1,
        offset: 0,
        length: 0,
    };
    check(
        "mismatch:send_with_reth",
        p::Frame::write(&hdr(p::Opcode::Send), Some(&r), &[], &mut buf)
            == Err(p::Error::RethMismatch),
    )
}

/// Strict decode rejections (host crates: strict_decode).
fn strict_decode() -> R {
    let mut b = [0u8; p::HEADER_LEN];
    hdr(p::Opcode::Ack).encode(&mut b).map_err(|_| "encode:ack")?;
    let mut bad = b;
    bad[0] = 9;
    check("strict:version", p::Header::decode(&bad) == Err(p::Error::BadVersion(9)))?;
    let mut bad = b;
    bad[1] = 99;
    check("strict:opcode", p::Header::decode(&bad) == Err(p::Error::BadOpcode(99)))?;
    let mut bad = b;
    bad[25] = 1;
    check("strict:reserved", p::Header::decode(&bad) == Err(p::Error::Reserved))?;
    check("strict:short", p::Header::decode(&b[..10]) == Err(p::Error::Short))?;
    let mut w = [0u8; p::HEADER_LEN + 4];
    hdr(p::Opcode::Write).encode(&mut w).map_err(|_| "encode:write")?;
    check("strict:short_reth", p::Frame::parse(&w) == Err(p::Error::Short))
}

/// Truncated parse must never panic (host crates: fuzz_never_panics, 200k
/// iterations on the host; 2000 here keeps module init fast).
fn fuzz_never_panics() -> R {
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut buf = [0u8; 128];
    for _ in 0..2000 {
        for b in buf.iter_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *b = s as u8;
        }
        if s & 3 != 0 {
            buf[0] = p::FORMAT_VERSION;
            buf[24] = 0;
            buf[25] = 0;
            buf[1] = 1 + (s >> 8) as u8 % 12;
        }
        let len = (s >> 16) as usize % 129;
        let _ = p::Frame::parse(&buf[..len]);
    }
    Ok(())
}

/// UC profile is Send-only (host crates: uc_profile_is_send_only).
fn uc_profile() -> R {
    check("uc:send_ok", p::Opcode::Send.allowed_in_uc())?;
    check("uc:write_denied", !p::Opcode::Write.allowed_in_uc())
}

/// Credits: data and control classes are independent (host crates:
/// control_not_starved_by_data).
fn credits() -> R {
    let mut q = CreditPool::new(2, 1);
    check("credit:data1", q.try_consume(Class::Data))?;
    check("credit:data2", q.try_consume(Class::Data))?;
    check("credit:data_exhausted", !q.try_consume(Class::Data))?;
    check("credit:ctrl_ok", q.try_consume(Class::Control))?;
    check("credit:ctrl_exhausted", !q.try_consume(Class::Control))?;
    q.return_ctrl(5);
    check("credit:ctrl_capped", q.available(Class::Control) == 1)?;
    q.grant_data(3);
    check("credit:granted", q.available(Class::Data) == 3)
}

/// PSN sequence, wraparound and RX verdicts (host crates:
/// psn_sequence_and_wrap, rx_verdicts).
fn qp_psn() -> R {
    let mut q = Qp::new(1)
        .to_init()
        .to_rtr(PeerAddr { node: 2, qpn: 3 }, u32::MAX)
        .to_rts(u32::MAX);
    check("psn:tx_max", q.next_tx_psn() == u32::MAX)?;
    check("psn:tx_wrap", q.next_tx_psn() == 0)?;
    check("psn:rx_max", q.check_rx(u32::MAX) == RxVerdict::InOrder)?;
    check("psn:rx_wrap", q.check_rx(0) == RxVerdict::InOrder)?;

    let mut q = Qp::new(5)
        .to_init()
        .to_rtr(PeerAddr { node: 2, qpn: 9 }, 100)
        .to_rts(0);
    check("psn:in_order", q.check_rx(100) == RxVerdict::InOrder)?;
    check("psn:duplicate", q.check_rx(100) == RxVerdict::Duplicate)?;
    check("psn:gap", q.check_rx(103) == RxVerdict::Gap)
}

const CHECKS: &[( &'static str, fn() -> R)] = &[
    ("sizes", sizes),
    ("byte_order", byte_order),
    ("roundtrip_send", roundtrip_send),
    ("roundtrip_write", roundtrip_write),
    ("reth_mismatch", reth_mismatch),
    ("strict_decode", strict_decode),
    ("fuzz_never_panics", fuzz_never_panics),
    ("uc_profile", uc_profile),
    ("credits", credits),
    ("qp_psn", qp_psn),
];

/// Run every check group; on the first failure return its name to the
/// caller. Each passing group is announced so dmesg shows what ran.
pub fn run_selftest() -> Result<(), &'static str> {
    for (name, f) in CHECKS {
        match f() {
            Ok(()) => pr_info!("tb_rdma: selftest {}: ok\n", name),
            Err(why) => return Err(why),
        }
    }
    Ok(())
}
