use crate::clause::{Clause, ClauseDB};
use crate::propagator::AssignStack;
use crate::state::State;
use crate::traits::*;
use crate::types::*;
use crate::var::{Var, VarDB};
use std::fmt;

#[derive(Eq, Debug, PartialEq)]
enum EliminatorMode {
    Deactive,
    Waiting,
    Running,
}

/// Literal eliminator
#[derive(Debug)]
pub struct Eliminator {
    mode: EliminatorMode,
    clause_queue: Vec<ClauseId>,
    var_queue: VarOccHeap,
    bwdsub_assigns: usize,
    elim_clauses: Vec<Lit>,
}

impl Default for Eliminator {
    fn default() -> Eliminator {
        Eliminator {
            mode: EliminatorMode::Deactive,
            var_queue: VarOccHeap::new(0, 0),
            clause_queue: Vec::new(),
            bwdsub_assigns: 0,
            elim_clauses: Vec::new(),
        }
    }
}

impl EliminatorIF for Eliminator {
    fn new(nv: usize) -> Eliminator {
        let mut e = Eliminator::default();
        e.var_queue = VarOccHeap::new(nv, 0);
        e
    }
    fn activate(&mut self) {
        debug_assert!(self.mode != EliminatorMode::Running);
        self.mode = EliminatorMode::Waiting;
    }
    fn is_running(&self) -> bool {
        self.mode == EliminatorMode::Running
    }
    fn is_waiting(&self) -> bool {
        self.mode == EliminatorMode::Waiting
    }
    // Due to a potential bug of killing clauses and difficulty about
    // synchronization between 'garbage_collect' and clearing occur lists,
    // 'stop' should purge all occur lists to purge any dead clauses for now.
    fn stop(&mut self, cdb: &mut ClauseDB, vdb: &mut VarDB) {
        let force: bool = true;
        self.clear_clause_queue(cdb);
        self.clear_var_queue(vdb);
        if force {
            for c in &mut cdb.clause[1..] {
                c.turn_off(Flag::OCCUR_LINKED);
            }
            for v in &mut vdb.vars[1..] {
                v.pos_occurs.clear();
                v.neg_occurs.clear();
            }
        }
        self.mode = EliminatorMode::Deactive;
    }
    fn prepare(&mut self, cdb: &mut ClauseDB, vdb: &mut VarDB, force: bool) {
        if self.mode != EliminatorMode::Waiting {
            return;
        }
        self.mode = EliminatorMode::Running;
        for v in &mut vdb.vars[1..] {
            v.pos_occurs.clear();
            v.neg_occurs.clear();
        }
        for (cid, c) in &mut cdb.clause.iter_mut().enumerate().skip(1) {
            if c.is(Flag::DEAD) || c.is(Flag::OCCUR_LINKED) {
                continue;
            }
            self.add_cid_occur(vdb, cid as ClauseId, c, false);
        }
        if force {
            for vi in 1..vdb.vars.len() {
                let v = &vdb.vars[vi];
                if v.is(Flag::ELIMINATED) || v.assign != BOTTOM {
                    continue;
                }
                self.enqueue_var(vdb, vi, true);
            }
        }
    }
    fn enqueue_clause(&mut self, cid: ClauseId, c: &mut Clause) {
        if self.mode != EliminatorMode::Running || c.is(Flag::ENQUEUED) {
            return;
        }
        self.clause_queue.push(cid);
        c.turn_on(Flag::ENQUEUED);
    }
    fn clear_clause_queue(&mut self, cdb: &mut ClauseDB) {
        for cid in &self.clause_queue {
            cdb.clause[*cid as usize].turn_off(Flag::ENQUEUED);
        }
        self.clause_queue.clear();
    }
    fn enqueue_var(&mut self, vdb: &mut VarDB, vi: VarId, upward: bool) {
        if self.mode != EliminatorMode::Running {
            return;
        }
        self.var_queue.insert(vdb, vi, upward);
        vdb.vars[vi].turn_on(Flag::ENQUEUED);
    }
    fn clear_var_queue(&mut self, vdb: &mut VarDB) {
        self.var_queue.clear(vdb);
    }
    fn clause_queue_len(&self) -> usize {
        self.clause_queue.len()
    }
    fn var_queue_len(&self) -> usize {
        self.var_queue.len()
    }
    fn eliminate(
        &mut self,
        asgs: &mut AssignStack,
        cdb: &mut ClauseDB,
        state: &mut State,
        vdb: &mut VarDB,
    ) -> MaybeInconsistent {
        debug_assert!(asgs.level() == 0);
        if self.mode == EliminatorMode::Deactive {
            return Ok(());
        }
        let mut cnt = 0;
        while self.bwdsub_assigns < asgs.len()
            || !self.var_queue.is_empty()
            || !self.clause_queue.is_empty()
        {
            if !self.clause_queue.is_empty() || self.bwdsub_assigns < asgs.len() {
                self.backward_subsumption_check(asgs, cdb, state, vdb)?;
            }
            while let Some(vi) = self.var_queue.select_var(vdb) {
                let v = &mut vdb.vars[vi];
                v.turn_off(Flag::ENQUEUED);
                cnt += 1;
                if cnt < state.elim_eliminate_loop_limit
                    && !v.is(Flag::ELIMINATED)
                    && v.assign == BOTTOM
                {
                    eliminate_var(asgs, cdb, self, state, vdb, vi)?;
                }
            }
            self.backward_subsumption_check(asgs, cdb, state, vdb)?;
            debug_assert!(self.clause_queue.is_empty());
            cdb.garbage_collect();
            if asgs.propagate(cdb, state, vdb) != NULL_CLAUSE {
                return Err(SolverError::Inconsistent);
            }
            cdb.eliminate_satisfied_clauses(self, vdb, true);
            cdb.garbage_collect();
        }
        Ok(())
    }
    fn extend_model(&mut self, model: &mut Vec<i32>) {
        if self.elim_clauses.is_empty() {
            return;
        }
        let mut i = self.elim_clauses.len() - 1;
        let mut width;
        'next: loop {
            width = self.elim_clauses[i] as usize;
            if width == 0 && i == 0 {
                break;
            }
            i -= 1;
            loop {
                if width <= 1 {
                    break;
                }
                let l = self.elim_clauses[i];
                let model_value = match model[l.vi() - 1] {
                    x if x == l.to_i32() => TRUE,
                    x if -x == l.to_i32() => FALSE,
                    _ => BOTTOM,
                };
                if model_value != FALSE {
                    if i < width {
                        break 'next;
                    }
                    i -= width;
                    continue 'next;
                }
                width -= 1;
                i -= 1;
            }
            debug_assert!(width == 1);
            let l = self.elim_clauses[i];
            // debug_assert!(model[l.vi() - 1] != l.negate().int());
            model[l.vi() - 1] = l.to_i32(); // .neg();
            if i < width {
                break;
            }
            i -= width;
        }
    }
    fn add_cid_occur(&mut self, vdb: &mut VarDB, cid: ClauseId, c: &mut Clause, enqueue: bool) {
        if self.mode != EliminatorMode::Running || c.is(Flag::OCCUR_LINKED) {
            return;
        }
        for l in &c.lits {
            let v = &mut vdb.vars[l.vi()];
            v.turn_on(Flag::TOUCHED);
            if !v.is(Flag::ELIMINATED) {
                if l.is_positive() {
                    debug_assert!(
                        !v.pos_occurs.contains(&cid),
                        format!("{} {:?} {}", cid.format(), vec2int(&c.lits), v.index,)
                    );
                    v.pos_occurs.push(cid);
                } else {
                    debug_assert!(
                        !v.neg_occurs.contains(&cid),
                        format!("{} {:?} {}", cid.format(), vec2int(&c.lits), v.index,)
                    );
                    v.neg_occurs.push(cid);
                }
                self.enqueue_var(vdb, l.vi(), false);
            }
        }
        c.turn_on(Flag::OCCUR_LINKED);
        if enqueue {
            self.enqueue_clause(cid, c);
        }
    }
    fn remove_lit_occur(&mut self, vdb: &mut VarDB, l: Lit, cid: ClauseId) {
        let v = &mut vdb.vars[l.vi()];
        if l.is_positive() {
            debug_assert_eq!(v.pos_occurs.iter().filter(|&c| *c == cid).count(), 1);
            v.pos_occurs.delete_unstable(|&c| c == cid);
            debug_assert!(!v.pos_occurs.contains(&cid));
        } else {
            debug_assert_eq!(v.neg_occurs.iter().filter(|&c| *c == cid).count(), 1);
            v.neg_occurs.delete_unstable(|&c| c == cid);
            debug_assert!(!v.neg_occurs.contains(&cid));
        }
        self.enqueue_var(vdb, l.vi(), true);
    }
    fn remove_cid_occur(&mut self, vdb: &mut VarDB, cid: ClauseId, c: &mut Clause) {
        debug_assert!(self.mode == EliminatorMode::Running);
        debug_assert!(!cid.is_lifted_lit());
        c.turn_off(Flag::OCCUR_LINKED);
        debug_assert!(c.is(Flag::DEAD));
        for l in &c.lits {
            if vdb.vars[l.vi()].assign == BOTTOM {
                self.remove_lit_occur(vdb, *l, cid);
                if !vdb.vars[l.vi()].is(Flag::ELIMINATED) {
                    self.enqueue_var(vdb, l.vi(), true);
                }
            }
        }
    }
}

