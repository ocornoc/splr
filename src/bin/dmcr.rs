// DIMACS Model Checker in Rust
#![allow(unused_imports)]
use {
    splr::{
        config::Config,
        solver::{SatSolverIF, Solver},
        validator::ValidatorIF,
    },
    std::{
        env,
        fs::File,
        io::{stdin, BufRead, BufReader, Result},
        path::{Path, PathBuf},
    },
    structopt::StructOpt,
};

const RED: &str = "\x1B[001m\x1B[031m";
const GREEN: &str = "\x1B[001m\x1B[032m";
const BLUE: &str = "\x1B[001m\x1B[034m";
const RESET: &str = "\x1B[000m";

#[derive(StructOpt)]
#[structopt(name = "dmcr", about = "DIMACS-format Model Checker in Rust")]
struct TargetOpts {
    #[structopt(parse(from_os_str))]
    #[structopt(short = "a", long = "assign")]
    /// an assign file generated by slpr
    assign: Option<std::path::PathBuf>,
    #[structopt(parse(from_os_str))]
    /// a CNF file
    problem: std::path::PathBuf,
    #[structopt(long = "without-color", short = "C")]
    /// disable colorized output
    without_color: bool,
}

fn main() {
    let mut from_file = true;
    let mut found = false;
    let mut args = TargetOpts::from_args();
    let cnf = args.problem.to_str().unwrap();
    if !args.problem.exists() {
        println!("{} does not exist.", args.problem.to_str().unwrap(),);
        return;
    }
    let mut config = Config::default();
    config.cnf_filename = args.problem.clone();
    let (red, green, blue) = if args.without_color {
        (RESET, RESET, RESET)
    } else {
        (RED, GREEN, BLUE)
    };
    let mut s = Solver::build(&config).expect("failed to load");
    if args.assign == None {
        args.assign = Some(PathBuf::from(format!(
            ".ans_{}",
            Path::new(&args.problem)
                .file_name()
                .unwrap()
                .to_string_lossy()
        )));
    }
    if let Some(f) = &args.assign {
        if let Ok(d) = File::open(f.as_path()) {
            if let Some(vec) = read_assignment(&mut BufReader::new(d), cnf, &args.assign) {
                if s.inject_assigmnent(&vec).is_err() {
                    println!(
                        "{}{} seems an unsat problem but no proof.{}",
                        blue,
                        args.problem.to_str().unwrap(),
                        RESET
                    );
                    return;
                }
            } else {
                return;
            }
            found = true;
        }
    }
    if !found {
        if let Some(vec) = read_assignment(&mut BufReader::new(stdin()), cnf, &args.assign) {
            if s.inject_assigmnent(&vec).is_err() {
                println!(
                    "{}{} seems an unsat problem but no proof.{}",
                    blue,
                    args.problem.to_str().unwrap(),
                    RESET,
                );
                return;
            }
            found = true;
            from_file = false;
        } else {
            return;
        }
    }
    if !found {
        println!("There's no assign file.");
        return;
    }
    match s.validate() {
        Some(v) => println!(
            "{}An invalid assignment set for {}{} due to {:?}.",
            red,
            args.problem.to_str().unwrap(),
            RESET,
            v,
        ),
        None if from_file => println!(
            "{}A valid assignment set for {}{} is found in {}",
            green,
            &args.problem.to_str().unwrap(),
            RESET,
            &args.assign.unwrap().to_str().unwrap(),
        ),
        None => println!(
            "{}A valid assignment set for {}.{}",
            green,
            &args.problem.to_str().unwrap(),
            RESET,
        ),
    }
}

fn read_assignment(rs: &mut dyn BufRead, cnf: &str, assign: &Option<PathBuf>) -> Option<Vec<i32>> {
    let mut buf = String::new();
    loop {
        match rs.read_line(&mut buf) {
            Ok(0) => return Some(Vec::new()),
            Ok(_) => {
                if buf.starts_with('c') {
                    buf.clear();
                    continue;
                }
                if buf.starts_with('s') {
                    if buf.starts_with("s SATISFIABLE") {
                        buf.clear();
                        continue;
                    } else if buf.starts_with("s UNSATISFIABLE") {
                        println!("{} seems an unsatisfiable problem. I can't handle it.", cnf);
                        return None;
                    } else if let Some(asg) = assign {
                        println!("{} seems an illegal format file.", asg.to_str().unwrap(),);
                        return None;
                    }
                }
                let mut v: Vec<i32> = Vec::new();
                for s in buf.split_whitespace() {
                    match s.parse::<i32>() {
                        Ok(0) => break,
                        Ok(x) => v.push(x),
                        Err(e) => panic!("{} by {}", e, s),
                    }
                }
                return Some(v);
            }
            Err(e) => panic!("{}", e),
        }
    }
}
