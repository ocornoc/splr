// SAT solver for Propositional Logic in Rust

use libc::{clock_gettime, timespec, CLOCK_PROCESS_CPUTIME_ID};
use splr::clause::CertifiedRecord;
use splr::config::{Config, VERSION};
use splr::solver::{Certificate, Solver, SolverResult};
use splr::state::*;
use splr::traits::SatSolverIF;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use structopt::StructOpt;

fn main() {
    let config = Config::from_args();
    if !config.cnf_filename.exists() {
        println!(
            "{} does not exist.",
            config.cnf_filename.file_name().unwrap().to_str().unwrap()
        );
        return;
    }
    let cnf_file = config.cnf_filename.to_string_lossy();
    let ans_file: Option<PathBuf> = match config.result_filename.to_string_lossy().as_ref() {
        "-" => None,
        "" => Some(config.output_dirname.join(PathBuf::from(format!(
            ".ans_{}",
            config.cnf_filename.file_name().unwrap().to_string_lossy(),
        )))),
        _ => Some(config.output_dirname.join(&config.result_filename)),
    };
    if config.proof_filename.to_string_lossy() != "proof.out" && !config.use_certification {
        println!("Abort: You set a proof filename with '--proof' explicitly, but didn't set '--certify'. It doesn't look good.");
        return;
    }
    let proof_file: PathBuf = config.output_dirname.join(&config.proof_filename);
    let mut s = Solver::build(&config).expect("failed to load");
    let res = s.solve();
    match &res {
        Ok(cert) => {
            save_result(&s, &res, &cnf_file, ans_file);
            if config.use_certification && *cert == Certificate::UNSAT {
                save_proof(&s, &cnf_file, &proof_file);
            }
        }
        Err(e) => println!("Failed to execution by {:?}.", e),
    }
    // /*
    if 0 < s.state.config.debug_dump && !s.state.development_history.is_empty() {
        let dump = config.cnf_filename.file_stem().unwrap().to_str().unwrap();
        if let Ok(f) = File::create(format!("dbg_{}.csv", dump)) {
            let mut buf = BufWriter::new(f);
            buf.write_all(b"conflict,value,kind\n").unwrap();
            for (n, a, b, c, d, e, f) in s.state.development_history.iter() {
                buf.write_all(format!("{:>7},{:>8.0},\"restartAsg\"\n", n, a).as_bytes())
                    .unwrap();
                buf.write_all(format!("{:>7},{:>8.0},\"restartFUP\"\n", n, b).as_bytes())
                    .unwrap();
                buf.write_all(format!("{:>7},{:>8.5},\"LDBtrend\"\n", n, c).as_bytes())
                    .unwrap();
                buf.write_all(format!("{:>7},{:>8.5},\"ASGtrend\"\n", n, d).as_bytes())
                    .unwrap();
                buf.write_all(format!("{:>7},{:>8.5},\"FUPtrend\"\n", n, e).as_bytes())
                    .unwrap();
                buf.write_all(format!("{:>7},{:>8.5},\"none\"\n", n, f).as_bytes())
                    .unwrap();
            }
        }
    }
    // */
}

#[allow(dead_code)]
fn save_result(s: &Solver, res: &SolverResult, input: &str, output: Option<PathBuf>) {
    let mut ofile;
    let mut otty;
    let mut redirect = false;
    let mut buf: &mut dyn Write = match output {
        Some(ref file) => {
            if let Ok(f) = File::create(file) {
                ofile = BufWriter::new(f);
                &mut ofile
            } else {
                redirect = true;
                otty = BufWriter::new(std::io::stdout());
                &mut otty
            }
        }
        None => {
            otty = BufWriter::new(std::io::stdout());
            &mut otty
        }
    };
    match res {
        Ok(Certificate::SAT(v)) => {
            match output {
                Some(ref f) if redirect => println!(
                    "SATISFIABLE: {}.\nRedirect the result to STDOUT instead of {} due to an IO error.\n",
                    input,
                    f.to_string_lossy(),
                    ),
                Some(ref f) => println!(
                    "SATISFIABLE: {}. The result was saved to {}.",
                    input,
                    f.to_str().unwrap()
                ),
                _ => println!("SATISFIABLE: {}.", input),
            }
            if let Err(why) = (|| {
                buf.write_all(
                    format!(
                        "c An assignment set generated by splr-{} for {}\nc\n",
                        VERSION, input,
                    )
                    .as_bytes(),
                )?;
                report(&s.state, buf)?;
                buf.write_all(b"s SATISFIABLE\n")?;
                for x in v {
                    buf.write_all(format!("{} ", x).as_bytes())?;
                }
                buf.write(b"0\n")
            })() {
                println!("Abort: failed to save by {}!", why);
            }
        }
        Ok(Certificate::UNSAT) => {
            match output {
                Some(ref f) if redirect => println!(
                    "UNSAT: {}.\nRedirect the result to STDOUT insteard of {} due to an IO error.\n",
                    input,
                    f.to_string_lossy()
                ),
                Some(ref f)  => println!(
                    "UNSAT: {}, The result was saved to {}.",
                    input,
                    f.to_str().unwrap()
                ),

                _ => println!("UNSAT: {}.", input),
            }
            if let Err(why) = (|| {
                buf.write_all(
                    format!(
                        "c The empty assignment set generated by splr-{} for {}\nc\n",
                        VERSION, input,
                    )
                    .as_bytes(),
                )?;
                report(&s.state, &mut buf)?;
                buf.write_all(b"s UNSATISFIABLE\n")?;
                buf.write_all(b"0\n")
            })() {
                println!("Abort: failed to save by {}!", why);
            }
        }
        Err(e) => println!("Failed to execution by {:?}.", e),
    }
}

