//! Conflict Analysis
use {
    super::{
        restart::{RestartIF, Restarter, RestarterModule},
        State,
    },
    crate::{
        assign::{AssignIF, AssignStack, PropagateIF, VarManipulateIF, VarRewardIF},
        cdb::{ClauseDB, ClauseDBIF},
        processor::{EliminateIF, Eliminator},
        types::*,
    },
};

#[allow(clippy::cognitive_complexity)]
pub fn handle_conflict(
    asg: &mut AssignStack,
    cdb: &mut ClauseDB,
    elim: &mut Eliminator,
    rst: &mut Restarter,
    state: &mut State,
    ci: ClauseId,
) -> MaybeInconsistent {
    let original_dl = asg.decision_level();
    // we need a catch here for handling the possibility of level zero conflict
    // at higher level due to the incoherence between the current level and conflicting
    // level in chronoBT. This leads to UNSAT solution. No need to update misc stats.
    {
        let level = asg.level_ref();
        if cdb[ci].iter().all(|l| level[l.vi()] == 0) {
            return Err(SolverError::NullLearnt);
        }
    }

    let (ncnfl, _num_propagation, asg_num_restart, _) = asg.exports();
    // If we can settle this conflict w/o restart, solver will get a big progress.
    let switch_chronobt = if ncnfl < 1000 || asg.recurrent_conflicts() {
        Some(false)
    } else {
        None
    };
    rst.update(RestarterModule::Counter, ncnfl);

    if 0 < state.last_asg {
        rst.update(RestarterModule::ASG, asg.stack_len());
        state.last_asg = 0;
    }

    //
    //## DYNAMIC BLOCKING RESTART based on ASG, updated on conflict path
    //
    rst.block_restart();
    let mut use_chronobt = switch_chronobt.unwrap_or(0 < state.config.cbt_thr);
    if use_chronobt {
        let level = asg.level_ref();
        let c = &cdb[ci];
        let lcnt = c.iter().filter(|l| level[l.vi()] == original_dl).count();
        if 1 == lcnt {
            debug_assert!(c.iter().any(|l| level[l.vi()] == original_dl));
            let decision = *c.iter().find(|l| level[l.vi()] == original_dl).unwrap();
            let snd_l = c
                .iter()
                .map(|l| level[l.vi()])
                .filter(|l| *l != original_dl)
                .max()
                .unwrap_or(0);
            if 0 < snd_l {
                // If the conflicting clause contains one literallfrom the maximal
                // decision level, we let BCP propagating that literal at the second
                // highest decision level in conflicting cls.
                // PREMISE: 0 < snd_l
                asg.cancel_until(snd_l - 1);
                debug_assert!(
                    asg.stack_iter().all(|l| l.vi() != decision.vi()),
                    format!("lcnt == 1: level {}, snd level {}", original_dl, snd_l)
                );
                asg.assign_by_decision(decision);
                return Ok(());
            }
        }
    }
    // conflicting level
    // By mixing two restart modes, we must assume a conflicting level is under the current decision level,
    // even if `use_chronobt` is off, because `use_chronobt` is a flag for future behavior.
    let cl = {
        let cl = asg.decision_level();
        let c = &cdb[ci];
        let level = asg.level_ref();
        let lv = c.iter().map(|l| level[l.vi()]).max().unwrap_or(0);
        if lv < cl {
            asg.cancel_until(lv);
            lv
        } else {
            cl
        }
    };
    debug_assert!(
        cdb[ci].iter().any(|l| asg.level(l.vi()) == cl),
        format!(
            "use_{}: {:?}, {:?}",
            use_chronobt,
            cl,
            cdb[ci]
                .iter()
                .map(|l| (i32::from(*l), asg.level(l.vi())))
                .collect::<Vec<_>>(),
        )
    );
    // backtrack level by analyze
    let bl_a = conflict_analyze(asg, cdb, state, ci).max(asg.root_level);
    if state.new_learnt.is_empty() {
        #[cfg(debug)]
        {
            println!(
                "empty learnt at {}({}) by {:?}",
                cl,
                asg.reason(asg.decision_vi(cl)) == ClauseId::default(),
                asg.dump(asg, &cdb[ci]),
            );
        }
        return Err(SolverError::NullLearnt);
    }
    // asg.bump_vars(asg, cdb, ci);
    let new_learnt = &mut state.new_learnt;
    let l0 = new_learnt[0];
    // assert: 0 < cl, which was checked already by new_learnt.is_empty().

    // NCB places firstUIP on level bl, while CB does it on level cl.
    // Therefore the condition to use CB is: activity(firstUIP) < activity(v(bl)).
    // PREMISE: 0 < bl, because asg.decision_vi accepts only non-zero values.
    use_chronobt &= switch_chronobt.unwrap_or(
        bl_a == 0
            || state.config.cbt_thr + bl_a <= cl
            || asg.activity(l0.vi()) < asg.activity(asg.decision_vi(bl_a)),
    );

    // (assign level, backtrack level)
    let (al, bl) = if use_chronobt {
        (
            {
                let level = asg.level_ref();
                new_learnt[1..]
                    .iter()
                    .map(|l| level[l.vi()])
                    .max()
                    .unwrap_or(0)
            },
            cl - 1,
        )
    } else {
        (bl_a, bl_a)
    };
    let learnt_len = new_learnt.len();
    if learnt_len == 1 {
        //
        //## PARTIAL FIXED SOLUTION by UNIT LEARNT CLAUSE GENERATION
        //
        // dump to certified even if it's a literal.
        cdb.certificate_add(new_learnt);
        if use_chronobt {
            asg.cancel_until(bl);
            debug_assert!(asg.stack_iter().all(|l| l.vi() != l0.vi()));
            asg.assign_by_implication(l0, AssignReason::default(), 0);
        } else {
            asg.assign_by_unitclause(l0);
        }
        asg.num_solved_vars += 1;
        rst.update(RestarterModule::Reset, 0);
    } else {
        {
            // At the present time, some reason clauses can contain first UIP or its negation.
            // So we have to filter vars instead of literals to avoid double counting.
            let mut bumped = new_learnt.iter().map(|l| l.vi()).collect::<Vec<VarId>>();
            for lit in new_learnt.iter() {
                //
                //## Learnt Literal Rewarding
                //
                asg.reward_at_analysis(lit.vi());
                if !state.stabilize {
                    continue;
                }
                if let AssignReason::Implication(r, _) = asg.reason(lit.vi()) {
                    for l in &cdb[r].lits {
                        let vi = l.vi();
                        if !bumped.contains(&vi) {
                            //
                            //## Reason-Side Rewarding
                            //
                            asg.reward_at_analysis(vi);
                            bumped.push(vi);
                        }
                    }
                }
            }
        }
        asg.cancel_until(bl);
        let cid = cdb.new_clause(asg, new_learnt, true, true);
        elim.add_cid_occur(asg, cid, &mut cdb[cid], true);
        state.c_lvl.update(cl as f64);
        state.b_lvl.update(bl as f64);
        asg.assign_by_implication(
            l0,
            AssignReason::Implication(
                cid,
                if learnt_len == 2 {
                    new_learnt[1]
                } else {
                    NULL_LIT
                },
            ),
            al,
        );
        let lbd = cdb[cid].rank;
        rst.update(RestarterModule::LBD, lbd);
        if 1 < learnt_len && learnt_len <= state.config.elim_cls_lim / 2 {
            elim.to_simplify += 1.0 / (learnt_len - 1) as f64;
        }
    }
    cdb.scale_activity();
    if 0 < state.config.dump_int && ncnfl % state.config.dump_int == 0 {
        let (_mode, rst_num_block, rst_asg_trend, _lbd_get, rst_lbd_trend) = rst.exports();
        state.development.push((
            ncnfl,
            (asg.num_solved_vars + asg.num_eliminated_vars) as f64
                / state.target.num_of_variables as f64,
            asg_num_restart as f64,
            rst_num_block as f64,
            rst_asg_trend.min(10.0),
            rst_lbd_trend.min(10.0),
        ));
    }
    cdb.check_and_reduce(asg, ncnfl);
    Ok(())
}

