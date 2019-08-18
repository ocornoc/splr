use crate::config::{EMA_FAST, EMA_SLOW};
use crate::propagator::AssignStack;
use crate::state::Stat;
use crate::traits::*;
use crate::types::Flag;
use crate::var::{Var, VarDB};

const RESTART_THRESHOLD: f64 = 1.6;

/// Exponential Moving Average w/ a calibrator
#[derive(Debug)]
pub struct Ema {
    val: f64,
    cal: f64,
    sca: f64,
}

impl EmaIF for Ema {
    fn new(s: usize) -> Ema {
        Ema {
            val: 0.0,
            cal: 0.0,
            sca: 1.0 / (s as f64),
        }
    }
    fn update(&mut self, x: f64) {
        self.val = self.sca * x + (1.0 - self.sca) * self.val;
        self.cal = self.sca + (1.0 - self.sca) * self.cal;
    }
    fn get(&self) -> f64 {
        self.val / self.cal
    }
    fn initialize(mut self, init: f64) -> Self {
        self.val = init;
        self
    }
    fn reinitialize(&mut self, init: f64) -> &mut Self {
        self.val = init;
        self
    }
}

/// Exponential Moving Average pair
#[derive(Debug)]
pub struct Ema2 {
    fast: f64,
    slow: f64,
    calf: f64,
    cals: f64,
    fe: f64,
    se: f64,
}

impl EmaIF for Ema2 {
    fn new(s: usize) -> Ema2 {
        Ema2 {
            fast: 0.0,
            slow: 0.0,
            calf: 0.0,
            cals: 0.0,
            fe: 1.0 / (s as f64),
            se: 1.0 / (s as f64),
        }
    }
    fn get(&self) -> f64 {
        self.slow / self.cals
    }
    fn update(&mut self, x: f64) {
        self.fast = self.fe * x + (1.0 - self.fe) * self.fast;
        self.slow = self.se * x + (1.0 - self.se) * self.slow;
        self.calf = self.fe + (1.0 - self.fe) * self.calf;
        self.cals = self.se + (1.0 - self.se) * self.cals;
    }
    fn reset(&mut self) {
        self.fast = self.slow;
        self.calf = self.cals;
    }
    fn initialize(mut self, init: f64) -> Self {
        self.fast = init;
        self.slow = init;
        self
    }
    fn reinitialize(&mut self, init: f64) -> &mut Self {
        // self.fast = self.slow;
        self.fast = init * self.calf;
        // self.slow = init * self.cals;
        self
    }
}

impl Ema2 {
    pub fn get_fast(&self) -> f64 {
        self.fast / self.calf
    }
    pub fn trend(&self) -> f64 {
        self.fast / self.slow * (self.cals / self.calf)
    }
    pub fn with_fast(mut self, f: usize) -> Self {
        self.fe = 1.0 / (f as f64);
        self
    }
}

/// Exponential Moving Average w/ a calibrator
/// ### About levels
///
/// - Level 0 is a memory cleared at each restart
/// - Level 1 is a memory held during restarts but clear after mega-restart
/// - Level 2 is a memory not to reset by restarts
/// *Note: all levels clear after finding a unit learnt (simplification).*
#[derive(Debug)]
pub struct RestartExecutor {
    pub restart_ratio: Ema,
    pub stationary_thrd: (f64, f64),
    pub blocking_ema: Ema,
    pub after_restart: usize,
    pub increasing_fup: bool,
    pub trend_max: f64,
    pub trend_min: f64,
    pub use_luby: bool,
    pub lbd: RestartLBD,
    pub asg: RestartASG2,
    pub fup: VarSet,
    pub luby: RestartLuby,
}

impl RestartExecutor {
    pub fn new() -> RestartExecutor {
        RestartExecutor {
            restart_ratio: Ema::new(EMA_SLOW),
            stationary_thrd: (0.25, 0.98), // (fup.trend, decay factor),
            blocking_ema: Ema::new(EMA_SLOW),
            after_restart: 1,
            increasing_fup: true,
            trend_max: 0.0,
            trend_min: 1000.0,
            use_luby: false,
            lbd: RestartLBD::new(RESTART_THRESHOLD),
            asg: RestartASG2::new(RESTART_THRESHOLD),
            fup: VarSet::new(Flag::FUP),
            luby: RestartLuby::new(2.0, 100.0),
        }
    }
}