impl Eliminator {
    /// returns false if solver is inconsistent
    /// - calls `clause_queue.pop`
    fn backward_subsumption_check(
        &mut self,
        asgs: &mut AssignStack,
        cdb: &mut ClauseDB,
        state: &State,
        vdb: &mut VarDB,
    ) -> MaybeInconsistent {
        let mut cnt = 0;
        debug_assert_eq!(asgs.level(), 0);
        while !self.clause_queue.is_empty() || self.bwdsub_assigns < asgs.len() {
            // Check top-level assignments by creating a dummy clause and placing it in the queue:
            if self.clause_queue.is_empty() && self.bwdsub_assigns < asgs.len() {
                let c = asgs.trail[self.bwdsub_assigns].to_cid();
                self.clause_queue.push(c);
                self.bwdsub_assigns += 1;
            }
            let cid = match self.clause_queue.pop() {
                Some(x) => x,
                None => 0,
            };
            // assert_ne!(cid, 0);
            cnt += 1;
            if state.elim_subsume_loop_limit < cnt {
                continue;
            }
            let best = if cid.is_lifted_lit() {
                cid.to_lit().vi()
            } else {
                let mut tmp = cdb.count(true);
                let c = &mut cdb.clause[cid as usize];
                c.turn_off(Flag::ENQUEUED);
                let lits = &c.lits;
                if c.is(Flag::DEAD) || state.elim_subsume_literal_limit < lits.len() {
                    continue;
                }
                // if c is subsumed by c', both of c and c' are included in the occurs of all literals of c
                // so searching the shortest occurs is most efficient.
                let mut b = 0;
                for l in lits {
                    let v = &vdb.vars[l.vi()];
                    if v.assign != BOTTOM {
                        continue;
                    }
                    let nsum = if l.is_positive() {
                        v.neg_occurs.len()
                    } else {
                        v.pos_occurs.len()
                    };
                    if !v.is(Flag::ELIMINATED) && nsum < tmp {
                        b = l.vi();
                        tmp = nsum;
                    }
                }
                b
            };
            if best == 0 || vdb.vars[best].is(Flag::ELIMINATED) {
                continue;
            }
            unsafe {
                for cs in &[
                    &mut vdb.vars[best].pos_occurs as *mut Vec<ClauseId>,
                    &mut vdb.vars[best].neg_occurs as *mut Vec<ClauseId>,
                ] {
                    for did in &**cs {
                        if *did == cid {
                            continue;
                        }
                        let db = &cdb.clause[*did as usize];
                        if !db.is(Flag::DEAD) && db.lits.len() <= state.elim_subsume_literal_limit {
                            try_subsume(asgs, cdb, self, vdb, cid, *did)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn try_subsume(
    asgs: &mut AssignStack,
    cdb: &mut ClauseDB,
    elim: &mut Eliminator,
    vdb: &mut VarDB,
    cid: ClauseId,
    did: ClauseId,
) -> MaybeInconsistent {
    match subsume(cdb, cid, did) {
        Some(NULL_LIT) => {
            // println!("BackSubsC    => {} {:#} subsumed completely by {} {:#}",
            //          did.fmt(),
            //          *clause!(cdb, cid),
            //          cid.fmt(),
            //          *clause!(cdb, cid),
            // );
            cdb.detach(did);
            elim.remove_cid_occur(vdb, did, &mut cdb.clause[did as usize]);
            if !cdb.clause[did as usize].is(Flag::LEARNT) {
                cdb.clause[cid as usize].turn_off(Flag::LEARNT);
            }
        }
        Some(l) => {
            // println!("BackSubC subsumes {} from {} and {}", l.int(), cid.format(), did.format());
            strengthen_clause(cdb, elim, vdb, asgs, did, l.negate())?;
            elim.enqueue_var(vdb, l.vi(), true);
        }
        None => {}
    }
    Ok(())
}

/// returns a literal if these clauses can be merged by the literal.
fn subsume(cdb: &mut ClauseDB, cid: ClauseId, other: ClauseId) -> Option<Lit> {
    debug_assert!(!other.is_lifted_lit());
    if cid.is_lifted_lit() {
        let l = cid.to_lit();
        let oh = &cdb.clause[other as usize];
        for lo in &oh.lits {
            if l == lo.negate() {
                return Some(l);
            }
        }
        return None;
    }
    let mut ret: Lit = NULL_LIT;
    let ch = &cdb.clause[cid as usize];
    let ob = &cdb.clause[other as usize];
    debug_assert!(ob.lits.contains(&ob.lits[0]));
    debug_assert!(ob.lits.contains(&ob.lits[1]));
    'next: for l in &ch.lits {
        for lo in &ob.lits {
            if *l == *lo {
                continue 'next;
            } else if ret == NULL_LIT && *l == lo.negate() {
                ret = *l;
                continue 'next;
            }
        }
        return None;
    }
    Some(ret)
}

/// Returns:
/// - `(false, -)` if one of the clauses is always satisfied.
/// - `(true, n)` if they are mergeable to a n-literal clause.
fn check_to_merge(
    cdb: &ClauseDB,
    vdb: &VarDB,
    cp: ClauseId,
    cq: ClauseId,
    v: VarId,
) -> (bool, usize) {
    let pqb = &cdb.clause[cp as usize];
    let qpb = &cdb.clause[cq as usize];
    let ps_smallest = pqb.lits.len() < qpb.lits.len();
    let (pb, qb) = if ps_smallest { (pqb, qpb) } else { (qpb, pqb) };
    let mut size = pb.lits.len() + 1;
    'next_literal: for l in &qb.lits {
        if vdb.vars[l.vi()].is(Flag::ELIMINATED) {
            continue;
        }
        if l.vi() != v {
            for j in &pb.lits {
                if vdb.vars[j.vi()].is(Flag::ELIMINATED) {
                    continue;
                }
                if j.vi() == l.vi() {
                    if j.negate() == *l {
                        return (false, size);
                    } else {
                        continue 'next_literal;
                    }
                }
            }
            size += 1;
        }
    }
    (true, size)
}

#[allow(dead_code)]
fn check_eliminator(cdb: &ClauseDB, vars: &[Var]) -> bool {
    // clause_queue should be clear.
    // all elements in occur_lists exist.
    // for v in vars {
    //     for ci in &v.pos_occurs {
    //         let c = clause!(cp, ci);
    //         if c.lits[0] < 2 || c.lits[1] < 2 {
    //             panic!("panic {:#}", c);
    //         }
    //     }
    //     for ci in &v.neg_occurs {
    //         let c = clause!(cp, ci);
    //         if c.lits[0] < 2 || c.lits[1] < 2 {
    //             panic!("panic {:#}", c);
    //         }
    //     }
    // }
    // all clauses are registered in corresponding occur_lists
    for (cid, c) in cdb.clause.iter().enumerate().skip(1) {
        if c.is(Flag::DEAD) {
            continue;
        }
        for l in &c.lits {
            let v = l.vi();
            if l.is_positive() {
                if !vars[v].pos_occurs.contains(&(cid as ClauseId)) {
                    panic!("failed to check {} {:#}", (cid as ClauseId).format(), c);
                }
            } else if !vars[v].neg_occurs.contains(&(cid as ClauseId)) {
                panic!("failed to check {} {:#}", (cid as ClauseId).format(), c);
            }
        }
    }
    true
}

/// Returns **false** if one of the clauses is always satisfied. (merge_vec should not be used.)
fn merge(cdb: &mut ClauseDB, cip: ClauseId, ciq: ClauseId, v: VarId, vec: &mut Vec<Lit>) -> usize {
    vec.clear();
    let pqb = &cdb.clause[cip as usize];
    let qpb = &cdb.clause[ciq as usize];
    let ps_smallest = pqb.lits.len() < qpb.lits.len();
    let (pb, qb) = if ps_smallest { (pqb, qpb) } else { (qpb, pqb) };
    // println!(" -  {:?}{:?} & {:?}{:?}", vec2int(&ph.lit),vec2int(&pb.lits),vec2int(&qh.lit),vec2int(&qb.lits));
    'next_literal: for l in &qb.lits {
        if l.vi() != v {
            for j in &pb.lits {
                if j.vi() == l.vi() {
                    if j.negate() == *l {
                        return 0;
                    } else {
                        continue 'next_literal;
                    }
                }
            }
            vec.push(*l);
        }
    }
    for l in &pb.lits {
        if l.vi() != v {
            vec.push(*l);
        }
    }
    // println!("merge generated {:?} from {} and {} to eliminate {}", vec2int(vec.clone()), p, q, v);
    vec.len()
}

/// removes `l` from clause `cid`
/// - calls `enqueue_clause`
/// - calls `enqueue_var`
fn strengthen_clause(
    cdb: &mut ClauseDB,
    elim: &mut Eliminator,
    vdb: &mut VarDB,
    asgs: &mut AssignStack,
    cid: ClauseId,
    l: Lit,
) -> MaybeInconsistent {
    debug_assert!(!cdb.clause[cid as usize].is(Flag::DEAD));
    debug_assert!(1 < cdb.clause[cid as usize].lits.len());
    cdb.touched[l as usize] = true;
    cdb.touched[l.negate() as usize] = true;
    debug_assert_ne!(cid, NULL_CLAUSE);
    if strengthen(cdb, cid, l) {
        // Vaporize the binary clause
        debug_assert!(2 == cdb.clause[cid as usize].lits.len());
        let c0 = cdb.clause[cid as usize].lits[0];
        debug_assert_ne!(c0, l);
        // println!("{} {:?} is removed and its first literal {} is enqueued.", cid.format(), vec2int(&cdb.clause[cid].lits), c0.int());
        cdb.detach(cid);
        elim.remove_cid_occur(vdb, cid, &mut cdb.clause[cid as usize]);
        asgs.enqueue(vdb, c0.vi(), c0.lbool(), NULL_CLAUSE, 0)
    } else {
        // println!("cid {} drops literal {}", cid.fmt(), l.int());
        debug_assert!(1 < cdb.clause[cid as usize].lits.len());
        elim.enqueue_clause(cid, &mut cdb.clause[cid as usize]);
        elim.remove_lit_occur(vdb, l, cid);
        unsafe {
            let vec = &cdb.clause[cid as usize].lits[..] as *const [Lit];
            cdb.certificate_add(&*vec);
        }
        Ok(())
    }
}

/// removes Lit `p` from Clause *self*. This is an O(n) function!
/// returns true if the clause became a unit clause.
/// Called only from strengthen_clause
fn strengthen(cdb: &mut ClauseDB, cid: ClauseId, p: Lit) -> bool {
    debug_assert!(!cdb.clause[cid as usize].is(Flag::DEAD));
    debug_assert!(1 < cdb.clause[cid as usize].lits.len());
    let ClauseDB {
        ref mut clause,
        ref mut watcher,
        ..
    } = cdb;
    let c = &mut clause[cid as usize];
    // debug_assert!((*ch).lits.contains(&p));
    // debug_assert!(1 < (*ch).lits.len());
    if (*c).is(Flag::DEAD) {
        return false;
    }
    debug_assert!(1 < p.negate());
    let lits = &mut (*c).lits;
    debug_assert!(1 < lits.len());
    if lits.len() == 2 {
        if lits[0] == p {
            lits.swap(0, 1);
        }
        debug_assert!(1 < lits[0].negate());
        return true;
    }
    if lits[0] == p || lits[1] == p {
        let (q, r) = if lits[0] == p {
            lits.swap_remove(0);
            (lits[0], lits[1])
        } else {
            lits.swap_remove(1);
            (lits[1], lits[0])
        };
        debug_assert!(1 < p.negate());
        watcher[p.negate() as usize].detach_with(cid);
        watcher[q.negate() as usize].register(r, cid);
        if lits.len() == 2 {
            // update another bocker
            watcher[r.negate() as usize].update_blocker(cid, q);
        }
    } else {
        lits.delete_unstable(|&x| x == p);
        if lits.len() == 2 {
            // update another bocker
            let q = lits[0];
            let r = lits[1];
            watcher[q.negate() as usize].update_blocker(cid, r);
            watcher[r.negate() as usize].update_blocker(cid, q);
        }
    }
    false
}

fn make_eliminating_unit_clause(vec: &mut Vec<Lit>, x: Lit) {
    vec.push(x);
    vec.push(1);
}

fn make_eliminated_clause(cdb: &mut ClauseDB, vec: &mut Vec<Lit>, vi: VarId, cid: ClauseId) {
    let first = vec.len();
    // Copy clause to the vector. Remember the position where the variable 'v' occurs:
    let c = &cdb.clause[cid as usize];
    debug_assert!(!c.lits.is_empty());
    for l in &c.lits {
        vec.push(*l as Lit);
        if l.vi() == vi {
            let index = vec.len() - 1;
            debug_assert_eq!(vec[index], *l);
            debug_assert_eq!(vec[index].vi(), vi);
            // swap the first literal with the 'v'. So that the literal containing 'v' will occur first in the clause.
            vec.swap(index, first);
        }
    }
    // Store the length of the clause last:
    debug_assert_eq!(vec[first].vi(), vi);
    vec.push(c.lits.len() as Lit);
    cdb.touched[Lit::from_var(vi, TRUE) as usize] = true;
    cdb.touched[Lit::from_var(vi, FALSE) as usize] = true;
    // println!("make_eliminated_clause: eliminate({}) clause {:?}", vi, vec2int(&ch.lits));
}

fn eliminate_var(
    asgs: &mut AssignStack,
    cdb: &mut ClauseDB,
    elim: &mut Eliminator,
    state: &mut State,
    vdb: &mut VarDB,
    vi: VarId,
) -> MaybeInconsistent {
    let v = &mut vdb.vars[vi];
    if v.assign != BOTTOM {
        return Ok(());
    }
    debug_assert!(!v.is(Flag::ELIMINATED));
    // count only alive clauses
    v.pos_occurs
        .retain(|&c| !cdb.clause[c as usize].is(Flag::DEAD));
    v.neg_occurs
        .retain(|&c| !cdb.clause[c as usize].is(Flag::DEAD));
    let pos = &v.pos_occurs as *const Vec<ClauseId>;
    let neg = &v.neg_occurs as *const Vec<ClauseId>;
    unsafe {
        if check_var_elimination_condition(cdb, state, vdb, &*pos, &*neg, vi) {
            return Ok(());
        }
        // OK, eliminate the literal and build constraints on it.
        state.num_eliminated_vars += 1;
        make_eliminated_clauses(cdb, elim, vi, &*pos, &*neg);
        let vec = &mut state.new_learnt as *mut Vec<Lit>;
        // Produce clauses in cross product:
        for p in &*pos {
            let rank_p = cdb.clause[*p as usize].rank;
            for n in &*neg {
                // println!("eliminator replaces {} with a cross product {:?}", p.fmt(), vec2int(&vec));
                match merge(cdb, *p, *n, vi, &mut *vec) {
                    0 => (),
                    1 => {
                        // println!(
                        //     "eliminate_var: grounds {} from {}{:?} and {}{:?}",
                        //     vec[0].int(),
                        //     p.fmt(),
                        //     vec2int(&clause!(*cp, *p).lits),
                        //     n.fmt(),
                        //     vec2int(&clause!(*cp, *n).lits)
                        // );
                        let lit = (*vec)[0];
                        cdb.certificate_add(&*vec);
                        asgs.enqueue(vdb, lit.vi(), lit.lbool(), NULL_CLAUSE, 0)?;
                    }
                    _ => {
                        let rank = if cdb.clause[*p as usize].is(Flag::LEARNT)
                            && cdb.clause[*n as usize].is(Flag::LEARNT)
                        {
                            rank_p.min(cdb.clause[*n as usize].rank)
                        } else {
                            0
                        };
                        let cid = cdb.attach(state, vdb, rank);
                        elim.add_cid_occur(vdb, cid, &mut cdb.clause[cid as usize], true);
                    }
                }
            }
        }
        for cid in &*pos {
            cdb.detach(*cid);
            elim.remove_cid_occur(vdb, *cid, &mut cdb.clause[*cid as usize]);
        }
        for cid in &*neg {
            cdb.detach(*cid);
            elim.remove_cid_occur(vdb, *cid, &mut cdb.clause[*cid as usize]);
        }
        vdb.vars[vi].pos_occurs.clear();
        vdb.vars[vi].neg_occurs.clear();
        vdb.vars[vi].turn_on(Flag::ELIMINATED);
        elim.backward_subsumption_check(asgs, cdb, state, vdb)
    }
}

/// returns `true` if elimination is impossible.
fn check_var_elimination_condition(
    cdb: &ClauseDB,
    state: &State,
    vdb: &VarDB,
    pos: &[ClauseId],
    neg: &[ClauseId],
    v: VarId,
) -> bool {
    // avoid thrashing
    if 0 < state.cdb_soft_limit && state.cdb_soft_limit < cdb.count(true) {
        return true;
    }
    let limit = if 0 < state.cdb_soft_limit && 3 * state.cdb_soft_limit < 4 * cdb.count(true) {
        state.elim_eliminate_grow_limit / 4
    } else {
        state.elim_eliminate_grow_limit
    };
    let clslen = pos.len() + neg.len();
    let mut cnt = 0;
    for c_pos in pos {
        for c_neg in neg {
            let (res, clause_size) = check_to_merge(cdb, vdb, *c_pos, *c_neg, v);
            if res {
                cnt += 1;
                if clslen + limit < cnt
                    || (state.elim_eliminate_combination_limit != 0
                        && state.elim_eliminate_combination_limit < clause_size)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn make_eliminated_clauses(
    cdb: &mut ClauseDB,
    elim: &mut Eliminator,
    v: VarId,
    pos: &[ClauseId],
    neg: &[ClauseId],
) {
    let tmp = &mut elim.elim_clauses;
    if neg.len() < pos.len() {
        for cid in neg {
            debug_assert!(!cdb.clause[*cid as usize].is(Flag::DEAD));
            make_eliminated_clause(cdb, tmp, v, *cid);
        }
        make_eliminating_unit_clause(tmp, Lit::from_var(v, TRUE));
    } else {
        for cid in pos {
            debug_assert!(!cdb.clause[*cid as usize].is(Flag::DEAD));
            make_eliminated_clause(cdb, tmp, v, *cid);
        }
        make_eliminating_unit_clause(tmp, Lit::from_var(v, FALSE));
    }
}

impl Var {
    fn occur_activity(&self) -> usize {
        self.pos_occurs.len().min(self.neg_occurs.len())
    }
}

/// Var heap structure based on the number of occurrences
// # Note
// - both fields has a fixed length. Don't use push and pop.
// - `idxs[0]` contains the number of alive elements
//   `indx` is positions. So the unused field 0 can hold the last position as a special case.
#[derive(Debug)]
pub struct VarOccHeap {
    heap: Vec<VarId>, // order : usize -> VarId
    idxs: Vec<usize>, // VarId : -> order : usize
}

trait VarOrderIF {
    fn new(n: usize, init: usize) -> VarOccHeap;
    fn insert(&mut self, vdb: &VarDB, vi: VarId, upword: bool);
    fn clear(&mut self, vdb: &mut VarDB);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn select_var(&mut self, vdb: &VarDB) -> Option<VarId>;
    fn rebuild(&mut self, vdb: &VarDB);
}

impl VarOrderIF for VarOccHeap {
    fn new(n: usize, init: usize) -> VarOccHeap {
        let mut heap = Vec::with_capacity(n + 1);
        let mut idxs = Vec::with_capacity(n + 1);
        heap.push(0);
        idxs.push(n);
        for i in 1..=n {
            heap.push(i);
            idxs.push(i);
        }
        idxs[0] = init;
        VarOccHeap { heap, idxs }
    }
    fn insert(&mut self, vdb: &VarDB, vi: VarId, upward: bool) {
        debug_assert!(vi < self.heap.len());
        if self.contains(vi) {
            let i = self.idxs[vi];
            if upward {
                self.percolate_up(&vdb.vars, i);
            } else {
                self.percolate_down(&vdb.vars, i);
            }
            return;
        }
        let i = self.idxs[vi];
        let n = self.idxs[0] + 1;
        let vn = self.heap[n];
        self.heap.swap(i, n);
        self.idxs.swap(vi, vn);
        debug_assert!(n < self.heap.len());
        self.idxs[0] = n;
        self.percolate_up(&vdb.vars, n);
    }
    fn clear(&mut self, vdb: &mut VarDB) {
        for v in &mut self.heap[0..self.idxs[0]] {
            vdb.vars[*v].turn_off(Flag::ENQUEUED);
        }
        self.reset()
    }
    fn len(&self) -> usize {
        self.idxs[0]
    }
    fn is_empty(&self) -> bool {
        self.idxs[0] == 0
    }
    fn select_var(&mut self, vdb: &VarDB) -> Option<VarId> {
        loop {
            let vi = self.get_root(&vdb.vars);
            if vi == 0 {
                return None;
            }
            if !vdb.vars[vi].is(Flag::ELIMINATED) {
                return Some(vi);
            }
        }
    }
    fn rebuild(&mut self, vdb: &VarDB) {
        self.reset();
        for v in &vdb.vars[1..] {
            if v.assign == BOTTOM && !v.is(Flag::ELIMINATED) {
                self.insert(vdb, v.index, true);
            }
        }
    }
}

impl VarOccHeap {
    fn contains(&self, v: VarId) -> bool {
        self.idxs[v] <= self.idxs[0]
    }
    fn reset(&mut self) {
        for i in 0..self.idxs.len() {
            self.idxs[i] = i;
            self.heap[i] = i;
        }
    }
    fn get_root(&mut self, vars: &[Var]) -> VarId {
        let s = 1;
        let vs = self.heap[s];
        let n = self.idxs[0];
        debug_assert!(n < self.heap.len());
        if n == 0 {
            return 0;
        }
        let vn = self.heap[n];
        debug_assert!(vn != 0, "Invalid VarId for heap");
        debug_assert!(vs != 0, "Invalid VarId for heap");
        self.heap.swap(n, s);
        self.idxs.swap(vn, vs);
        self.idxs[0] -= 1;
        if 1 < self.idxs[0] {
            self.percolate_down(&vars, 1);
        }
        vs
    }
    fn percolate_up(&mut self, vars: &[Var], start: usize) {
        let mut q = start;
        let vq = self.heap[q];
        debug_assert!(0 < vq, "size of heap is too small");
        let aq = vars[vq].occur_activity();
        loop {
            let p = q / 2;
            if p == 0 {
                self.heap[q] = vq;
                debug_assert!(vq != 0, "Invalid index in percolate_up");
                self.idxs[vq] = q;
                return;
            } else {
                let vp = self.heap[p];
                let ap = vars[vp].occur_activity();
                if ap > aq {
                    // move down the current parent, and make it empty
                    self.heap[q] = vp;
                    debug_assert!(vq != 0, "Invalid index in percolate_up");
                    self.idxs[vp] = q;
                    q = p;
                } else {
                    self.heap[q] = vq;
                    debug_assert!(vq != 0, "Invalid index in percolate_up");
                    self.idxs[vq] = q;
                    return;
                }
            }
        }
    }
    fn percolate_down(&mut self, vars: &[Var], start: usize) {
        let n = self.len();
        let mut i = start;
        let vi = self.heap[i];
        let ai = vars[vi].occur_activity();
        loop {
            let l = 2 * i; // left
            if l < n {
                let vl = self.heap[l];
                let al = vars[vl].occur_activity();
                let r = l + 1; // right
                let (target, vc, ac) = if r < n && al > vars[self.heap[r]].occur_activity() {
                    let vr = self.heap[r];
                    (r, vr, vars[vr].occur_activity())
                } else {
                    (l, vl, al)
                };
                if ai > ac {
                    self.heap[i] = vc;
                    self.idxs[vc] = i;
                    i = target;
                } else {
                    self.heap[i] = vi;
                    debug_assert!(vi != 0, "invalid index");
                    self.idxs[vi] = i;
                    return;
                }
            } else {
                self.heap[i] = vi;
                debug_assert!(vi != 0, "invalid index");
                self.idxs[vi] = i;
                return;
            }
        }
    }
    #[allow(dead_code)]
    fn peek(&self) -> VarId {
        self.heap[1]
    }
    #[allow(dead_code)]
    fn remove(&mut self, vec: &[Var], vs: VarId) {
        let s = self.idxs[vs];
        let n = self.idxs[0];
        if n < s {
            return;
        }
        let vn = self.heap[n];
        self.heap.swap(n, s);
        self.idxs.swap(vn, vs);
        self.idxs[0] -= 1;
        if 1 < self.idxs[0] {
            self.percolate_down(&vec, 1);
        }
    }
    #[allow(dead_code)]
    fn check(&self, s: &str) {
        let h = &mut self.heap.clone()[1..];
        let d = &mut self.idxs.clone()[1..];
        h.sort();
        d.sort();
        for i in 0..h.len() {
            if h[i] != i + 1 {
                panic!("heap {} {} {:?}", i, h[i], h);
            }
            if d[i] != i + 1 {
                panic!("idxs {} {} {:?}", i, d[i], d);
            }
        }
        println!(" - pass var_order test at {}", s);
    }
}

impl fmt::Display for VarOccHeap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            " - seek pointer - nth -> var: {:?}\n - var -> nth: {:?}",
            self.heap, self.idxs,
        )
    }
}
