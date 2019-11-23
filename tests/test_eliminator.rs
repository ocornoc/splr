// cargo test -- --nocapture
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
use {
    splr::{
        clause::ClauseId,
        config::Config,
        solver::Solver,
        traits::*,
        types::*,
    },
};
        
macro_rules! mkv {
    ($($x:expr),*) => {
        match &[$($x),*] {
            v => v.iter().map(|x| Lit::from(*x as i32)).collect::<Vec<Lit>>(),
        }
    };
}

// #[test]
fn check_occurs() {
    let cfg: Config = Default::default();
    let cnf: CNFDescription = CNFDescription {
        num_of_variables: 10,
        num_of_clauses: 10,
        pathname: "".to_string(),
    };
    let mut s = Solver::instantiate(&cfg, &cnf);

    let c1 = s.cdb.new_clause(&mkv![1, 2, 3], 3, true);
    let c2 = s.cdb.new_clause(&mkv![-2, 3, 4], 3, true);
    let c3 = s.cdb.new_clause(&mkv![-2, -3], 2, true);
    let c4 = s.cdb.new_clause(&mkv![1, 2, -3, 9], 4, true);
    //    {
    //        let vec = [&c2, &c3]; // [&c1, &c2, &c3, &c4];
    //        for x in &vec {
    //            for y in &vec {
    //                println!(
    //                    "{}\tsubsumes\t{}\t=>\t{:?}",
    //                    x,
    //                    y,
    //                    x.subsumes(&y).map(|l| l.int())
    //                );
    //            }
    //        }
    //    }
    //    // s.attach_clause(c1);
    //    s.attach_clause(c2);
    //    s.attach_clause(c3);
    //    // s.attach_clause(c4);
    //    // s.vars.dump("##added");
    //    println!("{:?}", s.eliminator);
    //    s.eliminate();
    //    // s.vars.dump("##eliminated");
    //    println!("{:?}", s.eliminator);
    //    println!("::done");
}

fn mk_c(s: &mut Solver, i: usize, v: Vec<i32>) -> ClauseId {
    let vec = v.iter().map(|i| Lit::from(*i)).collect::<Vec<Lit>>();
    let cid = s.cdb.new_clause(&vec, vec.len(), true);
    cid
}
