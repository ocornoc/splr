#include "mtl/Sort.h"
#include "simp/SimpSolver.h"
#include "utils/System.h"
using namespace Glucose;

// Options:
static const char* _cat = "SIMP";
static BoolOption opt_use_elim        (_cat, "elim",    "Perform variable elimination.", true);
static IntOption  opt_grow            (_cat, "grow",    "Allow a variable elimination step to grow by a number of clauses.", 0);
static IntOption  opt_clause_lim      (_cat, "cl-lim",  "Variables are not eliminated if it produces a resolvent with a length above this limit. -1 means no limit", 20,   IntRange(-1, INT32_MAX));
static IntOption  opt_subsumption_lim (_cat, "sub-lim", "Do not check if subsumption against a clause larger than this. -1 means no limit.", 1000, IntRange(-1, INT32_MAX));

// Constructor/Destructor:
SimpSolver::SimpSolver() :
   Solver()
  , grow               (opt_grow)
  , clause_lim         (opt_clause_lim)
  , subsumption_lim    (opt_subsumption_lim)
  , use_elim           (opt_use_elim)
  , merges             (0)
  , eliminated_vars    (0)
  , elimorder          (1)
  , occurs             (ClauseDeleted(ca))
  , elim_heap          (ElimLt(n_occ))
  , bwdsub_assigns     (0)
  , n_touched          (0)
{
    vec<Lit> dummy(1,lit_Undef);
    ca.extra_clause_field = true; // NOTE: must happen before allocating the dummy clause below.
    bwdsub_tmpunit        = ca.alloc(dummy);
    remove_satisfied      = false;
}

SimpSolver::SimpSolver(const SimpSolver &s) : Solver(s)
  , grow               (s.grow)
  , clause_lim         (s.clause_lim)
  , subsumption_lim    (s.subsumption_lim)
  , use_elim           (s.use_elim)
  , merges             (s.merges)
  , eliminated_vars    (s.eliminated_vars)
  , elimorder          (s.elimorder)
  , occurs             (ClauseDeleted(ca))
  , elim_heap          (ElimLt(n_occ))
  , bwdsub_assigns     (s.bwdsub_assigns)
  , n_touched          (s.n_touched)
{
    // TODO: Copy dummy... what is it???
    vec<Lit> dummy(1,lit_Undef);
    ca.extra_clause_field = true; // NOTE: must happen before allocating the dummy clause below.
    bwdsub_tmpunit        = ca.alloc(dummy);
    remove_satisfied      = false;
    //End TODO  
    s.elimclauses.memCopyTo(elimclauses);
    s.touched.memCopyTo(touched);
    s.occurs.copyTo(occurs);
    s.n_occ.memCopyTo(n_occ);
    s.elim_heap.copyTo(elim_heap);
    s.subsumption_queue.copyTo(subsumption_queue);
    s.frozen.memCopyTo(frozen);
    s.eliminated.memCopyTo(eliminated);
    use_simplification = s.use_simplification;
    bwdsub_assigns = s.bwdsub_assigns;
    n_touched = s.n_touched;
    bwdsub_tmpunit = s.bwdsub_tmpunit;
    qhead = s.qhead;
    ok = s.ok;
}

Var SimpSolver::newVar(bool sign, bool dvar) {
    Var v = Solver::newVar(sign, dvar);
    frozen    .push((char)false);
    eliminated.push((char)false);
    n_occ     .push(0);
    n_occ     .push(0);
    occurs    .init(v);
    touched   .push(0);
    elim_heap .insert(v);
    return v; }

lbool SimpSolver::solve_(bool do_simp, bool turn_off_simp) {
    vec<Var> extra_frozen;
    lbool    result = l_True;
    // Assumptions must be temporarily frozen to run variable elimination:
    for (int i = 0; i < assumptions.size(); i++){
      Var v = var(assumptions[i]);
      // If an assumption has been eliminated, remember it.
      assert(!isEliminated(v));
      if (!frozen[v]){
	// Freeze and store.
	setFrozen(v, true);
	extra_frozen.push(v);
      }
    }
    result = lbool(eliminate(turn_off_simp));
    if (result == l_True)
        result = Solver::solve_();
    if (result == l_True)
        extendModel();
    // Unfreeze the assumptions that were frozen:
    for (int i = 0; i < extra_frozen.size(); i++)
      setFrozen(extra_frozen[i], false);
    return result;
}