///
/// ## Conflict Analysis
///
#[allow(clippy::cognitive_complexity)]
fn conflict_analyze(
    asg: &mut AssignStack,
    cdb: &mut ClauseDB,
    state: &mut State,
    confl: ClauseId,
) -> DecisionLevel {
    let learnt = &mut state.new_learnt;
    learnt.clear();
    learnt.push(NULL_LIT);
    let dl = asg.decision_level();
    let mut p = NULL_LIT;
    let mut ti = asg.stack_len() - 1; // trail index
    let mut path_cnt = 0;
    loop {
        let reason = if p == NULL_LIT {
            AssignReason::Implication(confl, NULL_LIT)
        } else {
            asg.reason(p.vi())
        };
        match reason {
            AssignReason::Implication(_, l) if l != NULL_LIT => {
                // cid = asg.reason(p.vi());
                let vi = l.vi();
                if !asg.var(vi).is(Flag::CA_SEEN) {
                    let lvl = asg.level(vi);
                    if 0 == lvl {
                        continue;
                    }
                    debug_assert!(!asg.var(vi).is(Flag::ELIMINATED));
                    debug_assert!(asg.assign(vi).is_some());
                    asg.var_mut(vi).turn_on(Flag::CA_SEEN);
                    if dl <= lvl {
                        path_cnt += 1;
                        asg.reward_at_analysis(vi);
                    } else {
                        #[cfg(feature = "trace_analysis")]
                        println!("- push {} to learnt, which level is {}", q.int(), lvl);
                        // learnt.push(l);
                    }
                } else {
                    #[cfg(feature = "trace_analysis")]
                    {
                        if !asg.var(vi).is(Flag::CA_SEEN) {
                            println!("- ignore {} because it was flagged", q.int());
                        } else {
                            println!("- ignore {} because its level is {}", q.int(), lvl);
                        }
                    }
                }
            }
            AssignReason::Implication(cid, _) => {
                #[cfg(feature = "trace_analysis")]
                println!("analyze {}", p.int());
                debug_assert_ne!(cid, ClauseId::default());
                if cdb[cid].is(Flag::LEARNT) {
                    if !cdb[cid].is(Flag::JUST_USED) && !cdb.convert_to_permanent(asg, cid) {
                        cdb[cid].turn_on(Flag::JUST_USED);
                    }
                    cdb.bump_activity(cid, ());
                }
                let c = &cdb[cid];
                #[cfg(feature = "boundary_check")]
                assert!(
                    0 < c.len(),
                    format!(
                        "Level {} I-graph reaches {}:{} for {}:{}",
                        asg.decision_level(),
                        cid,
                        c,
                        p,
                        asg.var(p.vi())
                    )
                );
                #[cfg(feature = "trace_analysis")]
                println!("- handle {}", cid.fmt());
                for q in &c[(p != NULL_LIT) as usize..] {
                    let vi = q.vi();
                    if !asg.var(vi).is(Flag::CA_SEEN) {
                        // asg.reward_at_analysis(vi);
                        let lvl = asg.level(vi);
                        if 0 == lvl {
                            continue;
                        }
                        debug_assert!(!asg.var(vi).is(Flag::ELIMINATED));
                        debug_assert!(asg.assign(vi).is_some());
                        asg.var_mut(vi).turn_on(Flag::CA_SEEN);
                        if dl <= lvl {
                            // println!("- flag for {} which level is {}", q.int(), lvl);
                            path_cnt += 1;
                            //
                            //## Conflict-Side Rewarding
                            //
                            asg.reward_at_analysis(vi);
                        } else {
                            #[cfg(feature = "trace_analysis")]
                            println!("- push {} to learnt, which level is {}", q.int(), lvl);
                            learnt.push(*q);
                        }
                    } else {
                        #[cfg(feature = "trace_analysis")]
                        {
                            if !asg.var(vi).is(Flag::CA_SEEN) {
                                println!("- ignore {} because it was flagged", q.int());
                            } else {
                                println!("- ignore {} because its level is {}", q.int(), lvl);
                            }
                        }
                    }
                }
            }
            AssignReason::None => {
                #[cfg(feature = "boundary_check")]
                panic!("conflict_analyze: faced AssignReason::None.");
            }
        }
        // The following case was subsumed into `search`.
        /*
        // In an unsat problem, a conflict can occur at decision level zero
        // by a clause which literals' levels are zero.
        // So we have the posibility getting the following situation.
        if p == NULL_LIT && path_cnt == 0 {
            #[cfg(feature = "boundary_check")]
            println!("Empty learnt at lvl:{}", asg.level());
            learnt.clear();
            return asg.root_level;
        }
        */
        // set the index of the next literal to ti
        while {
            let vi = asg.stack(ti).vi();
            #[cfg(feature = "boundary_check")]
            assert!(
                vi < asg.level_ref().len(),
                format!("ti:{}, lit:{}, len:{}", ti, asg.stack(ti), asg.stack_len())
            );
            let lvl = asg.level(vi);
            let v = asg.var(vi);
            !v.is(Flag::CA_SEEN) || lvl != dl
        } {
            #[cfg(feature = "trace_analysis")]
            println!("- skip {} because it isn't flagged", asg[ti].int());
            #[cfg(feature = "boundary_check")]
            assert!(
                0 < ti,
                format!(
                    "p:{}, path_cnt:{}, lv:{}, learnt:{:?}\nconflict:{:?}",
                    p,
                    path_cnt,
                    dl,
                    asg.dump(&*learnt),
                    asg.dump(&cdb[confl].lits),
                ),
            );
            ti -= 1;
        }
        p = asg.stack(ti);
        #[cfg(feature = "trace_analysis")]
        println!(
            "- move to flagged {}, which reason is {}; num path: {}",
            p.vi(),
            path_cnt - 1,
            cid.fmt()
        );
        asg.var_mut(p.vi()).turn_off(Flag::CA_SEEN);
        // since the trail can contain a literal which level is under `dl` after
        // the `dl`-th thdecision var, we must skip it.
        path_cnt -= 1;
        if path_cnt == 0 {
            break;
        }
        debug_assert!(0 < ti);
        ti -= 1;
    }
    debug_assert!(learnt.iter().all(|l| *l != !p));
    debug_assert_eq!(asg.level(p.vi()), dl);
    learnt[0] = !p;
    #[cfg(feature = "trace_analysis")]
    println!(
        "- appending {}, the result is {:?}",
        learnt[0].int(),
        vec2int(learnt)
    );
    state.minimize_learnt(asg, cdb)
}

