// SAT solver for Propositional Logic in Rust
use {
    libc::{clock_gettime, timespec, CLOCK_PROCESS_CPUTIME_ID},
    splr::{
        cdb::CertifiedRecord,
        config::{Config, VERSION},
        restart::RestartMode,
        solver::{Certificate, SatSolverIF, Solver, SolverResult},
        state::*,
        types::{Export, SolverError},
    },
    std::{
        borrow::Cow,
        env,
        fs::File,
        io::{BufWriter, Write},
        path::PathBuf,
        thread,
        time::Duration,
    },
    structopt::StructOpt,
};

const RED: &str = "\x1B[001m\x1B[031m";
const GREEN: &str = "\x1B[001m\x1B[032m";
const BLUE: &str = "\x1B[001m\x1B[034m";
const RESET: &str = "\x1B[000m";

fn colored(v: Result<bool, &SolverError>, quiet: bool) -> Cow<'static, str> {
    if quiet {
        match v {
            Ok(false) => Cow::Borrowed("s UNSATISFIABLE"),
            Ok(true) => Cow::Borrowed("s SATISFIABLE"),
            Err(e) => Cow::from(format!("c UNKNOWN ({})", e)),
        }
    } else {
        match v {
            Ok(false) => Cow::from(format!("{}s UNSATISFIABLE{}", GREEN, RESET)),
            Ok(true) => Cow::from(format!("{}s SATISFIABLE{}", BLUE, RESET)),
            Err(e) => Cow::from(format!("{}c UNKNOWN ({}){}", RED, e, RESET)),
        }
    }
}

fn main() {
    let config = Config::from_args().override_args();
    if !config.cnf_file.exists() {
        println!(
            "{} does not exist.",
            config.cnf_file.file_name().unwrap().to_str().unwrap()
        );
        return;
    }
    let cnf_file = config.cnf_file.to_string_lossy();
    let ans_file: Option<PathBuf> = match config.result_file.to_string_lossy().as_ref() {
        "-" => None,
        "" => Some(config.output_dir.join(PathBuf::from(format!(
            ".ans_{}",
            config.cnf_file.file_name().unwrap().to_string_lossy(),
        )))),
        _ => Some(config.output_dir.join(&config.result_file)),
    };
    if config.proof_file.to_string_lossy() != "proof.out" && !config.use_certification {
        println!("Abort: You set a proof filename with '--proof' explicitly, but didn't set '--certify'. It doesn't look good.");
        return;
    }
    if let Ok(val) = env::var("SPLR_TIMEOUT") {
        if let Ok(timeout) = val.parse::<u64>() {
            let input = cnf_file.as_ref().to_string();
            let quiet_mode = config.quiet_mode;
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(timeout * 1000));
                println!(
                    "{}: {}",
                    colored(Err(&SolverError::TimeOut), quiet_mode),
                    input
                );
                std::process::exit(0);
            });
        }
    }
    let mut s = Solver::build(&config).expect("failed to load");
    let res = s.solve();
    save_result(&s, &res, &cnf_file, ans_file);
    if 0 < s.state.config.dump_int && !s.state.development.is_empty() {
        let dump = config.cnf_file.file_stem().unwrap().to_str().unwrap();
        if let Ok(f) = File::create(format!("stat_{}.csv", dump)) {
            let mut buf = BufWriter::new(f);
            buf.write_all(b"conflict,solved,restart,block,ASG,LBD\n")
                .unwrap();
            for (n, a, b, c, d, e) in s.state.development.iter() {
                buf.write_all(
                    format!("{:.0},{:.5},{:.0},{:.0},{:.5},{:.5}\n", n, a, b, c, d, e,).as_bytes(),
                )
                .unwrap();
            }
        }
    }
    std::process::exit(match res {
        Ok(Certificate::SAT(_)) => 10,
        Ok(Certificate::UNSAT) => 20,
        Err(_) => 0,
    });
}