impl RestartIF for RestartExecutor {
    // stagnation-triggered restart engine
    fn restart(&mut self, _asgs: &mut AssignStack, _vdb: &mut VarDB, stats: &mut [usize]) -> bool {
        if self.use_luby {
            self.luby.update(true);
            if self.luby.is_active() {
                stats[Stat::RestartByLuby] += 1;
                stats[Stat::Restart] += 1;
                self.restart_ratio.update(1.0);
                return true;
            } else {
                return self.return_without_restart();
            }
        }
        let RestartExecutor {
            after_restart,
            increasing_fup,
            trend_min,
            trend_max,
            fup,
            ..
        } = self;
        *after_restart += 1;
        let mut peak = false;
        let mut band = 0.0;
        let trend = fup.trend();
        if *increasing_fup {
            if *trend_max < trend {
                *trend_max = trend;
            } else {
                peak = true;
                *increasing_fup = false;
                band = *trend_max - *trend_min;
                *trend_min = *trend_max;
            }
        } else {
            if trend < *trend_min {
                *trend_min = trend;
            } else {
                peak = true;
                *increasing_fup = true;
                band = *trend_max - *trend_min;
                *trend_max = *trend_min;
            }
        }
        if 10 < *after_restart && 0.01 < band && peak {
            self.return_for_restart(stats)
        } else {
            self.return_without_restart()
        }
    }
}

impl RestartExecutor {
    fn return_for_restart(&mut self, stats: &mut [usize]) -> bool {
        if 0 < self.after_restart {
            self.blocking_ema.update((self.after_restart - 1) as f64);
        }
        self.after_restart = 1;
        self.restart_ratio.update(1.0);
        stats[Stat::Restart] += 1;
        true
    }
    #[allow(dead_code)]
    fn return_for_blocking(&mut self, stats: &mut [usize]) -> bool {
        if 0 < self.after_restart {
            self.blocking_ema.update((self.after_restart - 1) as f64);
        }
        self.after_restart = 1;
        self.restart_ratio.update(0.0);
        stats[Stat::Blocking] += 1;
        false
    }
    fn return_without_restart(&mut self) -> bool {
        self.restart_ratio.update(0.0);
        false
    }
    #[allow(dead_code)]
    pub fn reset_fup(&mut self, vdb: &mut VarDB) {
        for v in &mut vdb.vars[1..] {
            self.fup.remove(v);
        }
        self.fup.reset();
    }
    #[allow(dead_code)]
    pub fn check_stationary_fup(&mut self, vdb: &mut VarDB) {
        if 100 < self.fup.num && self.fup.trend() < self.stationary_thrd.0 {
            self.stationary_thrd.0 *= self.stationary_thrd.1;
            self.reset_fup(vdb)
        }
    }
}
/// Glucose-style forcing restart condition w/o restart_steps
/// ### Implementing the original algorithm
///
/// ```ignore
///    rst = RestartLBD::new(1.4);
///    rst.add(learnt.lbd()).commit();
///    if rst.eval(|ema, ave| rst.threshold * ave < ema.get()) {
///        restarting...
///    }
/// ```
///
#[derive(Debug)]
pub struct RestartLBD {
    pub sum: f64,
    pub num: usize,
    pub threshold: f64,
    pub ema: Ema,
    lbd: Option<usize>,
    result: bool,
    timer: usize,
}

