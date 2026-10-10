// SPDX-License-Identifier: GPL-2.0-only
//! Type-state queue pair: Reset -> Init -> Rtr -> Rts, and Errored (doc section 5).
use core::marker::PhantomData;

pub struct Reset;
pub struct Init;
pub struct Rtr;
pub struct Rts;
pub struct Errored;
pub trait QpState {}
impl QpState for Reset {}
impl QpState for Init {}
impl QpState for Rtr {}
impl QpState for Rts {}
impl QpState for Errored {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr {
    pub node: u16,
    pub qpn: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxVerdict {
    InOrder,
    Duplicate,
    Gap,
}

#[derive(Debug)]
pub struct Qp<S: QpState> {
    pub qpn: u32,
    peer: Option<PeerAddr>,
    tx_psn: u32,
    rx_expected: u32,
    _s: PhantomData<S>,
}

impl<S: QpState> Qp<S> {
    fn into_state<T: QpState>(self) -> Qp<T> {
        Qp {
            qpn: self.qpn,
            peer: self.peer,
            tx_psn: self.tx_psn,
            rx_expected: self.rx_expected,
            _s: PhantomData,
        }
    }
    /// Any state may fail into Err (flush completions happen at the caller).
    pub fn to_err(self) -> Qp<Errored> {
        self.into_state()
    }
}

impl Qp<Reset> {
    pub fn new(qpn: u32) -> Self {
        Qp {
            qpn,
            peer: None,
            tx_psn: 0,
            rx_expected: 0,
            _s: PhantomData,
        }
    }
    pub fn to_init(self) -> Qp<Init> {
        self.into_state()
    }
}

impl Qp<Init> {
    /// `rx_start` is the first PSN we expect from the peer.
    pub fn to_rtr(mut self, peer: PeerAddr, rx_start: u32) -> Qp<Rtr> {
        self.peer = Some(peer);
        self.rx_expected = rx_start;
        self.into_state()
    }
}

impl Qp<Rtr> {
    pub fn to_rts(mut self, tx_start: u32) -> Qp<Rts> {
        self.tx_psn = tx_start;
        self.into_state()
    }
}

impl Qp<Rts> {
    pub fn peer(&self) -> PeerAddr {
        self.peer.expect("RTS implies peer")
    }

    pub fn next_tx_psn(&mut self) -> u32 {
        let p = self.tx_psn;
        self.tx_psn = self.tx_psn.wrapping_add(1);
        p
    }

    /// Check an incoming PSN. On a direct lossless link a Gap is fatal and the
    /// caller must move the QP to Err (doc section 4, Reliability).
    pub fn check_rx(&mut self, psn: u32) -> RxVerdict {
        if psn == self.rx_expected {
            self.rx_expected = self.rx_expected.wrapping_add(1);
            RxVerdict::InOrder
        } else if (self.rx_expected.wrapping_sub(psn) as i32) > 0 {
            RxVerdict::Duplicate
        } else {
            RxVerdict::Gap
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rts() -> Qp<Rts> {
        Qp::new(5)
            .to_init()
            .to_rtr(PeerAddr { node: 2, qpn: 9 }, 100)
            .to_rts(0)
    }

    #[test]
    fn psn_sequence_and_wrap() {
        let mut q = Qp::new(1)
            .to_init()
            .to_rtr(PeerAddr { node: 2, qpn: 3 }, u32::MAX)
            .to_rts(u32::MAX);
        assert_eq!(q.next_tx_psn(), u32::MAX);
        assert_eq!(q.next_tx_psn(), 0);
        assert_eq!(q.check_rx(u32::MAX), RxVerdict::InOrder);
        assert_eq!(q.check_rx(0), RxVerdict::InOrder);
    }

    #[test]
    fn rx_verdicts() {
        let mut q = rts();
        assert_eq!(q.check_rx(100), RxVerdict::InOrder);
        assert_eq!(q.check_rx(100), RxVerdict::Duplicate);
        assert_eq!(q.check_rx(103), RxVerdict::Gap);
        let _e: Qp<Errored> = q.to_err();
    }
    // Invalid transitions (e.g. Reset -> Rts) do not compile: no such method.
}