impl State {
    fn minimize_learnt(&mut self, asg: &mut AssignStack, cdb: &mut ClauseDB) -> DecisionLevel {
        let State {
            ref mut new_learnt, ..
        } = self;
        let mut to_clear: Vec<Lit> = vec![new_learnt[0]];
        let mut levels = vec![false; asg.decision_level() as usize + 1];
        let level = asg.level_ref();
        for l in &new_learnt[1..] {
            to_clear.push(*l);
            levels[level[l.vi()] as usize] = true;
        }
        let l0 = new_learnt[0];
        #[cfg(feature = "boundary_check")]
        assert!(!new_learnt.is_empty());
        new_learnt.retain(|l| *l == l0 || !l.is_redundant(asg, cdb, &mut to_clear, &levels));
        let len = new_learnt.len();
        if 2 < len && len < 30 {
            cdb.minimize_with_biclauses(asg, new_learnt);
        }
        // find correct backtrack level from remaining literals
        let mut level_to_return = 0;
        let level = asg.level_ref();
        if 1 < new_learnt.len() {
            let mut max_i = 1;
            level_to_return = level[new_learnt[max_i].vi()];
            for (i, l) in new_learnt.iter().enumerate().skip(2) {
                let lv = level[l.vi()];
                if level_to_return < lv {
                    level_to_return = lv;
                    max_i = i;
                }
            }
            new_learnt.swap(1, max_i);
        }
        for l in &to_clear {
            asg.var_mut(l.vi()).turn_off(Flag::CA_SEEN);
        }
        level_to_return
    }
}