impl ProgressEvaluatorIF<'_> for RestartLBD {
    type Memory = Ema;
    type Item = usize;
    /*
    fn add(&mut self, item: Self::Item) -> &mut RestartLBD {
        // assert_eq!(self.lbd, None);
        self.sum += item as f64;
        self.num += 1;
        self.lbd = Some(item);
        self
    }
    fn commit(&mut self) -> &mut Self {
        assert!(!self.lbd.is_none());
        if let Some(lbd) = self.lbd {
            self.ema.update(lbd as f64);
            self.lbd = None;
        }
        self
    }
    */
    fn update(&mut self, item: Self::Item) {
        self.sum += item as f64;
        self.num += 1;
        self.ema.update(item as f64)
    }
    fn update_with<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&Self::Memory, f64) -> bool,
    {
        assert!(self.lbd.is_none());
        if 0 < self.timer {
            self.timer -= 1;
        } else {
            self.result = f(&self.ema, self.sum / self.num as f64);
            if self.result {
                self.timer = 50;
            }
        }
        self
    }
    fn is_active(&self) -> bool {
        0 < self.timer && self.result
    }
    fn eval<F, R>(&self, f: F) -> R
    where
        F: Fn(&Self::Memory, f64) -> R,
    {
        assert!(self.lbd.is_none());
        f(&self.ema, self.sum / self.num as f64)
    }
    fn trend(&self) -> f64 {
        self.ema.get() * self.num as f64 / self.sum as f64
    }
}

impl RestartLBD {
    pub fn new(threshold: f64) -> Self {
        RestartLBD {
            sum: 0.0,
            num: 0,
            threshold,
            ema: Ema::new(EMA_FAST),
            lbd: None,
            result: false,
            timer: 0,
        }
    }
}

/// Glucose-style restart blocking condition w/o restart_steps
/// ### Implementing the original algorithm
///
/// ```ignore
///    blk = RestartASG::new(1.0 / 0.8);
///    blk.add(solver.num_assigns).commit();
///    if blk.eval(|ema, ave| blk.threshold * ave < ema.get()) {
///        blocking...
///    }
/// ```
/*
#[derive(Debug)]
pub struct RestartASG {
    pub max: usize,
    pub sum: usize,
    pub num: usize,
    pub threshold: f64,
    pub ema: Ema,
    asg: Option<usize>,
    result: bool,
    // timer: usize,
}

impl ProgressEvaluatorIF<'_> for RestartASG {
    type Memory = Ema;
    type Item = usize;
    /*
    fn add(&mut self, item: Self::Item) -> &mut Self {
        assert!(self.asg.is_none());
        self.sum += item;
        self.num += 1;
        self.asg = Some(item);
        self
    }
    fn commit(&mut self) -> &mut Self {
        assert!(!self.asg.is_none());
        if let Some(a) = self.asg {
            self.ema.update(a as f64);
            self.asg = None;
            self.max = a.max(self.max);
        }
        self
    }
    */
    fn update(&mut self, item: Self::Item) {
        self.ema.update(item as f64);
        self.max = item.max(self.max);
    }
    fn update_with<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&Self::Memory, f64) -> bool,
    {
        assert!(self.asg.is_none());
        self.result = f(&self.ema, self.sum as f64 / self.num as f64);
        self
    }
    fn is_active(&self) -> bool {
        self.result
    }
    fn eval<F, R>(&self, f: F) -> R
    where
        F: Fn(&Self::Memory, f64) -> R,
    {
        assert!(self.asg.is_none());
        f(&self.ema, self.sum as f64 / self.num as f64)
    }
    fn trend(&self) -> f64 {
        self.ema.get() * self.num as f64 / self.sum as f64
    }
}

impl RestartASG {
    pub fn new(threshold: f64) -> Self {
        RestartASG {
            max: 0,
            sum: 0,
            num: 0,
            threshold,
            ema: Ema::new(EMA_FAST),
            asg: None,
            result: false,
            // timer: 20,
        }
    }
}
 */

#[derive(Debug)]
pub struct RestartASG2 {
    pub max: usize,
    pub sum: usize,
    pub num: usize,
    pub threshold: f64,
    ema: Ema2,
    asg: Option<usize>,
    result: bool,
    // timer: usize,
}

