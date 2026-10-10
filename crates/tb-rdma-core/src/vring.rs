// SPDX-License-Identifier: GPL-2.0-only
//! In-memory virtual ring: a bounded, lossless, in-order frame queue standing
//! in for an NHI TX/RX ring pair so protocol logic runs without hardware.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum PushError {
    Full,
}

#[derive(Clone)]
pub struct VRing {
    q: Arc<Mutex<VecDeque<Vec<u8>>>>,
    cap: usize,
}

impl VRing {
    pub fn new(cap: usize) -> Self {
        Self {
            q: Default::default(),
            cap,
        }
    }
    pub fn push(&self, frame: &[u8]) -> Result<(), PushError> {
        let mut q = self.q.lock().unwrap();
        if q.len() >= self.cap {
            return Err(PushError::Full);
        }
        q.push_back(frame.to_vec());
        Ok(())
    }
    pub fn pop(&self) -> Option<Vec<u8>> {
        self.q.lock().unwrap().pop_front()
    }
    pub fn len(&self) -> usize {
        self.q.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A bidirectional link: what `a` transmits, `b` receives, and vice versa.
pub struct VLink {
    pub a_tx: VRing,
    pub b_tx: VRing,
}
impl VLink {
    pub fn new(cap: usize) -> Self {
        Self {
            a_tx: VRing::new(cap),
            b_tx: VRing::new(cap),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{qp::*, route::*};
    use tb_rdma_proto::*;

    #[test]
    fn ring_is_bounded_and_ordered() {
        let r = VRing::new(2);
        r.push(b"a").unwrap();
        r.push(b"b").unwrap();
        assert_eq!(r.push(b"c"), Err(PushError::Full));
        assert_eq!(r.pop().unwrap(), b"a");
        assert_eq!(r.pop().unwrap(), b"b");
        assert!(r.pop().is_none());
    }

    /// End to end: node 1 sends to node 3 through node 2 over two virtual links.
    #[test]
    fn three_node_forwarding() {
        let (l12, l23) = (VLink::new(8), VLink::new(8));
        let mut topo = Topology::default();
        topo.add_link(1, 2);
        topo.add_link(2, 3);
        let transit = Router {
            me: 2,
            topo: topo.clone(),
        };
        let dest = Router { me: 3, topo };

        // Sender: QP on node 1 targeting node 3.
        let mut q1 = Qp::new(10)
            .to_init()
            .to_rtr(PeerAddr { node: 3, qpn: 20 }, 0)
            .to_rts(0);
        let mut q3 = Qp::new(20)
            .to_init()
            .to_rtr(PeerAddr { node: 1, qpn: 10 }, 0)
            .to_rts(0);

        for i in 0..3u8 {
            let h = Header {
                opcode: Opcode::Send,
                flags: flags::FIRST | flags::LAST,
                dst: 3,
                src: 1,
                ttl: 4,
                vc: 0,
                qpn: q1.peer().qpn,
                psn: q1.next_tx_psn(),
                ack_psn: 0,
                credits: 0,
            };
            let mut buf = [0u8; FRAME_LEN];
            let n = Frame::write(&h, None, &[i; 8], &mut buf).unwrap();
            l12.a_tx.push(&buf[..n]).unwrap();
        }

        // Transit node: pop from link 1-2, route, push onto link 2-3.
        while let Some(raw) = l12.a_tx.pop() {
            let mut h = Header::decode(&raw).unwrap();
            let Frame { payload, .. } = Frame::parse(&raw).unwrap();
            assert_eq!(transit.on_rx(&mut h), RxAction::Forward(3));
            let mut buf = [0u8; FRAME_LEN];
            let n = Frame::write(&h, None, payload, &mut buf).unwrap();
            l23.a_tx.push(&buf[..n]).unwrap();
        }

        // Destination: deliver in order.
        let mut got = vec![];
        while let Some(raw) = l23.a_tx.pop() {
            let f = Frame::parse(&raw).unwrap();
            let mut h = f.header;
            assert_eq!(dest.on_rx(&mut h), RxAction::Deliver);
            assert_eq!(q3.check_rx(h.psn), RxVerdict::InOrder);
            assert_eq!(h.ttl, 3); // decremented once at the transit hop
            got.push(f.payload[0]);
        }
        assert_eq!(got, vec![0, 1, 2]);
    }
}
