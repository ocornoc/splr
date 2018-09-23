use solver::{CDCL, Solver, Stat};
use types::*;

pub trait Restart {
    fn block_restart(&mut self, lbd: usize, clv: usize, blv: usize, nas: usize) -> ();
    fn force_restart(&mut self) -> ();
}

const RESTART_PERIOD: u64 = 50;

const RESET_EMA: u64 = 50;

/// for block restart based on average assigments: 1.40
const R: f64 = 1.5;

/// for force restart based on average LBD of newly generated clauses: 1.15
const K: f64 = 1.6;

impl Restart for Solver {
    /// called after conflict resolution
    fn block_restart(&mut self, lbd: usize, clv: usize, blv: usize, nas: usize) -> () {
        let count = self.stats[Stat::Conflict as usize] as u64;
        self.c_lvl.update(clv as f64);
        self.b_lvl.update(blv as f64);
        self.ema_asg.update(nas as f64 / self.c_lvl.0);
        self.ema_lbd.update(lbd as f64 / self.b_lvl.0);
        if count == RESET_EMA {
            self.ema_asg.reset();
            self.ema_lbd.reset();
            self.c_lvl.reset();
            self.b_lvl.reset();
        }
        if self.next_restart <= count && 0 < lbd && R < self.ema_asg.get() {
            self.next_restart = count + RESTART_PERIOD;
            self.stats[Stat::BlockRestart as usize] += 1;
        }
    }

    /// called after no conflict propagation
    fn force_restart(&mut self) -> () {
        let count = self.stats[Stat::Conflict as usize] as u64;
        if self.next_restart < count && K < self.ema_lbd.get() {
            self.next_restart = count + RESTART_PERIOD;
            self.stats[Stat::Restart as usize] += 1;
            let rl = self.root_level;
            self.cancel_until(rl);
        }
    }
}
