// SPDX-License-Identifier: GPL-2.0-only
//! Per-peer credit accounting with a reserved control class (doc section 4).
//!
//! Data frames consume data credits granted by the receiver. Control frames
//! (ACK/CREDIT/TERMINATE) draw on a separate fixed pool so a data stall can
//! never block the frames that would unstall it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Data,
    Control,
}

#[derive(Debug)]
pub struct CreditPool {
    data: u32,
    ctrl: u32,
    ctrl_max: u32,
}

impl CreditPool {
    pub fn new(initial_data: u32, ctrl_reserved: u32) -> Self {
        Self {
            data: initial_data,
            ctrl: ctrl_reserved,
            ctrl_max: ctrl_reserved,
        }
    }

    pub fn available(&self, c: Class) -> u32 {
        match c {
            Class::Data => self.data,
            Class::Control => self.ctrl,
        }
    }

    /// Try to spend one credit. Returns false (and spends nothing) if empty.
    pub fn try_consume(&mut self, c: Class) -> bool {
        let slot = match c {
            Class::Data => &mut self.data,
            Class::Control => &mut self.ctrl,
        };
        if *slot == 0 {
            return false;
        }
        *slot -= 1;
        true
    }

    /// Peer granted `n` more data credits (from the header `credits` field).
    pub fn grant_data(&mut self, n: u16) {
        self.data = self.data.saturating_add(n as u32);
    }

    /// Control credits are returned when the peer has consumed the frame.
    pub fn return_ctrl(&mut self, n: u32) {
        self.ctrl = (self.ctrl + n).min(self.ctrl_max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_not_starved_by_data() {
        let mut p = CreditPool::new(2, 1);
        assert!(p.try_consume(Class::Data));
        assert!(p.try_consume(Class::Data));
        assert!(!p.try_consume(Class::Data));
        assert!(p.try_consume(Class::Control)); // still allowed
        assert!(!p.try_consume(Class::Control));
        p.return_ctrl(5);
        assert_eq!(p.available(Class::Control), 1); // capped at reservation
        p.grant_data(3);
        assert_eq!(p.available(Class::Data), 3);
    }
}