fn save_proof(s: &Solver, input: &str, output: &PathBuf) {
    let mut buf = match File::create(output) {
        Ok(out) => BufWriter::new(out),
        Err(e) => {
            println!(
                "Abort: failed to create the proof file {:?} by {}!",
                output.to_string_lossy(),
                e
            );
            return;
        }
    };
    if let Err(why) = (|| {
        buf.write_all(
            format!("c Proof generated by splr-{} for {}\nc\n", VERSION, input).as_bytes(),
        )?;
        buf.write_all(b"s UNSATISFIABLE\n")?;
        for (f, x) in &s.cdb.certified[1..] {
            if *f == CertifiedRecord::DELETE {
                buf.write_all(b"d ")?;
            }
            for l in x {
                buf.write_all(format!("{} ", l).as_bytes())?;
            }
            buf.write_all(b"0\n")?;
        }
        buf.write_all(b"0\n")
    })() {
        println!(
            "Abort: failed to save to {} by {}!",
            output.to_string_lossy(),
            why
        );
        return;
    }
    println!(
        "The certification was saved to {}.",
        output.to_str().unwrap()
    );
}

fn report(state: &State, out: &mut dyn Write) -> std::io::Result<()> {
    let tm = {
        let mut time = timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut time) } == -1 {
            match state.start.elapsed() {
                Ok(e) => e.as_secs() as f64 + f64::from(e.subsec_millis()) / 1000.0f64,
                Err(_) => 0.0f64,
            }
        } else {
            time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0f64
        }
    };
    out.write_all(
        format!(
            "c {:<43}, #var:{:9}, #cls:{:9}\n",
            state.target.pathname, state.target.num_of_variables, state.target.num_of_clauses,
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c  #conflict:{}, #decision:{}, #propagate:{} \n",
            format!("{:>11}", state.record[LogUsizeId::Conflict]),
            format!("{:>13}", state.record[LogUsizeId::Decision]),
            format!("{:>15}", state.record[LogUsizeId::Propagate]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     Progress|#rem:{}, #fix:{}, #elm:{}, prg%:{} \n",
            format!("{:>9}", state.record[LogUsizeId::Remain]),
            format!("{:>9}", state.record[LogUsizeId::Fixed]),
            format!("{:>9}", state.record[LogUsizeId::Eliminated]),
            format!("{:>9.4}", state.record[LogF64Id::Progress]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c  Clause Kind|Remv:{}, LBD2:{}, Binc:{}, Perm:{} \n",
            format!("{:>9}", state.record[LogUsizeId::Removable]),
            format!("{:>9}", state.record[LogUsizeId::LBD2]),
            format!("{:>9}", state.record[LogUsizeId::Binclause]),
            format!("{:>9}", state.record[LogUsizeId::Permanent]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     Conflict|cnfl:{}, bjmp:{}, aLBD:{}, trnd:{} \n",
            format!("{:>9.2}", state.record[LogF64Id::CLevel]),
            format!("{:>9.2}", state.record[LogF64Id::BLevel]),
            format!("{:>9.2}", state.record[LogF64Id::LBD]),
            format!("{:>9.4}", state.record[LogF64Id::LBDTrend]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c   Assignment|#ave:{}, #ave:{}, e-64:{}, trnd:{} \n",
            format!("{:>9.0}", state.record[LogUsizeId::AsgMax]),
            format!("{:>9.2}", state.record[LogF64Id::AsgAve]),
            format!("{:>9.4}", state.record[LogF64Id::AsgEma]),
            format!("{:>9.4}", state.record[LogF64Id::AsgTrn]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c    First UIP|#all:{}, #now:{}, #inc:{}, end%:{} \n",
            format!("{:>9}", state.record[LogUsizeId::SuF]),
            format!("{:>9}", state.record[LogUsizeId::FUP]),
            format!("{:>9.4}", state.record[LogF64Id::FUPInc]),
            format!("{:>9.4}", state.record[LogF64Id::FUPPrg]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c      Restart|#byA:{}, #byF:{}, #byL:{}, #sum:{} \n",
            format!("{:>9}", state.record[LogUsizeId::RestartByAsg]),
            format!("{:>9}", state.record[LogUsizeId::RestartByFUP]),
            format!("{:>9}", state.record[LogUsizeId::RestartByLuby]),
            format!("{:>9}", state.record[LogUsizeId::Restart]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     ClauseDB|#rdc:{}, #sce:{}, #exe:{}, ____:{} \n",
            format!("{:>9}", state.record[LogUsizeId::Reduction]),
            format!("{:>9}", state.record[LogUsizeId::SatClauseElim]),
            format!("{:>9}", state.record[LogUsizeId::ExhaustiveElim]),
            format!("{:>9}", 0),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     Strategy|mode:{:>15}, time:{:9.2}\n",
            state.strategy, tm,
        )
        .as_bytes(),
    )?;
    out.write_all(b"c\n")?;
    Ok(())
}