impl ProgressEvaluatorIF<'_> for RestartASG2 {
    type Memory = Ema2;
    type Item = usize;
    /*
    fn add(&mut self, item: Self::Item) -> &mut Self {
        assert!(self.asg.is_none());
        self.sum += item;
        self.num += 1;
        self.asg = Some(item);
        self
    }
    fn commit(&mut self) -> &mut Self {
        assert!(!self.asg.is_none());
        if let Some(a) = self.asg {
            self.ema.update(a as f64);
            self.asg = None;
            self.max = a.max(self.max);
        }
        self
    }
    */
    fn update(&mut self, item: Self::Item) {
        self.ema.update(item as f64);
        self.max = item.max(self.max);
    }
    fn update_with<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&Self::Memory, f64) -> bool,
    {
        assert!(self.asg.is_none());
        self.result = f(&self.ema, self.sum as f64 / self.num as f64);
        self
    }
    fn is_active(&self) -> bool {
        self.result
    }
    fn eval<F, R>(&self, f: F) -> R
    where
        F: Fn(&Self::Memory, f64) -> R,
    {
        assert!(self.asg.is_none());
        f(&self.ema, self.sum as f64 / self.num as f64)
    }
    fn trend(&self) -> f64 {
        self.ema.trend()
    }
}

impl RestartASG2 {
    pub fn new(threshold: f64) -> Self {
        RestartASG2 {
            max: 0,
            sum: 0,
            num: 0,
            threshold,
            ema: Ema2::new(EMA_SLOW).with_fast(EMA_FAST),
            asg: None,
            result: false,
            // timer: 20,
        }
    }
    pub fn get(&self) -> f64 {
        self.ema.get()
    }
    pub fn get_fast(&self) -> f64 {
        self.ema.get_fast()
    }
    pub fn reset(&mut self) {
        self.max = 0;
        self.sum = 0;
        self.num = 0;
    }
}

#[derive(Debug)]
pub struct RestartLuby {
    pub num_conflict: f64,
    pub inc: f64,
    pub current_restart: usize,
    pub factor: f64,
    pub cnfl_cnt: f64,
    result: bool,
}

impl ProgressEvaluatorIF<'_> for RestartLuby {
    type Memory = usize;
    type Item = bool;
    /*
    fn add(&mut self, in_use: Self::Item) -> &mut Self {
        if in_use {
            self.cnfl_cnt += 1.0;
        }
        self
    }
    fn commit(&mut self) -> &mut Self {
        self
    }
    */
    fn update(&mut self, in_use: Self::Item) {
        if in_use {
            self.cnfl_cnt += 1.0;
            if self.num_conflict <= self.cnfl_cnt {
                self.cnfl_cnt = 0.0;
                self.current_restart += 1;
                self.num_conflict = luby(self.inc, self.current_restart) * self.factor;
                self.result = true;
            } else {
                self.result = false;
            }
        }
    }
    fn update_with<F>(&mut self, _f: F) -> &mut Self
    where
        F: Fn(&Self::Memory, f64) -> bool,
    {
        if self.num_conflict <= self.cnfl_cnt {
            self.cnfl_cnt = 0.0;
            self.current_restart += 1;
            self.num_conflict = luby(self.inc, self.current_restart) * self.factor;
            self.result = true;
        } else {
            self.result = false;
        }
        self
    }
    fn is_active(&self) -> bool {
        self.result
    }
    fn eval<F, R>(&self, _f: F) -> R
    where
        F: Fn(&Self::Memory, f64) -> R,
    {
        panic!("RestartLuby doesn't implement check.");
    }
    fn trend(&self) -> f64 {
        0.0
    }
}

impl Default for RestartLuby {
    fn default() -> Self {
        RestartLuby {
            num_conflict: 0.0,
            inc: 2.0,
            current_restart: 0,
            factor: 100.0,
            cnfl_cnt: 0.0,
            result: false,
        }
    }
}

impl RestartLuby {
    pub fn new(inc: f64, factor: f64) -> Self {
        RestartLuby {
            num_conflict: 0.0,
            inc,
            current_restart: 0,
            factor,
            cnfl_cnt: 0.0,
            result: false,
        }
    }
    pub fn initialize(&mut self) -> &mut Self {
        self.cnfl_cnt = 0.0;
        self.num_conflict = luby(self.inc, self.current_restart) * self.factor;
        self
    }
}