fn save_result<S: AsRef<str> + std::fmt::Display>(
    s: &Solver,
    res: &SolverResult,
    input: S,
    output: Option<PathBuf>,
) {
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
                    "      Result|dump: to STDOUT instead of {} due to an IO error.",
                    f.to_string_lossy(),
                ),
                Some(ref f) => println!("      Result|file: {}", f.to_str().unwrap(),),
                _ => (),
            }
            println!(
                "{}: {}",
                colored(Ok(true), s.state.config.quiet_mode),
                input
            );
            if let Err(why) = (|| {
                buf.write_all(
                    format!(
                        "c An assignment set generated by splr-{} for {}\nc\n",
                        VERSION, input,
                    )
                    .as_bytes(),
                )?;
                report(s, buf)?;
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
                    "      Result|dump: to STDOUT instead of {} due to an IO error.",
                    f.to_string_lossy(),
                ),
                Some(ref f) => println!("      Result|file: {}", f.to_str().unwrap(),),
                _ => (),
            }
            if s.state.config.use_certification {
                let proof_file: PathBuf =
                    s.state.config.output_dir.join(&s.state.config.proof_file);
                save_proof(&s, &input, &proof_file);
                println!(
                    " Certificate|file: {}",
                    s.state.config.proof_file.to_string_lossy()
                );
            }
            println!(
                "{}: {}",
                colored(Ok(false), s.state.config.quiet_mode),
                input
            );
            if let Err(why) = (|| {
                buf.write_all(
                    format!(
                        "c The empty assignment set generated by splr-{} for {}\nc\n",
                        VERSION, input,
                    )
                    .as_bytes(),
                )?;
                report(s, &mut buf)?;
                buf.write_all(b"s UNSATISFIABLE\n")?;
                buf.write_all(b"0\n")
            })() {
                println!("Abort: failed to save by {}!", why);
            }
        }
        Err(e) => {
            match output {
                Some(ref f) if redirect => println!(
                    "      Result|dump: to STDOUT instead of {} due to an IO error.",
                    f.to_string_lossy(),
                ),
                Some(ref f) => println!("      Result|file: {}", f.to_str().unwrap(),),
                _ => (),
            }
            println!("{}: {}", colored(Err(e), s.state.config.quiet_mode), input);
            if let Err(why) = (|| {
                buf.write_all(
                    format!(
                        "c An assignment set generated by splr-{} for {}\nc\n",
                        VERSION, input,
                    )
                    .as_bytes(),
                )?;
                report(s, buf)?;
                buf.write_all(format!("c {}\n", e,).as_bytes())?;
                buf.write(b"0\n")
            })() {
                println!("Abort: failed to save by {}!", why);
            }
        }
    }
}

fn save_proof<S: AsRef<str> + std::fmt::Display>(s: &Solver, input: S, output: &PathBuf) {
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
}

fn report(s: &Solver, out: &mut dyn Write) -> std::io::Result<()> {
    let state = &s.state;
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
    let (asg_num_conflict, _num_propagation, asg_num_restart, _core, vdb_activity_decay) =
        s.asg.exports();
    let (rst_mode, _num_block, _asg_trend, _lbd_get, _lbd_trend) = s.rst.exports();
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
            format!("{:>11}", state[LogUsizeId::Conflict]),
            format!("{:>13}", state[LogUsizeId::Decision]),
            format!("{:>15}", state[LogUsizeId::Propagate]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c   Assignment|#rem:{}, #fix:{}, #elm:{}, prg%:{} \n",
            format!("{:>9}", state[LogUsizeId::Remain]),
            format!("{:>9}", state[LogUsizeId::Fixed]),
            format!("{:>9}", state[LogUsizeId::Eliminated]),
            format!("{:>9.4}", state[LogF64Id::Progress]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c       Clause|Remv:{}, LBD2:{}, Binc:{}, Perm:{} \n",
            format!("{:>9}", state[LogUsizeId::Removable]),
            format!("{:>9}", state[LogUsizeId::LBD2]),
            format!("{:>9}", state[LogUsizeId::Binclause]),
            format!("{:>9}", state[LogUsizeId::Permanent]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c  {}|#BLK:{}, #RST:{}, eASG:{}, eLBD:{} \n",
            if rst_mode == RestartMode::Luby {
                "LubyRestart"
            } else {
                "    Restart"
            },
            format!(
                "{:>9}",
                state.record.vali[LogUsizeId::RestartBlock as usize]
            ),
            format!("{:>9}", state[LogUsizeId::Restart]),
            format!("{:>9.4}", state[LogF64Id::EmaAsg]),
            format!("{:>9.4}", state[LogF64Id::EmaLBD]),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     Conflict|eLBD:{}, cnfl:{}, bjmp:{}, rpc%:{} \n",
            format!("{:>9.2}", state[LogF64Id::AveLBD]),
            format!("{:>9.2}", state[LogF64Id::CLevel]),
            format!("{:>9.2}", state[LogF64Id::BLevel]),
            format!(
                "{:>9.4}",
                100.0 * asg_num_restart as f64 / asg_num_conflict as f64
            ),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c         misc|#rdc:{}, #elm:{}, core:{}, vdcy:{} \n",
            format!("{:>9}", state[LogUsizeId::Reduction]),
            format!("{:>9}", state[LogUsizeId::Elimination]),
            format!("{:>9.0}", state[LogF64Id::CoreSize]),
            format!("{:>9.4}", vdb_activity_decay),
        )
        .as_bytes(),
    )?;
    out.write_all(
        format!(
            "c     Strategy|mode:{:>15}, time:{:9.2}\n",
            state.strategy.0, tm,
        )
        .as_bytes(),
    )?;
    out.write_all(b"c\n")?;
    Ok(())
}
