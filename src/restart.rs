use {
    crate::{
        config::Config,
        traits::*,
        types::*,
    },
    std::fmt,
};

// const RESET_EMA: usize = 400;

#[derive(Debug)]
pub struct ProgressASG {
    asg: usize,
    ema: Ema,
    /// For block restart based on average assignments: 1.40.
    /// This is called `R` in Glucose
    pub threshold: f64,
    pub best: usize,
}

impl Instantiate for ProgressASG {
    fn instantiate(config: &Config, _: &CNFDescription) -> Self {
        ProgressASG {
            ema: Ema::new(config.restart_asg_len),
            asg: 0,
            threshold: config.restart_blocking,
            best: 0,
        }
    }
}

impl ProgressEvaluator for ProgressASG {
    type Input = usize;
    fn update(&mut self, n: usize) {
        self.asg = n;
        self.ema.update(n as f64);
        if self.best < n {
            self.best = n;
        }
    }
    fn get(&self) -> f64 {
        self.ema.get()
    }
    fn trend(&self) -> f64 {
        (self.asg as f64) / self.ema.get()
    }
    fn is_active(&self) -> bool {
        self.threshold * self.ema.get() < (self.asg as f64)
    }
}

#[derive(Debug)]
pub struct ProgressLBD {
    ema: Ema2,
    num: usize,
    sum: usize,
    /// For force restart based on average LBD of newly generated clauses: 0.80.
    /// This is called `K` in Glucose
    pub threshold: f64,
}

impl Instantiate for ProgressLBD {
    fn instantiate(config: &Config, _: &CNFDescription) -> Self {
        ProgressLBD {
            ema: Ema2::new(config.restart_lbd_len).with_slow(4 * config.restart_lbd_len),
            num: 0,
            sum: 0,
            threshold: config.restart_threshold,
        }
    }
}

impl ProgressEvaluator for ProgressLBD {
    type Input = usize;
    fn update(&mut self, d: usize) {
        self.num += 1;
        self.sum += d;
        self.ema.update(d as f64);
    }
    fn get(&self) -> f64 {
        self.ema.get()
    }
    fn trend(&self) -> f64 {
        self.ema.rate().max(self.ema.get() * (self.num as f64) / (self.sum as f64))
    }
    fn is_active(&self) -> bool {
        (self.sum as f64) < self.ema.get() * (self.num as f64) * self.threshold
    }
}

#[derive(Debug)]
pub struct LubySeries {
    pub active: bool,
    next_restart: usize,
    index: usize,
    restart_inc: f64,
    restart_step: usize,
}

impl Default for LubySeries {
    fn default() -> Self {
        LubySeries {
            active: false,
            next_restart: 0,
            index: 1,
            restart_inc: 2.0,
            restart_step: 10,
        }
    }
}

impl Instantiate for LubySeries {
    fn instantiate(config: &Config, _: &CNFDescription) -> Self {
        LubySeries {
            restart_step: config.restart_step,
            .. LubySeries::default()
        }
    }
}

impl fmt::Display for LubySeries {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.active {
            write!(f, "Luby[index:{}, step:{}]",
                   self.index,
                   self.next_restart,
            )
        } else {
            write!(f, "Luby(deactive)")
        }
    }
}

impl ProgressEvaluator for LubySeries {
    type Input = usize;
    fn update(&mut self, reset: usize) {
        assert!(self.active);
        if reset == 0 {
            self.index = 0;
        } else {
            self.index += 1;
        }
        self.next_restart = self.next_step();
    }
    fn get(&self) -> f64 {
        self.next_restart as f64
    }
    fn trend(&self) -> f64 {
        todo!()
    }
    fn is_active(&self) -> bool {
        todo!()
    }
}

/// Find the finite subsequence that contains index 'x', and the
/// size of that subsequence as: 1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8
impl LubySeries {
    fn next_step(&self) -> usize {
        if self.index == 0 {
            return self.restart_step;
        }
        let mut size: usize = 1;
        let mut seq: usize = 0;
        while size < self.index + 1 {
            seq += 1;
            size = 2 * size + 1;
        }
        let mut x = self.index;
        while size - 1 != x {
            size = (size - 1) >> 1;
            seq -= 1;
            x %= size;
        }
        (self.restart_inc.powf(seq as f64) * self.restart_step as f64) as usize
    }
}

#[test]
fn test_luby_series() {
    let mut luby = LubySeries {
        active: true,
        restart_step: 1,
        .. LubySeries::default()
    };
    luby.update(0);
    for v in vec![1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8] {
        assert_eq!(luby.next_restart, v);
        luby.update(1);
    }
}

// Restart stat
#[derive(Debug)]
pub struct RestartExecutor {
    pub adaptive_restart: bool,
    pub asg: ProgressASG,
    pub lbd: ProgressLBD,
    pub luby: LubySeries,
    pub after_restart: usize,
    pub cur_restart: usize,
    pub next_restart: usize,
    pub restart_step: usize,
}

impl Instantiate for RestartExecutor {
    fn instantiate(config: &Config, cnf: &CNFDescription) -> Self {
        RestartExecutor {
            adaptive_restart: !config.without_adaptive_restart,
            asg: ProgressASG::instantiate(config, cnf),
            lbd: ProgressLBD::instantiate(config, cnf),
            luby: LubySeries::instantiate(config, cnf),
            after_restart: 0,
            cur_restart: 1,
            next_restart: 100,
            restart_step: config.restart_step,
        }
    }
}

impl RestartIF for RestartExecutor {
    fn block_restart(&mut self) -> bool {
        if 100 < self.lbd.num
            && !self.luby.active
            && self.restart_step <= self.after_restart
            && self.asg.is_active()
        {
            self.after_restart = 0;
            return true;
        }
        false
    }
    fn force_restart(&mut self) -> bool {
        if self.luby.active {
            if self.luby.next_restart <= self.after_restart {
                self.luby.update(1);
                self.after_restart = 0;
                return true;
            }
        } else if self.restart_step <= self.after_restart && self.lbd.is_active() {
            self.after_restart = 0;
            return true;
        }
        false
    }
}
