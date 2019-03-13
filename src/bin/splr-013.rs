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
    let cnf_file = config.cnf_filename.to_str().unwrap().to_string();
    let ans_file: Option<PathBuf> = match config.result_filename.as_str() {
        "-" => None,
        "" => Some(PathBuf::from(&config.output_dirname).join(PathBuf::from(format!(
            ".ans_{}",
            config.cnf_filename.file_name().unwrap().to_str().unwrap()
        )))),
        _ => Some(PathBuf::from(&config.output_dirname).join(PathBuf::from(&config.result_filename))),
    };
    let proof_file: PathBuf =
        PathBuf::from(&config.output_dirname).join(PathBuf::from(&config.proof_filename));
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
}

#[allow(dead_code)]
fn save_result(s: &Solver, res: &SolverResult, input: &str, output: Option<PathBuf>) {
    let mut ofile;
    let mut otty;
    let mut buf: &mut dyn Write = match output {
        Some(ref f) => {
            ofile = BufWriter::new(File::create(f).expect("fail to create"));
            &mut ofile
        }
        None => {
            otty = BufWriter::new(std::io::stdout());
            &mut otty
        }
    };
    match res {
        Ok(Certificate::SAT(v)) => {
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
                panic!("failed to save: {:?}!", why);
            }
            match output {
                Some(f) => println!(
                    "SATISFIABLE: {}. The answer was saved to {}.",
                    input,
                    f.to_str().unwrap()
                ),
                None => println!("SATISFIABLE: {}.", input),
            }
        }
        Ok(Certificate::UNSAT) => {
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
                panic!("failed to save: {:?}!", why);
            }
            match output {
                Some(f) => println!(
                    "UNSAT: {}, The answer was saved to {}.",
                    input,
                    f.to_str().unwrap()
                ),
                None => println!("UNSAT: {}.", input),
            }
        }
        Err(e) => println!("Failed to execution by {:?}.", e),
    }
}

fn save_proof(s: &Solver, input: &str, output: &PathBuf) {
    let mut buf = if let Ok(out) = File::create(output) {
        BufWriter::new(out)
    } else {
        panic!("failed to create {:?}!", output);
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
        panic!("failed to save: {:?}!", why);
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
            format!("{:>11}", state.record.vali[LogUsizeId::Conflict as usize]),
            format!("{:>13}", state.record.vali[LogUsizeId::Decision as usize]),
            format!("{:>15}", state.record.vali[LogUsizeId::Propagate as usize]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c   Assignment|#rem:{}, #fix:{}, #elm:{}, prg%:{} \n",
            format!("{:>9}", state.record.vali[LogUsizeId::Remain as usize]),
            format!("{:>9}", state.record.vali[LogUsizeId::Fixed as usize]),
            format!("{:>9}", state.record.vali[LogUsizeId::Eliminated as usize]),
            format!("{:>9.4}", state.record.valf[LogF64Id::Progress as usize]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c  Clause Kind|Remv:{}, LBD2:{}, Binc:{}, Perm:{} \n",
            format!("{:>9}", state.record.vali[LogUsizeId::Removable as usize]),
            format!("{:>9}", state.record.vali[LogUsizeId::LBD2 as usize]),
            format!("{:>9}", state.record.vali[LogUsizeId::Binclause as usize]),
            format!("{:>9}", state.record.vali[LogUsizeId::Permanent as usize]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c      Restart|#BLK:{}, #RST:{}, eASG:{}, eLBD:{} \n",
            format!(
                "{:>9}",
                state.record.vali[LogUsizeId::RestartBlock as usize]
            ),
            format!("{:>9}", state.record.vali[LogUsizeId::Restart as usize]),
            format!("{:>9.4}", state.record.valf[LogF64Id::EmaAsg as usize]),
            format!("{:>9.4}", state.record.valf[LogF64Id::EmaLBD as usize]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c    Conflicts|aLBD:{}, bjmp:{}, cnfl:{} |blkR:{} \n",
            format!("{:>9.2}", state.record.valf[LogF64Id::AveLBD as usize]),
            format!("{:>9.2}", state.record.valf[LogF64Id::BLevel as usize]),
            format!("{:>9.2}", state.record.valf[LogF64Id::CLevel as usize]),
            format!("{:>9.4}", state.record.valf[LogF64Id::RestartBlkR as usize]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c    Clause DB|#rdc:{}, #sce:{}, #exe:{} |frcK:{} \n",
            format!("{:>9}", state.record.vali[LogUsizeId::Reduction as usize]),
            format!(
                "{:>9}",
                state.record.vali[LogUsizeId::SatClauseElim as usize]
            ),
            format!(
                "{:>9}",
                state.record.vali[LogUsizeId::ExhaustiveElim as usize]
            ),
            format!("{:>9.4}", state.record.valf[LogF64Id::RestartThrK as usize]),
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
