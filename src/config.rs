use crate::clause::{ClauseDB, ClauseFlag, GC};
use crate::eliminator::Eliminator;
use crate::restart::{RESTART_BLK, RESTART_THR};
use crate::state::{SolverState, Stat};
use crate::types::{EmaKind, LiteralEncoding};
use crate::var::Var;

#[derive(Eq, PartialEq)]
pub enum SearchStrategy {
    Initial,
    Generic,
    LowDecisions,
    HighSuccesive,
    LowSuccesive,
    ManyGlues,
}

impl SearchStrategy {
    pub fn to_str(&self) -> &'static str {
        match self {
            SearchStrategy::Initial => "init",
            SearchStrategy::Generic => "dflt",
            SearchStrategy::LowDecisions => "LowD",
            SearchStrategy::HighSuccesive => "High",
            SearchStrategy::LowSuccesive => "LowS",
            SearchStrategy::ManyGlues => "Many",
        }
    }
}

/// `Solver`'s parameters; random decision rate was dropped.
pub struct SolverConfiguration {
    pub root_level: usize,
    pub num_vars: usize,
    /// STARATEGY
    pub adapt_strategy: bool,
    pub strategy: SearchStrategy,
    pub use_chan_seok: bool,
    pub co_lbd_bound: usize,
    /// CLAUSE/VARIABLE ACTIVITY
    pub cla_decay: f64,
    pub cla_inc: f64,
    pub var_decay: f64,
    pub var_decay_max: f64,
    pub var_inc: f64,
    /// CLAUSE REDUCTION
    pub first_reduction: usize,
    pub glureduce: bool,
    pub inc_reduce_db: usize,
    pub inc_reduce_db_extra: usize,
    pub ema_coeffs: (i32, i32),
    /// RESTART
    pub restart_thr: f64,
    pub restart_blk: f64,
    pub restart_expansion: f64,
    pub restart_step: f64,
    pub luby_restart: bool,
    pub luby_restart_num_conflict: f64,
    pub luby_restart_inc: f64,
    pub luby_current_restarts: usize,
    pub luby_restart_factor: f64,
    /// MISC
    pub use_sve: bool,
    pub use_tty: bool,
    /// dump stats data during solving
    pub dump_solver_stat_mode: i32,
}

impl Default for SolverConfiguration {
    fn default() -> SolverConfiguration {
        SolverConfiguration {
            root_level: 0,
            num_vars: 0,
            adapt_strategy: true,
            strategy: SearchStrategy::Initial,
            use_chan_seok: false,
            co_lbd_bound: 5,
            cla_decay: 0.999,
            cla_inc: 1.0,
            var_decay: 0.9,
            var_decay_max: 0.95,
            var_inc: 0.9,
            first_reduction: 1000,
            glureduce: true,
            inc_reduce_db: 300,
            inc_reduce_db_extra: 1000,
            restart_thr: RESTART_THR,
            restart_blk: RESTART_BLK,
            restart_expansion: 1.15,
            restart_step: 100.0,
            luby_restart: false,
            luby_restart_num_conflict: 0.0,
            luby_restart_inc: 2.0,
            luby_current_restarts: 0,
            luby_restart_factor: 100.0,
            ema_coeffs: (2 ^ 5, 2 ^ 15),
            use_sve: true,
            use_tty: true,
            dump_solver_stat_mode: 0,
        }
    }
}

impl SolverConfiguration {
    #[inline(always)]
    pub fn adapt_strategy(
        &mut self,
        cps: &mut ClauseDB,
        elim: &mut Eliminator,
        state: &mut SolverState,
        vars: &mut [Var],
    ) {
        if !self.adapt_strategy || self.strategy != SearchStrategy::Initial {
            return;
        }
        let mut re_init = false;
        let decpc =
            state.stat[Stat::Decision as usize] as f64 / state.stat[Stat::Conflict as usize] as f64;
        if decpc <= 1.2 {
            self.strategy = SearchStrategy::LowDecisions;
            self.use_chan_seok = true;
            self.co_lbd_bound = 4;
            self.glureduce = true;
            self.first_reduction = 2000;
            state.next_reduction = 2000;
            state.cur_restart = (state.stat[Stat::Conflict as usize] as f64
                / state.next_reduction as f64
                + 1.0) as usize;
            self.inc_reduce_db = 0;
            re_init = true;
        }
        if state.stat[Stat::NoDecisionConflict as usize] < 30_000 {
            self.strategy = SearchStrategy::LowSuccesive;
            self.luby_restart = true;
            self.luby_restart_factor = 100.0;
            self.var_decay = 0.999;
            self.var_decay_max = 0.999;
        }
        if state.stat[Stat::NoDecisionConflict as usize] > 54_400 {
            self.strategy = SearchStrategy::HighSuccesive;
            self.use_chan_seok = true;
            self.glureduce = true;
            self.co_lbd_bound = 3;
            self.first_reduction = 30000;
            self.var_decay = 0.99;
            self.var_decay_max = 0.99;
            // randomize_on_restarts = 1;
        }
        if state.stat[Stat::NumLBD2 as usize] - state.stat[Stat::NumBin as usize] > 20_000 {
            self.strategy = SearchStrategy::ManyGlues;
            self.var_decay = 0.91;
            self.var_decay_max = 0.91;
        }
        if self.strategy == SearchStrategy::Initial {
            self.strategy = SearchStrategy::Generic;
            return;
        }
        state.ema_asg.reset();
        state.ema_lbd.reset();
        state.lbd_queue.clear();
        state.stat[Stat::SumLBD as usize] = 0;
        state.stat[Stat::Conflict as usize] = 0;
        let [_, learnts, permanents, _] = cps;
        if self.use_chan_seok {
            // println!("# Adjusting for low decision levels.");
            // move some clauses with good lbd (col_lbd_bound) to Permanent
            for ch in &mut learnts.head[1..] {
                if ch.get_flag(ClauseFlag::Dead) {
                    continue;
                }
                if ch.rank <= self.co_lbd_bound || re_init {
                    if ch.rank <= self.co_lbd_bound {
                        permanents.new_clause(&ch.lits, ch.rank);
                    }
                    learnts.touched[ch.lits[0].negate() as usize] = true;
                    learnts.touched[ch.lits[1].negate() as usize] = true;
                    ch.flag_on(ClauseFlag::Dead);
                }
            }
            learnts.garbage_collect(vars, elim);
        }
    }
}