bool SimpSolver::addClause_(vec<Lit>& ps) {
#ifndef NDEBUG
    for (int i = 0; i < ps.size(); i++)
        assert(!isEliminated(var(ps[i])));
#endif
    int nclauses = clauses.size();
    if (!Solver::addClause_(ps))
        return false;
    if (clauses.size() == nclauses + 1){
        CRef          cr = clauses.last();
        const Clause& c  = ca[cr];
        // NOTE: the clause is added to the queue immediately and then
        // again during 'gatherTouchedClauses()'. If nothing happens
        // in between, it will only be checked once. Otherwise, it may
        // be checked twice unnecessarily. This is an unfortunate
        // consequence of how backward subsumption is used to mimic
        // forward subsumption.
        subsumption_queue.insert(cr);
        for (int i = 0; i < c.size(); i++){
            occurs[var(c[i])].push(cr);
            n_occ[toInt(c[i])]++;
            touched[var(c[i])] = 1;
            n_touched++;
            if (elim_heap.inHeap(var(c[i])))
                elim_heap.increase(var(c[i]));
        }
    }
    return true;
}

void SimpSolver::removeClause(CRef cr,bool inPurgatory) {
    const Clause& c = ca[cr];
    for (int i = 0; i < c.size(); i++){
      n_occ[toInt(c[i])]--;
      updateElimHeap(var(c[i]));
      occurs.smudge(var(c[i]));
    Solver::removeClause(cr,inPurgatory);
}

bool SimpSolver::strengthenClause(CRef cr, Lit l) {
    Clause& c = ca[cr];
    assert(decisionLevel() == 0);
    // FIX: this is too inefficient but would be nice to have (properly implemented)
    // if (!find(subsumption_queue, &c))
    subsumption_queue.insert(cr);
    if (c.size() == 2){
        removeClause(cr);
        c.strengthen(l);
    }else{
        detachClause(cr, true);
        c.strengthen(l);
        attachClause(cr);
        remove(occurs[var(l)], cr);
        n_occ[toInt(l)]--;
        updateElimHeap(var(l));
    }
    return c.size() == 1 ? enqueue(c[0]) && propagate() == CRef_Undef : true;
}

// Returns FALSE if clause is always satisfied ('out_clause' should not be used).
bool SimpSolver::merge(const Clause& _ps, const Clause& _qs, Var v, vec<Lit>& out_clause) {
    merges++;
    out_clause.clear();
    bool  ps_smallest = _ps.size() < _qs.size();
    const Clause& ps  =  ps_smallest ? _qs : _ps;
    const Clause& qs  =  ps_smallest ? _ps : _qs;
    for (int i = 0; i < qs.size(); i++){
        if (var(qs[i]) != v){
            for (int j = 0; j < ps.size(); j++)
                if (var(ps[j]) == var(qs[i]))
                    if (ps[j] == ~qs[i])
                        return false;
                    else
                        goto next;
            out_clause.push(qs[i]);
        }
        next:;
    }
    for (int i = 0; i < ps.size(); i++)
        if (var(ps[i]) != v)
            out_clause.push(ps[i]);
    return true;
}

// Returns FALSE if clause is always satisfied.
bool SimpSolver::merge(const Clause& _ps, const Clause& _qs, Var v, int& size) {
    merges++;
    bool  ps_smallest = _ps.size() < _qs.size();
    const Clause& ps  =  ps_smallest ? _qs : _ps;
    const Clause& qs  =  ps_smallest ? _ps : _qs;
    const Lit*  __ps  = (const Lit*)ps;
    const Lit*  __qs  = (const Lit*)qs;
    size = ps.size()-1;
    for (int i = 0; i < qs.size(); i++){
        if (var(__qs[i]) != v){
            for (int j = 0; j < ps.size(); j++)
                if (var(__ps[j]) == var(__qs[i]))
                    if (__ps[j] == ~__qs[i])
                        return false;
                    else
                        goto next;
            size++;
        }
        next:;
    }
    return true;
}

void SimpSolver::gatherTouchedClauses() {
    if (n_touched == 0) return;
    int i,j;
    for (i = j = 0; i < subsumption_queue.size(); i++)
        if (ca[subsumption_queue[i]].mark() == 0)
            ca[subsumption_queue[i]].mark(2);
    for (i = 0; i < touched.size(); i++)
        if (touched[i]){
            const vec<CRef>& cs = occurs.lookup(i);
            for (j = 0; j < cs.size(); j++)
                if (ca[cs[j]].mark() == 0){
                    subsumption_queue.insert(cs[j]);
                    ca[cs[j]].mark(2);
                }
            touched[i] = 0;
        }
    for (i = 0; i < subsumption_queue.size(); i++)
        if (ca[subsumption_queue[i]].mark() == 2)
            ca[subsumption_queue[i]].mark(0);
    n_touched = 0;
}

bool SimpSolver::implied(const vec<Lit>& c) {
    assert(decisionLevel() == 0);
    trail_lim.push(trail.size());
    for (int i = 0; i < c.size(); i++)
        if (value(c[i]) == l_True){
            cancelUntil(0);
            return false;
        }else if (value(c[i]) != l_False){
            assert(value(c[i]) == l_Undef);
            uncheckedEnqueue(~c[i]);
        }
    bool result = propagate() != CRef_Undef;
    cancelUntil(0);
    return result;
}

// Backward subsumption + backward subsumption resolution
bool SimpSolver::backwardSubsumptionCheck(bool verbose) {
    int cnt = 0;
    int subsumed = 0;
    int deleted_literals = 0;
    assert(decisionLevel() == 0);
    while (subsumption_queue.size() > 0 || bwdsub_assigns < trail.size()){
        // Empty subsumption queue and return immediately on user-interrupt:
        if (asynch_interrupt){
            subsumption_queue.clear();
            bwdsub_assigns = trail.size();
            break; }
        // Check top-level assignments by creating a dummy clause and placing it in the queue:
        if (subsumption_queue.size() == 0 && bwdsub_assigns < trail.size()){
            Lit l = trail[bwdsub_assigns++];
            ca[bwdsub_tmpunit][0] = l;
            ca[bwdsub_tmpunit].calcAbstraction();
            subsumption_queue.insert(bwdsub_tmpunit); }
        CRef    cr = subsumption_queue.peek(); subsumption_queue.pop();
        Clause& c  = ca[cr];
        if (c.mark()) continue;
        assert(c.size() > 1 || value(c[0]) == l_True);    // Unit-clauses should have been propagated before this point.
        // Find best variable to scan:
        Var best = var(c[0]);
        for (int i = 1; i < c.size(); i++)
            if (occurs[var(c[i])].size() < occurs[best].size())
                best = var(c[i]);
        // Search all candidates:
        vec<CRef>& _cs = occurs.lookup(best);
        CRef*       cs = (CRef*)_cs;
        for (int j = 0; j < _cs.size(); j++)
            if (c.mark())
                break;
            else if (!ca[cs[j]].mark() &&  cs[j] != cr && (subsumption_lim == -1 || ca[cs[j]].size() < subsumption_lim)){
                Lit l = c.subsumes(ca[cs[j]]);
                if (l == lit_Undef)
                    subsumed++, removeClause(cs[j]);
                else if (l != lit_Error){
                    deleted_literals++;
                    if (!strengthenClause(cs[j], ~l))
                        return false;
                    // Did current candidate get deleted from cs? Then check candidate at index j again:
                    if (var(l) == best)
                        j--;
                }
            }
    }
    return true;
}


static void mkElimClause(vec<uint32_t>& elimclauses, Lit x) {
    elimclauses.push(toInt(x));
    elimclauses.push(1);
}

static void mkElimClause(vec<uint32_t>& elimclauses, Var v, Clause& c) {
    int first = elimclauses.size();
    int v_pos = -1;
    // Copy clause to elimclauses-vector. Remember position where the
    // variable 'v' occurs:
    for (int i = 0; i < c.size(); i++){
        elimclauses.push(toInt(c[i]));
        if (var(c[i]) == v)
            v_pos = i + first;
    }
    assert(v_pos != -1);
    // Swap the first literal with the 'v' literal, so that the literal
    // containing 'v' will occur first in the clause:
    uint32_t tmp = elimclauses[v_pos];
    elimclauses[v_pos] = elimclauses[first];
    elimclauses[first] = tmp;
    // Store the length of the clause last:
    elimclauses.push(c.size());
}

bool SimpSolver::eliminateVar(Var v) {
    assert(!frozen[v]);
    assert(!isEliminated(v));
    assert(value(v) == l_Undef);
    // Split the occurrences into positive and negative:
    const vec<CRef>& cls = occurs.lookup(v);
    vec<CRef>        pos, neg;
    for (int i = 0; i < cls.size(); i++)
        (find(ca[cls[i]], mkLit(v)) ? pos : neg).push(cls[i]);
    // Check wether the increase in number of clauses stays within the allowed ('grow'). Moreover, no
    // clause must exceed the limit on the maximal clause size (if it is set):
    int cnt         = 0;
    int clause_size = 0;
    for (int i = 0; i < pos.size(); i++)
        for (int j = 0; j < neg.size(); j++)
            if (merge(ca[pos[i]], ca[neg[j]], v, clause_size) && 
                (++cnt > cls.size() + grow || (clause_lim != -1 && clause_size > clause_lim)))
                return true;
    // Delete and store old clauses
    eliminated[v] = true;
    setDecisionVar(v, false);
    eliminated_vars++;
    if (pos.size() > neg.size()){
        for (int i = 0; i < neg.size(); i++)
            mkElimClause(elimclauses, v, ca[neg[i]]);
        mkElimClause(elimclauses, mkLit(v));
    }else{
        for (int i = 0; i < pos.size(); i++)
            mkElimClause(elimclauses, v, ca[pos[i]]);
        mkElimClause(elimclauses, ~mkLit(v));
    }
    // Produce clauses in cross product:
    vec<Lit>& resolvent = add_tmp;
    for (int i = 0; i < pos.size(); i++)
        for (int j = 0; j < neg.size(); j++)
            if (merge(ca[pos[i]], ca[neg[j]], v, resolvent) && !addClause_(resolvent))
                return false;
    for (int i = 0; i < cls.size(); i++)
        removeClause(cls[i]);
    // Free occurs list for this variable:
    occurs[v].clear(true);
    // Free watchers lists for this variable, if possible:
    if (watches[ mkLit(v)].size() == 0) watches[ mkLit(v)].clear(true);
    if (watches[~mkLit(v)].size() == 0) watches[~mkLit(v)].clear(true);
    return backwardSubsumptionCheck();
}

bool SimpSolver::substitute(Var v, Lit x) {
    assert(!frozen[v]);
    assert(!isEliminated(v));
    assert(value(v) == l_Undef);
    if (!ok) return false;
    eliminated[v] = true;
    setDecisionVar(v, false);
    const vec<CRef>& cls = occurs.lookup(v);
    vec<Lit>& subst_clause = add_tmp;
    for (int i = 0; i < cls.size(); i++){
        Clause& c = ca[cls[i]];
        subst_clause.clear();
        for (int j = 0; j < c.size(); j++){
            Lit p = c[j];
            subst_clause.push(var(p) == v ? x ^ sign(p) : p);
        }
        if (!addClause_(subst_clause))
            return ok = false;
       removeClause(cls[i]);
   }
    return true;
}

void SimpSolver::extendModel() {
    int i, j;
    Lit x;
    if(model.size()==0) model.growTo(nVars());
    for (i = elimclauses.size()-1; i > 0; i -= j){
        for (j = elimclauses[i--]; j > 1; j--, i--)
            if (modelValue(toLit(elimclauses[i])) != l_False)
                goto next;
        x = toLit(elimclauses[i]);
        model[var(x)] = lbool(!sign(x));
    next:;
    }
}

bool SimpSolver::eliminate(bool turn_off_elim) {
    if (!simplify()) {
        ok = false;
        return false;
    }
    // Main simplification loop:
    int toPerform = clauses.size()<=4800000;
    if(!toPerform) {
      printf("c Too many clauses... No preprocessing\n");
    }
    while (toPerform && (n_touched > 0 || bwdsub_assigns < trail.size() || elim_heap.size() > 0)){
        gatherTouchedClauses();
        // printf("  ## (time = %6.2f s) BWD-SUB: queue = %d, trail = %d\n", cpuTime(), subsumption_queue.size(), trail.size() - bwdsub_assigns);
        if ((subsumption_queue.size() > 0 || bwdsub_assigns < trail.size()) && 
            !backwardSubsumptionCheck(true)){
            ok = false; goto cleanup; }
        // Empty elim_heap and return immediately on user-interrupt:
        if (asynch_interrupt){
            assert(bwdsub_assigns == trail.size());
            assert(subsumption_queue.size() == 0);
            assert(n_touched == 0);
            elim_heap.clear();
            goto cleanup; }
        // printf("  ## (time = %6.2f s) ELIM: vars = %d\n", cpuTime(), elim_heap.size());
        for (int cnt = 0; !elim_heap.empty(); cnt++){
            Var elim = elim_heap.removeMin();
            if (asynch_interrupt) break;
            if (isEliminated(elim) || value(elim) != l_Undef) continue;
            // At this point, the variable may have been set by assymetric branching, so check it
            // again. Also, don't eliminate frozen variables:
            if (use_elim && value(elim) == l_Undef && !frozen[elim] && !eliminateVar(elim)){
                ok = false; goto cleanup; }
            checkGarbage(false);
        }
        assert(subsumption_queue.size() == 0);
    }
 cleanup:
    // If no more simplification is needed, free all simplification-related data structures:
    if (turn_off_elim){
        touched  .clear(true);
        occurs   .clear(true);
        n_occ    .clear(true);
        elim_heap.clear(true);
        subsumption_queue.clear(true);
        use_simplification    = false;
        remove_satisfied      = true;
        ca.extra_clause_field = false;
        // Force full cleanup (this is safe and desirable since it only happens once):
        rebuildOrderHeap();
        garbageCollect();
    }else{
        // Cheaper cleanup:
        cleanUpClauses(); // TODO: can we make 'cleanUpClauses()' not be linear in the problem size somehow?
        checkGarbage();
    }
    return ok;
}

void SimpSolver::cleanUpClauses() {
    occurs.cleanAll();
    int i,j;
    for (i = j = 0; i < clauses.size(); i++)
        if (ca[clauses[i]].mark() == 0)
            clauses[j++] = clauses[i];
    clauses.shrink(i - j);
}

// Garbage Collection methods:
void SimpSolver::relocAll(ClauseAllocator& to) {
    // All occurs lists:
    for (int i = 0; i < nVars(); i++){
        vec<CRef>& cs = occurs[i];
        for (int j = 0; j < cs.size(); j++)
            ca.reloc(cs[j], to);
    }
    // Subsumption queue:
    for (int i = 0; i < subsumption_queue.size(); i++)
        ca.reloc(subsumption_queue[i], to);
    // Temporary clause:
    ca.reloc(bwdsub_tmpunit, to);
}

void SimpSolver::garbageCollect() {
    // Initialize the next region to a size corresponding to the estimated utilization degree. This
    // is not precise but should avoid some unnecessary reallocations for the new region:
    ClauseAllocator to(ca.size() - ca.wasted()); 
    cleanUpClauses();
    to.extra_clause_field = ca.extra_clause_field; // NOTE: this is important to keep (or lose) the extra fields.
    relocAll(to);
    Solver::relocAll(to);
    to.moveTo(ca);
}
