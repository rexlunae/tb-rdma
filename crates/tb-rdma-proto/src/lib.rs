// SPDX-License-Identifier: GPL-2.0-only
//! Wire format for RDMA over USB4/Thunderbolt (design doc section 4).
//!
//! PROVISIONAL: sizes and layout follow the design document and are not yet
//! validated against NHI frame limits or Apple's (unpublished) format.
//! All multi-byte fields are little-endian. `no_std`, zero dependencies.
#![no_std]

// Hand-written little-endian codec built only on `core`, so the same source
// can be compiled by kbuild inside the kernel (no std, no dependencies, no
// proc-macros). The `simple_endian` derive is used only in tests/oracle.rs to
// cross-check these bytes on the host.

#[inline]
fn put16(out: &mut [u8], at: usize, v: u16) {
    out[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put64(out: &mut [u8], at: usize, v: u64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
#[inline]
fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
#[inline]
fn get64(b: &[u8], at: usize) -> u64 {
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(x)
}

/// Size of the common header in bytes.
pub const HEADER_LEN: usize = 26;
/// Size of the RETH (Write/Read/Atomic) in bytes.
pub const RETH_LEN: usize = 16;
/// Provisional frame size (4 KiB).
pub const FRAME_LEN: usize = 4096;
pub const FORMAT_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Buffer shorter than the structure being decoded or encoded.
    Short,
    BadVersion(u8),
    BadOpcode(u8),
    /// Reserved field non-zero.
    Reserved,
    /// Opcode requires a RETH but none was present or vice versa.
    RethMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Send = 1,
    Write = 2,
    ReadReq = 3,
    ReadResp = 4,
    Atomic = 5,
    AtomicAck = 6,
    Rts = 7,
    Cts = 8,
    Ack = 9,
    Nak = 10,
    Credit = 11,
    Terminate = 12,
}

impl Opcode {
    pub fn from_u8(v: u8) -> Result<Self, Error> {
        use Opcode::*;
        Ok(match v {
            1 => Send,
            2 => Write,
            3 => ReadReq,
            4 => ReadResp,
            5 => Atomic,
            6 => AtomicAck,
            7 => Rts,
            8 => Cts,
            9 => Ack,
            10 => Nak,
            11 => Credit,
            12 => Terminate,
            x => return Err(Error::BadOpcode(x)),
        })
    }

    /// Operations that carry a RETH.
    pub fn has_reth(self) -> bool {
        matches!(
            self,
            Opcode::Write | Opcode::ReadReq | Opcode::Atomic | Opcode::Rts
        )
    }

    /// Operations allowed in the Apple-compatible `tb-uc` profile.
    pub fn allowed_in_uc(self) -> bool {
        matches!(self, Opcode::Send)
    }

    /// Control frames use the reserved control-credit class.
    pub fn is_control(self) -> bool {
        matches!(
            self,
            Opcode::Ack | Opcode::Nak | Opcode::Credit | Opcode::Terminate
        )
    }
}

/// Header flag bits.
pub mod flags {
    pub const FIRST: u16 = 1 << 0;
    pub const LAST: u16 = 1 << 1;
    pub const SOLICITED: u16 = 1 << 2;
    pub const ACK_REQ: u16 = 1 << 3;
    pub const FENCE: u16 = 1 << 4;
    pub const ALL: u16 = FIRST | LAST | SOLICITED | ACK_REQ | FENCE;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub opcode: Opcode,
    pub flags: u16,
    pub dst: u16,
    pub src: u16,
    pub ttl: u8,
    pub vc: u8,
    pub qpn: u32,
    pub psn: u32,
    pub ack_psn: u32,
    pub credits: u16,
}

impl Header {
    pub fn encode(&self, out: &mut [u8]) -> Result<(), Error> {
        if out.len() < HEADER_LEN {
            return Err(Error::Short);
        }
        out[0] = FORMAT_VERSION;
        out[1] = self.opcode as u8;
        put16(out, 2, self.flags);
        put16(out, 4, self.dst);
        put16(out, 6, self.src);
        out[8] = self.ttl;
        out[9] = self.vc;
        put32(out, 10, self.qpn);
        put32(out, 14, self.psn);
        put32(out, 18, self.ack_psn);
        put16(out, 22, self.credits);
        put16(out, 24, 0); // reserved
        Ok(())
    }

    /// Strict decode: rejects unknown version/opcode and non-zero reserved bytes.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < HEADER_LEN {
            return Err(Error::Short);
        }
        if b[0] != FORMAT_VERSION {
            return Err(Error::BadVersion(b[0]));
        }
        if get16(b, 24) != 0 {
            return Err(Error::Reserved);
        }
        Ok(Header {
            opcode: Opcode::from_u8(b[1])?,
            flags: get16(b, 2) & flags::ALL,
            dst: get16(b, 4),
            src: get16(b, 6),
            ttl: b[8],
            vc: b[9],
            qpn: get32(b, 10),
            psn: get32(b, 14),
            ack_psn: get32(b, 18),
            credits: get16(b, 22),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reth {
    pub rkey: u32,
    pub offset: u64,
    pub length: u32,
}

impl Reth {
    pub fn encode(&self, out: &mut [u8]) -> Result<(), Error> {
        if out.len() < RETH_LEN {
            return Err(Error::Short);
        }
        put32(out, 0, self.rkey);
        put64(out, 4, self.offset);
        put32(out, 12, self.length);
        Ok(())
    }

    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < RETH_LEN {
            return Err(Error::Short);
        }
        Ok(Reth {
            rkey: get32(b, 0),
            offset: get64(b, 4),
            length: get32(b, 12),
        })
    }
}

/// A parsed frame borrowing its payload from the input buffer.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub header: Header,
    pub reth: Option<Reth>,
    pub payload: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self, Error> {
        let header = Header::decode(buf)?;
        let mut off = HEADER_LEN;
        let reth = if header.opcode.has_reth() {
            let r = Reth::decode(&buf[off.min(buf.len())..])?;
            off += RETH_LEN;
            Some(r)
        } else {
            None
        };
        Ok(Frame {
            header,
            reth,
            payload: &buf[off..],
        })
    }

    /// Serialize into `out`, returning bytes written.
    pub fn write(
        header: &Header,
        reth: Option<&Reth>,
        payload: &[u8],
        out: &mut [u8],
    ) -> Result<usize, Error> {
        if header.opcode.has_reth() != reth.is_some() {
            return Err(Error::RethMismatch);
        }
        let hdr = HEADER_LEN + if reth.is_some() { RETH_LEN } else { 0 };
        if out.len() < hdr + payload.len() {
            return Err(Error::Short);
        }
        header.encode(out)?;
        if let Some(r) = reth {
            r.encode(&mut out[HEADER_LEN..])?;
        }
        out[hdr..hdr + payload.len()].copy_from_slice(payload);
        Ok(hdr + payload.len())
    }

    /// Maximum payload bytes in one frame for this opcode.
    pub fn max_payload(op: Opcode) -> usize {
        FRAME_LEN - HEADER_LEN - if op.has_reth() { RETH_LEN } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(op: Opcode) -> Header {
        Header {
            opcode: op,
            flags: flags::FIRST | flags::LAST,
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

    #[test]
    fn byte_order_is_little_endian_on_any_host() {
        let mut b = [0u8; HEADER_LEN];
        let mut h = hdr(Opcode::Send);
        h.dst = 0x0102;
        h.qpn = 0x0A0B0C0D;
        h.psn = 0x11223344;
        h.encode(&mut b).unwrap();
        assert_eq!(&b[4..6], &[0x02, 0x01]);
        assert_eq!(&b[10..14], &[0x0D, 0x0C, 0x0B, 0x0A]);
        assert_eq!(&b[14..18], &[0x44, 0x33, 0x22, 0x11]);
        let r = Reth {
            rkey: 1,
            offset: 0x0102030405060708,
            length: 2,
        };
        let mut rb = [0u8; RETH_LEN];
        r.encode(&mut rb).unwrap();
        assert_eq!(&rb[4..12], &[8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn sizes_match_design() {
        assert_eq!(Frame::max_payload(Opcode::Send), 4070);
        assert_eq!(Frame::max_payload(Opcode::Write), 4054);
    }

    #[test]
    fn roundtrip_send() {
        let mut buf = [0u8; FRAME_LEN];
        let n = Frame::write(&hdr(Opcode::Send), None, b"hello", &mut buf).unwrap();
        assert_eq!(n, HEADER_LEN + 5);
        let f = Frame::parse(&buf[..n]).unwrap();
        assert_eq!(f.header, hdr(Opcode::Send));
        assert_eq!(f.payload, b"hello");
        assert!(f.reth.is_none());
    }

    #[test]
    fn roundtrip_write_with_reth() {
        let r = Reth {
            rkey: 0x1234_5678,
            offset: 1 << 40,
            length: 4054,
        };
        let mut buf = [0u8; FRAME_LEN];
        let n = Frame::write(&hdr(Opcode::Write), Some(&r), &[9; 100], &mut buf).unwrap();
        let f = Frame::parse(&buf[..n]).unwrap();
        assert_eq!(f.reth, Some(r));
        assert_eq!(f.payload.len(), 100);
    }

    #[test]
    fn reth_mismatch_rejected() {
        let mut buf = [0u8; 64];
        assert_eq!(
            Frame::write(&hdr(Opcode::Write), None, &[], &mut buf),
            Err(Error::RethMismatch)
        );
        let r = Reth {
            rkey: 1,
            offset: 0,
            length: 0,
        };
        assert_eq!(
            Frame::write(&hdr(Opcode::Send), Some(&r), &[], &mut buf),
            Err(Error::RethMismatch)
        );
    }

    #[test]
    fn strict_decode() {
        let mut b = [0u8; HEADER_LEN];
        hdr(Opcode::Ack).encode(&mut b).unwrap();
        let mut bad = b;
        bad[0] = 9;
        assert_eq!(Header::decode(&bad), Err(Error::BadVersion(9)));
        let mut bad = b;
        bad[1] = 99;
        assert_eq!(Header::decode(&bad), Err(Error::BadOpcode(99)));
        let mut bad = b;
        bad[25] = 1;
        assert_eq!(Header::decode(&bad), Err(Error::Reserved));
        assert_eq!(Header::decode(&b[..10]), Err(Error::Short));
        // Write frame truncated before RETH completes.
        let mut w = [0u8; HEADER_LEN + 4];
        hdr(Opcode::Write).encode(&mut w).unwrap();
        assert_eq!(Frame::parse(&w), Err(Error::Short));
    }

    #[test]
    fn fuzz_never_panics() {
        // Deterministic xorshift; every input must return Ok or Err, never panic.
        let mut s = 0x9E3779B97F4A7C15u64;
        let mut buf = [0u8; 128];
        for _ in 0..200_000 {
            for b in buf.iter_mut() {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                *b = s as u8;
            }
            if s & 3 != 0 {
                buf[0] = FORMAT_VERSION;
                buf[24] = 0;
                buf[25] = 0;
                buf[1] = 1 + (s >> 8) as u8 % 12;
            }
            let len = (s >> 16) as usize % 129;
            let _ = Frame::parse(&buf[..len]);
        }
    }

    #[test]
    fn uc_profile_is_send_only() {
        assert!(Opcode::Send.allowed_in_uc());
        assert!(!Opcode::Write.allowed_in_uc());
    }
}