/// return `true` if the `lit` is redundant, which is defined by
/// any leaf of implication graph for it isn't a fixed var nor a decision var.
impl Lit {
    fn is_redundant(
        self,
        asg: &mut AssignStack,
        cdb: &ClauseDB,
        clear: &mut Vec<Lit>,
        levels: &[bool],
    ) -> bool {
        if asg.reason(self.vi()) == AssignReason::default() {
            return false;
        }
        let mut stack = Vec::new();
        stack.push(self);
        let top = clear.len();
        while let Some(sl) = stack.pop() {
            match asg.reason(sl.vi()) {
                AssignReason::None => panic!("no idea"),
                AssignReason::Implication(_, l) if l != NULL_LIT => {
                    let vi = l.vi();
                    let lv = asg.level(vi);
                    if 0 < lv && !asg.var(vi).is(Flag::CA_SEEN) {
                        if asg.reason(vi) != AssignReason::default() && levels[lv as usize] {
                            asg.var_mut(vi).turn_on(Flag::CA_SEEN);
                            stack.push(l);
                            clear.push(l);
                        } else {
                            // one of the roots is a decision var at an unchecked level.
                            for l in &clear[top..] {
                                asg.var_mut(l.vi()).turn_off(Flag::CA_SEEN);
                            }
                            clear.truncate(top);
                            return false;
                        }
                    }
                }
                AssignReason::Implication(cid, _) => {
                    let c = &cdb[cid];
                    #[cfg(feature = "boundary_check")]
                    assert!(0 < c.len());
                    for q in &(*c)[1..] {
                        let vi = q.vi();
                        let lv = asg.level(vi);
                        if 0 < lv && !asg.var(vi).is(Flag::CA_SEEN) {
                            if asg.reason(vi) != AssignReason::default() && levels[lv as usize] {
                                asg.var_mut(vi).turn_on(Flag::CA_SEEN);
                                stack.push(*q);
                                clear.push(*q);
                            } else {
                                // one of the roots is a decision var at an unchecked level.
                                for l in &clear[top..] {
                                    asg.var_mut(l.vi()).turn_off(Flag::CA_SEEN);
                                }
                                clear.truncate(top);
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }
}
