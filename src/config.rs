use std::path::PathBuf;
use structopt::StructOpt;

pub const VERSION: &str = "0.1.6-dev.1";
pub const ACTIVITY_MAX: f64 = 1e308;

/// Configuration built from command line options
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "splr", about, author)]
pub struct Config {
    /// soft limit of #clauses (6MC/GB)
    #[structopt(long = "cl", default_value = "0")]
    pub clause_limit: usize,
    /// grow limit of #clauses by v-elim
    #[structopt(long = "eg", default_value = "4")]
    pub elim_grow_limit: usize,
    /// #literals in a clause by v-elim
    #[structopt(long = "el", default_value = "64")]
    pub elim_lit_limit: usize,
    /// length for assignment average
    #[structopt(long = "ra", default_value = "3500")]
    pub restart_asg_len: usize,
    /// length for LBD average
    #[structopt(long = "rl", default_value = "50")]
    pub restart_lbd_len: usize,
    /// forcing restart threshold
    #[structopt(long = "rt", default_value = "0.70")]
    pub restart_threshold: f64, // Glucose's K
    /// blocking restart threshold
    #[structopt(long = "rb", default_value = "1.40")]
    pub restart_blocking: f64, // Glucose's R
    /// #conflicts between restarts
    #[structopt(long = "rs", default_value = "50")]
    pub restart_step: usize,
    /// a DIMACS format CNF file
    #[structopt(parse(from_os_str))]
    pub cnf_filename: PathBuf,
    /// output directory
    #[structopt(long = "dir", short = "o", default_value = ".", parse(from_os_str))]
    pub output_dirname: PathBuf,
    /// result filename/stdout
    #[structopt(long = "result", short = "r", default_value = "", parse(from_os_str))]
    pub result_filename: PathBuf,
    /// filename for DRAT cert.
    #[structopt(
        long = "proof",
        default_value = "proof.out",
        short = "p",
        parse(from_os_str)
    )]
    pub proof_filename: PathBuf,
    /// Uses Glucose-like progress report
    #[structopt(long = "log", short = "l")]
    pub use_log: bool,
    /// Disables exhaustive simplification
    #[structopt(long = "without-elim", short = "E")]
    pub without_elim: bool,
    /// Disables dynamic restart adaptation
    #[structopt(long = "without-adaptive-restart", short = "R")]
    pub without_adaptive_restart: bool,
    /// Disables dynamic strategy adaptation
    #[structopt(long = "without-adaptive-strategy", short = "S")]
    pub without_adaptive_strategy: bool,
    /// Disables deep search mode
    #[structopt(long = "without-deep-search", short = "D")]
    pub without_deep_search: bool,
    /// Writes a DRAT UNSAT certification file
    #[structopt(long = "certify", short = "c")]
    pub use_certification: bool,
    /// CPU time limit in sec.
    #[structopt(long = "to", default_value = "0")]
    pub timeout: f64,
    /// interval for dumpping stat data
    #[structopt(long = "stat", default_value = "0")]
    pub dump_interval: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            clause_limit: 18_000_000,
            elim_grow_limit: 4,
            elim_lit_limit: 100,
            restart_asg_len: 3500,
            restart_lbd_len: 50,
            restart_threshold: 0.60,
            restart_blocking: 1.40,
            restart_step: 50,
            cnf_filename: PathBuf::new(),
            output_dirname: PathBuf::from("."),
            result_filename: PathBuf::new(),
            proof_filename: PathBuf::from("proof.out"),
            use_log: false,
            without_elim: false,
            without_adaptive_restart: false,
            without_adaptive_strategy: false,
            without_deep_search: true,
            use_certification: false,
            timeout: 0.0,
            dump_interval: 0,
        }
    }
}

impl<T> From<T> for Config
where
    PathBuf: From<T>,
{
    fn from(path: T) -> Config {
        let mut config = Config::default();
        config.cnf_filename = PathBuf::from(path);
        config
    }
}
