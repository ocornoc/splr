use clause::{Clause, RANK_CONST, RANK_NEED};
use solver::{Solver, Stat};
use solver_propagate::SolveSAT;
use std::usize::MAX;
use types::*;

pub trait ClauseManagement {
    fn bump_ci(&mut self, ci: ClauseIndex) -> ();
    fn decay_cla_activity(&mut self) -> ();
    fn add_clause(&mut self, v: Vec<Lit>) -> bool;
    fn add_learnt(&mut self, v: Vec<Lit>) -> usize;
    fn reduce_database(&mut self) -> ();
    fn simplify_database(&mut self) -> ();
    fn lbd_of(&mut self, v: &[Lit]) -> usize;
}

impl ClauseManagement for Solver {
    fn bump_ci(&mut self, ci: ClauseIndex) -> () {
        debug_assert_ne!(ci, 0);
        let a = self.clauses[ci].activity + self.cla_inc;
        self.clauses[ci].activity = a;
        if 1.0e20 < a {
            for c in &mut self.clauses {
                if c.learnt {
                    c.activity *= 1.0e-20;
                }
            }
            self.cla_inc *= 1.0e-20;
        }
    }
    fn decay_cla_activity(&mut self) -> () {
        self.cla_inc = self.cla_inc / self.config.clause_decay_rate;
    }
    // renamed from clause_new
    fn add_clause(&mut self, mut v: Vec<Lit>) -> bool {
        v.sort_unstable();
        let mut j = 0;
        let mut l_ = NULL_LIT; // last literal; [x, x.negate()] means totology.
        for i in 0..v.len() {
            let li = v[i];
            let sat = self.assigned(li);
            if sat == LTRUE || li.negate() == l_ {
                return true;
            } else if sat != LFALSE && li != l_ {
                v[j] = li;
                j += 1;
                l_ = li;
            }
        }
        v.truncate(j);
        match v.len() {
            0 => true,
            1 => self.enqueue(v[0], NULL_CLAUSE),
            _ => {
                self.attach_clause(Clause::new(RANK_CONST, v));
                true
            }
        }
    }
    /// renamed from newLearntClause
    fn add_learnt(&mut self, v: Vec<Lit>) -> usize {
        let lbd;
        if v.len() == 2 {
            lbd = 0;
        } else {
            lbd = self.lbd_of(&v);
        }
        let mut c = Clause::new(RANK_NEED + lbd, v);
        let mut i_max = 0;
        let mut lv_max = 0;
        // seek a literal with max level
        for i in 0..c.lits.len() {
            let vi = c.lits[i].vi();
            let lv = self.vars[vi].level;
            if self.vars[vi].assign != BOTTOM && lv_max < lv {
                i_max = i;
                lv_max = lv;
            }
        }
        c.lits.swap(1, i_max);
        let l0 = c.lits[0];
        let ci = self.attach_clause(c);
        self.bump_ci(ci);
        self.uncheck_enqueue(l0, ci);
        lbd
    }
    fn reduce_database(&mut self) -> () {
        let start = self.fixed_len;
        debug_assert_ne!(start, 0);
        let nc = self.clauses.len();
        if self.clause_permutation.len() < nc {
            unsafe {
                self.clause_permutation.reserve(nc + 1);
                self.clause_permutation.set_len(nc + 1);
            }
        }
        // sort the range
        self.clauses[start..].sort();
        {
            let perm = &mut self.clause_permutation;
            for mut i in 0..nc {
                perm[self.clauses[i].index] = i;
            }
            let _ac = 0.1 * self.cla_inc / ((nc - start) as f64);
            let nkeep = start + (nc - start) / 2;
            // println!("ac {}, pased {}, index {}, locked {}, activity threshold {}",
            //          ac,
            //          self.clauses[start..].iter().filter(|c| c.locked || ac < c.activity).count(),
            //          self.clauses[start..].iter().filter(|c| perm[c.index] < nkeep).count(),
            //          self.clauses[start..].iter().filter(|c| c.locked).count(),
            //          self.clauses[start..].iter().filter(|c| ac < c.activity).count(),
            //          );
            self.clauses.retain(|c| perm[c.index] < nkeep || c.locked);
            // println!("start {}, end {}, nkeep {}, new len {}", start, nc, nkeep, self.clauses.len());
        }
        let new_len = self.clauses.len();
        // update permutation table.
        for i in 0..nc {
            self.clause_permutation[i] = 0;
        }
        for new in 0..new_len {
            let c = &mut self.clauses[new];
            self.clause_permutation[c.index] = new;
            c.index = new;
        }
        // rebuild reason
        for v in &mut self.vars[1..] {
            v.reason = self.clause_permutation[v.reason];
        }
        // rebuild watches
        let perm = &self.clause_permutation;
        for v in &mut self.watches {
            for w in &mut v[..] {
                w.by = perm[w.by];
            }
        }
        self.stats[Stat::NumOfReduction as usize] += 1;
        println!(
            "# DB::drop 1/2 {:>9}({:>8}) => {:>9}   Restart:: block {:>4} force {:>4}",
            nc,
            self.fixed_len,
            new_len,
            self.stats[Stat::NumOfBlockRestart as usize],
            self.stats[Stat::NumOfRestart as usize],
        );
    }
    fn simplify_database(&mut self) -> () {
        debug_assert_eq!(self.decision_level(), 0);
        let end = self.clauses.len();
        // remove clauses containing new fixed literals
        let targets: Vec<Lit> = self.trail[self.num_solved_vars..]
            .iter()
            .map(|l| l.negate())
            .collect();
        for mut c in &mut self.clauses {
            c.lits.retain(|l| {
                for t in &targets {
                    if t == l {
                        return false;
                    }
                }
                true
            });
        }
        let nc = self.clauses.len();
        let mut purges = 0;
        if self.clause_permutation.len() < nc {
            unsafe {
                self.clause_permutation.reserve(nc + 1);
                self.clause_permutation.set_len(nc + 1);
            }
        }
        // reinitialize the permutation table.
        for x in &mut self.clause_permutation {
            *x = 0;
        }
        // set key
        for ci in 1..self.clauses.len() {
            unsafe {
                let c = &mut self.clauses[ci] as *mut Clause;
                if self.satisfies(&self.clauses[ci]) {
                    (*c).index = MAX;
                    purges += 1;
                } else if (*c).lits.len() == 1 {
                    if !self.enqueue((*c).lits[0], NULL_CLAUSE) {
                        self.ok = false;
                    }
                    (*c).index = MAX;
                } else {
                    if RANK_NEED < (*c).rank {
                        let new = self.lbd_of(&(*c).lits);
                        if new < (*c).rank {
                            (*c).rank = new;
                        }
                    }
                }
            }
        }
        self.clauses.retain(|ref c| c.index < MAX);
        let new_end = self.clauses.len();
        debug_assert_eq!(new_end, nc - purges);
        for i in 1..new_end {
            let old = self.clauses[i].index;
            debug_assert!(0 < old, "0 index");
            self.clause_permutation[old] = i;
            self.clauses[i].index = i;
        }
        // clear the reasons of variables satisfied at level zero.
        for l in &self.trail {
            self.vars[l.vi() as usize].reason = NULL_CLAUSE;
        }
        let mut c0 = 0;
        for c in &self.clauses[..] {
            if c.rank <= RANK_NEED {
                c0 += 1;
            } else {
                break;
            }
        }
        if new_end == end {
            return;
        }
        self.fixed_len = c0;
        self.clauses.truncate(new_end);
        // rebuild watches
        let (w0, wr) = self.watches.split_first_mut().unwrap();
        w0.clear();
        for ws in wr {
            while let Some(mut w) = ws.pop() {
                match self.clause_permutation[w.by] {
                    0 => {}
                    x => {
                        w.by = x;
                        w0.push(w);
                    }
                }
            }
            while let Some(w) = w0.pop() {
                ws.push(w);
            }
        }
        println!(
            "# DB::simplify {:>9}({:>8}) => {:>9}   Restart:: block {:>4} force {:>4}",
            nc,
            self.fixed_len,
            self.clauses.len(),
            self.stats[Stat::NumOfBlockRestart as usize],
            self.stats[Stat::NumOfRestart as usize],
        );
    }
    fn lbd_of(&mut self, v: &[Lit]) -> usize {
        if v.len() == 2 {
            return RANK_NEED;
        }
        let key;
        let key_old = self.lbd_seen[0];
        if 10_000_000 < key_old {
            key = 1;
        } else {
            key = key_old + 1;
        }
        self.lbd_seen[0] = key;
        let mut cnt = 0;
        for l in v {
            let lv = self.vars[l.vi()].level;
            if self.lbd_seen[lv] != key && lv != 0 {
                self.lbd_seen[lv] = key;
                cnt += 1;
            }
        }
        if cnt == 0 { 1 } else { cnt }
    }
}