/// Find the finite subsequence that contains index 'x', and the
/// size of that subsequence:
fn luby(y: f64, mut x: usize) -> f64 {
    let mut size: usize = 1;
    let mut seq: usize = 0;
    // for(size = 1, seq = 0; size < x + 1; seq++, size = 2 * size + 1);
    while size < x + 1 {
        seq += 1;
        size = 2 * size + 1;
    }
    // while(size - 1 != x) {
    //     size = (size - 1) >> 1;
    //     seq--;
    //     x = x % size;
    // }
    while size - 1 != x {
        size = (size - 1) >> 1;
        seq -= 1;
        x %= size;
    }
    // return pow(y, seq);
    y.powf(seq as f64)
}

/// A superium of variables as a metrics of progress of search.
/// There were a various categories:
/// - assigned variable set, AVS
/// - assigned or cancelled variables, ACV
/// - superium of ACV (not being reset), SuA
/// - first UIPs, FUP
/// - superium of first UIPs, SuA
///
/// AVS was a variant of the present AGS. And it has no flag in Var::flag field.
#[derive(Debug)]
pub struct VarSet {
    pub flag: Flag,
    pub sum: usize,
    pub num: usize,
    pub diff: Option<f64>,
    pub diff_ema: Ema2,
    is_closed: bool,
    pub num_build: usize,
}

impl VarSetIF for VarSet {
    fn new(flag: Flag) -> Self {
        VarSet {
            flag,
            sum: 0,
            num: 0,
            diff: None,
            diff_ema: Ema2::new(EMA_SLOW).with_fast(EMA_FAST * 4),
            is_closed: false,
            num_build: 1,
        }
    }
    fn remove(&self, v: &mut Var) {
        if v.is(self.flag) {
            v.turn_off(self.flag);
        }
    }
    fn reset(&mut self) {
        self.sum = 0;
        self.num = 0;
        self.diff = None;
        self.is_closed = false;
        self.diff_ema.reinitialize(1.0);
        self.num_build += 1;
    }
    fn reach_top(&self) -> bool {
        self.diff_ema.get_fast() <= self.diff_ema.get()
    }
    fn reach_bottom(&self) -> bool {
        true
    }
}

impl<'a> ProgressEvaluatorIF<'a> for VarSet {
    type Memory = Ema2;
    type Item = &'a mut Var;
    /*
    fn add(&mut self, v: Self::Item) -> &mut Self {
        self.num += 1;
        if !v.is(self.flag) {
            v.turn_on(self.flag);
            self.sum += 1;
            self.diff = Some(self.diff.map_or(1.0, |v| v + 1.0));
        } else if self.diff.is_none() {
            self.diff = Some(0.0);
        }
        self
    }
    fn commit(&mut self) -> &mut Self {
        if let Some(diff) = self.diff {
            self.diff_ema.update(diff);
            self.diff = None;
        } else {
            panic!("VarSet {:?}::commit, self.diff is None", self.flag);
        }
        self
    }
    */
    fn update(&mut self, v: Self::Item) {
        self.num += 1;
        if !v.is(self.flag) {
            v.turn_on(self.flag);
            self.sum += 1;
            self.diff_ema.update(1.0);
        } else {
            self.diff_ema.update(0.0);
        }
    }
    fn update_with<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&Self::Memory, f64) -> bool,
    {
        // assert!(self.diff.is_none());
        self.is_closed = f(&self.diff_ema, self.sum as f64 / self.num as f64);
        self
    }
    fn is_active(&self) -> bool {
        self.is_closed
    }
    fn eval<F, R>(&self, f: F) -> R
    where
        F: Fn(&Self::Memory, f64) -> R,
    {
        // assert!(self.diff.is_none());
        f(&self.diff_ema, self.sum as f64 / self.num as f64)
    }
    fn trend(&self) -> f64 {
        self.diff_ema.get() * self.num as f64 / self.sum as f64
    }
}
