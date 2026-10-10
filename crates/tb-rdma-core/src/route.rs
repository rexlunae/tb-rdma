// SPDX-License-Identifier: GPL-2.0-only
//! Multi-hop forwarding decisions and shortest-path next-hop (doc section 7).
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tb_rdma_proto::Header;

pub type NodeId = u16;

#[derive(Debug, PartialEq, Eq)]
pub enum RxAction {
    Deliver,
    Forward(NodeId),
    Drop(DropReason),
}

#[derive(Debug, PartialEq, Eq)]
pub enum DropReason {
    TtlExpired,
    NoRoute,
}

/// Link-state view of the cluster. Edges are undirected links between nodes.
#[derive(Default, Debug, Clone)]
pub struct Topology {
    adj: BTreeMap<NodeId, BTreeSet<NodeId>>,
}

impl Topology {
    pub fn add_link(&mut self, a: NodeId, b: NodeId) {
        self.adj.entry(a).or_default().insert(b);
        self.adj.entry(b).or_default().insert(a);
    }
    pub fn remove_link(&mut self, a: NodeId, b: NodeId) {
        if let Some(s) = self.adj.get_mut(&a) {
            s.remove(&b);
        }
        if let Some(s) = self.adj.get_mut(&b) {
            s.remove(&a);
        }
    }

    /// First hop on a minimum-hop path from `from` to `to` (BFS; ties broken
    /// by lowest node id so every node computes consistent routes).
    pub fn next_hop(&self, from: NodeId, to: NodeId) -> Option<NodeId> {
        if from == to {
            return None;
        }
        let mut prev: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        let mut q = VecDeque::from([from]);
        let mut seen = BTreeSet::from([from]);
        while let Some(n) = q.pop_front() {
            for &m in self.adj.get(&n)? {
                if seen.insert(m) {
                    prev.insert(m, n);
                    if m == to {
                        let mut cur = to;
                        while prev[&cur] != from {
                            cur = prev[&cur];
                        }
                        return Some(cur);
                    }
                    q.push_back(m);
                }
            }
        }
        None
    }
}

pub struct Router {
    pub me: NodeId,
    pub topo: Topology,
}

impl Router {
    /// Decide what to do with a received header. Decrements TTL in place when
    /// forwarding (caller re-encodes the header).
    pub fn on_rx(&self, h: &mut Header) -> RxAction {
        if h.dst == self.me {
            return RxAction::Deliver;
        }
        if h.ttl <= 1 {
            return RxAction::Drop(DropReason::TtlExpired);
        }
        match self.topo.next_hop(self.me, h.dst) {
            Some(n) => {
                h.ttl -= 1;
                RxAction::Forward(n)
            }
            None => RxAction::Drop(DropReason::NoRoute),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tb_rdma_proto::Opcode;

    fn h(dst: u16, ttl: u8) -> Header {
        Header {
            opcode: Opcode::Send,
            flags: 0,
            dst,
            src: 1,
            ttl,
            vc: 0,
            qpn: 1,
            psn: 0,
            ack_psn: 0,
            credits: 0,
        }
    }

    fn line() -> Topology {
        let mut t = Topology::default();
        t.add_link(1, 2);
        t.add_link(2, 3);
        t
    }

    #[test]
    fn line_topology_next_hop() {
        let t = line();
        assert_eq!(t.next_hop(1, 3), Some(2));
        assert_eq!(t.next_hop(3, 1), Some(2));
        assert_eq!(t.next_hop(1, 2), Some(2));
        assert_eq!(t.next_hop(1, 9), None);
    }

    #[test]
    fn reroutes_after_link_failure() {
        let mut t = Topology::default();
        for (a, b) in [(1, 2), (2, 4), (1, 3), (3, 4)] {
            t.add_link(a, b);
        }
        assert_eq!(t.next_hop(1, 4), Some(2));
        t.remove_link(2, 4);
        assert_eq!(t.next_hop(1, 4), Some(3));
    }

    #[test]
    fn router_decisions() {
        let r = Router {
            me: 2,
            topo: line(),
        };
        let mut hd = h(3, 5);
        assert_eq!(r.on_rx(&mut hd), RxAction::Forward(3));
        assert_eq!(hd.ttl, 4);
        assert_eq!(r.on_rx(&mut h(2, 5)), RxAction::Deliver);
        assert_eq!(
            r.on_rx(&mut h(3, 1)),
            RxAction::Drop(DropReason::TtlExpired)
        );
        assert_eq!(r.on_rx(&mut h(9, 5)), RxAction::Drop(DropReason::NoRoute));
    }
}
