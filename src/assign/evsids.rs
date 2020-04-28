/// Var Rewarding based on EVSIDS
use {super::AssignStack, crate::types::*, std::slice::Iter};

/// API for var rewarding.
pub trait VarRewardIF {
    /// return var's activity.
    fn activity(&mut self, vi: VarId) -> f64;
    /// initialize rewards based on an order of vars.
    fn initialize_reward(&mut self, iterator: Iter<'_, usize>);
    /// clear var's activity
    fn clear_reward(&mut self, vi: VarId);
    /// modify var's activity at conflict analysis in `analyze`.
    fn reward_at_analysis(&mut self, vi: VarId);
    /// modify var's activity at value assignment in `uncheck_{assume, enqueue, fix}`.
    fn reward_at_assign(&mut self, vi: VarId);
    /// modify var's activity at value unassigment in `cancel_until`.
    fn reward_at_unassign(&mut self, vi: VarId);
    /// update internal counter.
    fn reward_update(&mut self);
}

impl VarRewardIF for AssignStack {
    #[inline]
    fn activity(&mut self, vi: VarId) -> f64 {
        self.var[vi].reward
    }
    fn initialize_reward(&mut self, _iterator: Iter<'_, usize>) {
        self.reward_step = 1.0;
    }
    fn clear_reward(&mut self, vi: VarId) {
        self.var[vi].reward = 0.0;
    }
    fn reward_at_analysis(&mut self, vi: VarId) {
        let s = self.reward_step;
        let t = self.ordinal;
        let v = &mut self.var[vi];
        if v.timestamp == t {
            return;
        }
        v.timestamp = t;
        v.reward += s;
        const SCALE: f64 = 1e-100;
        const SCALE_MAX: f64 = 1e100;
        if SCALE_MAX < v.reward {
            for v in &mut self.var[1..] {
                v.reward *= SCALE;
            }
            self.reward_step *= SCALE;
        }
    }
    fn reward_at_assign(&mut self, _: VarId) {}
    fn reward_at_unassign(&mut self, _: VarId) {}
    fn reward_update(&mut self) {
        self.ordinal += 1;
        const INC_SCALE: f64 = 1.01;
        self.reward_step *= INC_SCALE;
    }
}
