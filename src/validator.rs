use crate::{
    clause::ClauseId,
    solver::Solver,
    traits::{LitIF, PropagatorIF, ValidatorIF, VarDBIF},
    types::{Lit, MaybeInconsistent, SolverError},
};

impl ValidatorIF for Solver {
    fn inject_assigmnent(&mut self, vec: &[i32]) -> MaybeInconsistent {
        if vec.is_empty() {
            return Err(SolverError::Inconsistent);
        }
        for val in vec {
            let l = Lit::from(*val);
            let vi = l.vi();
            self.asgs
                .enqueue(&mut self.vdb[vi], bool::from(l), ClauseId::default(), 0)?;
        }
        Ok(())
    }
    /// returns None if the given assignment is a model of a problem.
    /// Otherwise returns a clause which is not satisfiable under a given assignment.
    fn validate(&self) -> Option<Vec<i32>> {
        for ch in &self.cdb[1..] {
            if !self.vdb.satisfies(&ch.lits) {
                let mut v = Vec::new();
                for l in &ch.lits {
                    v.push(i32::from(*l));
                }
                return Some(v);
            }
        }
        None
    }
}
