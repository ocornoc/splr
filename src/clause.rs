#![allow(dead_code)]
#![allow(unused_imports)]
use std::fmt;
use types::*;

/// Clause
pub struct Clause {
    pub activity: f64,
    pub rank: i32,
    pub lits: Vec<Lit>,
}

/// Clause should be placed on heap anytime.
/// And `Box` provides Eq for 'clause pointer'.
pub type BoxClause = Box<Clause>;

impl Clause {
    pub fn new(v: Vec<Lit>) -> Clause {
        Clause {
            activity: 0.0,
            rank: v.len() as i32,
            lits: v,
        }
    }
    pub fn null() -> Clause {
        Clause {
            activity: 0.0,
            rank: 0,
            lits: vec![],
        }
    }
}

impl PartialEq for Clause {
    fn eq(&self, other: &Clause) -> bool {
        self.lits == other.lits
    }
}

impl Eq for Clause {}

impl fmt::Display for Clause {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.lits.len() {
            0 => write!(f, "null_clause"),
            2 => write!(f, "a biclause"),
            _ => write!(f, "a clause"),
        }
    }
}

pub struct ClauseExtManager {
    num_actives: i32,          // number of active clause
    purged: bool,              // -- whether it needs gc
    clauses: Vec<Box<Clause>>, // -- clause list
    keys: Vec<i32>,            // Int list
}

impl ClauseExtManager {
    fn new() -> ClauseExtManager {
        ClauseExtManager {
            num_actives: 0,
            purged: false,
            clauses: vec![],
            keys: vec![],
        }
    }
}
