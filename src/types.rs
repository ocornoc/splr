/// Crate `types' provides various building blocks, including
/// some common traits.
pub use crate::{
    assign::AssignReason,
    cdb::{Clause, ClauseIF, ClauseId, ClauseIdIF, Watch},
    config::Config,
};
use {
    crate::solver::SolverEvent,
    std::{
        convert::TryFrom,
        fmt,
        fs::File,
        io::{BufRead, BufReader},
        ops::{Index, IndexMut, Neg, Not},
        path::{Path, PathBuf},
    },
};

/// API for accessing internal data in a module.
/// For example, `State::progress` needs to access misc parameters and statistics,
/// which, however, should be used locally in the defining modules.
/// To avoid to make them public, we define a generic accessor or exporter here.
/// `T` is the list of exporting values.
pub trait Export<T, Mode> {
    fn exports(&self) -> T;
    fn mode(&self) -> Mode;
}

pub trait ExportBox<'a, T> {
    fn exports_box(&'a self) -> Box<T>;
}

/// API for Literal like `from_int`, `from_assign`, `to_cid` and so on.
pub trait LitIF {
    /// convert to bool
    fn as_bool(self) -> bool;
    /// convert [VarId](../type.VarId.html) to [Lit](../type.Lit.html).
    /// It returns a positive literal if `p` is `TRUE` or `BOTTOM`.
    fn from_assign(vi: VarId, p: bool) -> Self;
    /// convert to var index.
    fn vi(self) -> VarId;
    /// return `true` if it is a valid literal, namely non-zero.
    fn is_none(self) -> bool;
}

/// API for clause and var rewarding.
pub trait ActivityIF {
    /// type for index
    type Ix;
    /// an extra parameter for bumping
    type Inc;
    /// return the current activity of an element.
    fn activity(&mut self, ix: Self::Ix) -> f64;
    /// set activity forcibly.
    fn set_activity(&mut self, ix: Self::Ix, val: f64);
    /// update an element's activity.
    fn bump_activity(&mut self, ix: Self::Ix, dl: Self::Inc);
    /// increment activity step.
    fn scale_activity(&mut self);
}

/// API for object instantiation based on `Configuration` and `CNFDescription`.
/// This is implemented by *all the Splr modules* except `Configuration` and `CNFDescription`.
///
/// # Example
///
/// ```
/// use crate::{splr::config::Config, splr::types::*};
/// use splr::{cdb::ClauseDB, solver::Solver};
/// let _ = ClauseDB::instantiate(&Config::default(), &CNFDescription::default());
/// let _ = Solver::instantiate(&Config::default(), &CNFDescription::default());
///```
pub trait Instantiate {
    /// make and return an object from `Config` and `CNFDescription`.
    fn instantiate(conf: &Config, cnf: &CNFDescription) -> Self;
    /// update by a solver event.
    fn handle(&mut self, _e: SolverEvent) {}
}

/// API for O(n) deletion from a list, providing `delete_unstable`.
pub trait Delete<T> {
    /// *O(n)* item deletion protocol.
    fn delete_unstable<F>(&mut self, filter: F)
    where
        F: FnMut(&T) -> bool;
}

/// 'Variable' identifier or 'variable' index, starting with one.
/// Implementation note: NonZeroUsize can be used but requires a lot of changes.
/// The current abstraction is incomplete.
pub type VarId = usize;

/// Decision Level Representation.
pub type DecisionLevel = u32;

/// Literal encoded on `u32` as:
///
/// - the literal corresponding to a positive occurrence of *variable `n` is `2 * n` and
/// - that for the negative one is `2 * n + 1`.
///
/// # Examples
///
/// ```
/// use splr::types::*;
/// assert_eq!(2usize, Lit::from(-1i32).into());
/// assert_eq!(3usize, Lit::from( 1i32).into());
/// assert_eq!(4usize, Lit::from(-2i32).into());
/// assert_eq!(5usize, Lit::from( 2i32).into());
/// assert_eq!( 1i32, Lit::from( 1i32).into());
/// assert_eq!(-1i32, Lit::from(-1i32).into());
/// assert_eq!( 2i32, Lit::from( 2i32).into());
/// assert_eq!(-2i32, Lit::from(-2i32).into());
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Lit {
    /// literal encoded into folded u32
    ordinal: u32,
}

/// a dummy literal.
pub const NULL_LIT: Lit = Lit { ordinal: 0 };

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}L", i32::from(self))
    }
}

/// convert literals to `[i32]` (for debug).
pub fn i32s(v: &[Lit]) -> Vec<i32> {
    v.iter().map(|l| i32::from(*l)).collect::<Vec<_>>()
}

impl From<(VarId, bool)> for Lit {
    #[inline]
    fn from((vi, b): (VarId, bool)) -> Self {
        Lit {
            ordinal: ((vi as u32) * 2) + (b as u32),
        }
    }
}

impl From<(VarId, Option<bool>)> for Lit {
    #[inline]
    fn from((vi, ob): (VarId, Option<bool>)) -> Self {
        match ob {
            None => NULL_LIT,
            Some(b) => Lit::from((vi, b)),
        }
    }
}

impl From<usize> for Lit {
    #[inline]
    fn from(l: usize) -> Self {
        Lit { ordinal: l as u32 }
    }
}

impl From<i32> for Lit {
    #[inline]
    fn from(x: i32) -> Self {
        Lit {
            ordinal: (if x < 0 { -2 * x } else { 2 * x + 1 }) as u32,
        }
    }
}

impl From<ClauseId> for Lit {
    #[inline]
    fn from(cid: ClauseId) -> Self {
        Lit {
            ordinal: cid.ordinal & 0x7FFF_FFFF,
        }
    }
}

/*
/// While Lit::ordinal is private, Var::{index, assign} are public.
/// So we define the following here.
/// # CAVEAT
/// Unassigned vars are converted to the null literal.
impl From<&Var> for Lit {
    fn from(v: &Var) -> Self {
        match v.assign {
            Some(true) => Lit {
                ordinal: (v.index as u32) << 1 | 1 as u32,
            },
            Some(false) => Lit {
                ordinal: (v.index as u32) << 1,
            },
            None => NULL_LIT,
        }
    }
}

impl From<&mut Var> for Lit {
    fn from(v: &mut Var) -> Self {
        Lit {
            ordinal: match v.assign {
                Some(true) => (v.index as u32) << 1 | 1 as u32,
                _e => (v.index as u32) << 1,
            },
        }
    }
}
*/

impl From<Lit> for bool {
    /// - positive Lit (= even u32) => Some(true)
    /// - negative Lit (= odd u32)  => Some(false)
    #[inline]
    fn from(l: Lit) -> bool {
        (l.ordinal & 1) != 0
    }
}

impl From<Lit> for ClauseId {
    #[inline]
    fn from(l: Lit) -> ClauseId {
        ClauseId {
            ordinal: l.ordinal | 0x8000_0000,
        }
    }
}

impl From<Lit> for usize {
    #[inline]
    fn from(l: Lit) -> usize {
        l.ordinal as usize
    }
}

impl From<Lit> for i32 {
    #[inline]
    fn from(l: Lit) -> i32 {
        if l.ordinal % 2 == 0 {
            ((l.ordinal >> 1) as i32).neg()
        } else {
            (l.ordinal >> 1) as i32
        }
    }
}

impl From<&Lit> for i32 {
    #[inline]
    fn from(l: &Lit) -> i32 {
        if l.ordinal % 2 == 0 {
            ((l.ordinal >> 1) as i32).neg()
        } else {
            (l.ordinal >> 1) as i32
        }
    }
}

impl Not for Lit {
    type Output = Lit;
    #[inline]
    fn not(self) -> Self {
        Lit {
            ordinal: self.ordinal ^ 1,
        }
    }
}

impl Index<Lit> for [bool] {
    type Output = bool;
    #[inline]
    fn index(&self, l: Lit) -> &Self::Output {
        unsafe { self.get_unchecked(usize::from(l)) }
    }
}

impl IndexMut<Lit> for [bool] {
    #[inline]
    fn index_mut(&mut self, l: Lit) -> &mut Self::Output {
        unsafe { self.get_unchecked_mut(usize::from(l)) }
    }
}

impl Index<Lit> for Vec<bool> {
    type Output = bool;
    #[inline]
    fn index(&self, l: Lit) -> &Self::Output {
        unsafe { self.get_unchecked(usize::from(l)) }
    }
}

impl IndexMut<Lit> for Vec<bool> {
    #[inline]
    fn index_mut(&mut self, l: Lit) -> &mut Self::Output {
        unsafe { self.get_unchecked_mut(usize::from(l)) }
    }
}

impl Index<Lit> for Vec<Vec<Watch>> {
    type Output = Vec<Watch>;
    #[inline]
    fn index(&self, l: Lit) -> &Self::Output {
        unsafe { self.get_unchecked(usize::from(l)) }
    }
}

impl IndexMut<Lit> for Vec<Vec<Watch>> {
    #[inline]
    fn index_mut(&mut self, l: Lit) -> &mut Self::Output {
        unsafe { self.get_unchecked_mut(usize::from(l)) }
    }
}

/// # Examples
///
/// ```
/// use splr::types::*;
/// assert_eq!(Lit::from(1i32), Lit::from_assign(1 as VarId, true));
/// assert_eq!(Lit::from(2i32), Lit::from_assign(2 as VarId, true));
/// assert_eq!(1, Lit::from_assign(1, true).vi());
/// assert_eq!(1, Lit::from_assign(1, false).vi());
/// assert_eq!(2, Lit::from_assign(2, true).vi());
/// assert_eq!(2, Lit::from_assign(2, false).vi());
/// assert_eq!(Lit::from( 1i32), !Lit::from(-1i32));
/// assert_eq!(Lit::from(-1i32), !Lit::from( 1i32));
/// assert_eq!(Lit::from( 2i32), !Lit::from(-2i32));
/// assert_eq!(Lit::from(-2i32), !Lit::from( 2i32));
/// ```
impl LitIF for Lit {
    #[inline]
    fn as_bool(self) -> bool {
        self.ordinal & 1 == 1
    }
    #[inline]
    fn from_assign(vi: VarId, p: bool) -> Lit {
        Lit {
            ordinal: (vi as u32) << 1 | (p as u32),
        }
    }
    #[inline]
    fn vi(self) -> VarId {
        (self.ordinal >> 1) as VarId
    }
    #[inline]
    fn is_none(self) -> bool {
        self.ordinal == 0
    }
}

/// API for Exponential Moving Average, EMA, like `get`, `reset`, `update` and so on.
pub trait EmaIF {
    /// the type of the argument of `update`.
    type Input;
    /// return the current value.
    fn get(&self) -> f64;
    /// reset internal data.
    fn reset(&mut self) {}
    /// catch up with the current state.
    fn update(&mut self, x: Self::Input);
    /// return a ratio of short / long statistics.
    fn trend(&self) -> f64 {
        unimplemented!()
    }
}

/// Exponential Moving Average, with a calibrator if feature `ema_calibration` is on.
#[derive(Debug, Clone, Copy)]
pub struct Ema {
    val: f64,
    #[cfg(feature = "ema_calibration")]
    cal: f64,
    sca: f64,
}

impl EmaIF for Ema {
    type Input = f64;
    #[cfg(not(feature = "ema_calibration"))]
    fn update(&mut self, x: Self::Input) {
        self.val = self.sca * x + (1.0 - self.sca) * self.val;
    }
    #[cfg(feature = "ema_calibration")]
    fn update(&mut self, x: Self::Input) {
        self.val = self.sca * x + (1.0 - self.sca) * self.val;
        self.cal = self.sca + (1.0 - self.sca) * self.cal;
    }
    #[cfg(feature = "ema_calibration")]
    fn get(&self) -> f64 {
        self.val / self.cal
    }
    #[cfg(not(feature = "ema_calibration"))]
    fn get(&self) -> f64 {
        self.val
    }
}

impl Ema {
    pub fn new(s: usize) -> Ema {
        Ema {
            val: 0.0,
            #[cfg(feature = "ema_calibration")]
            cal: 0.0,
            sca: 1.0 / (s as f64),
        }
    }
}

/// Exponential Moving Average pair, with a calibrator if feature `ema_calibration` is on.
#[derive(Debug, Clone, Copy)]
pub struct Ema2 {
    fast: f64,
    slow: f64,
    #[cfg(feature = "ema_calibration")]
    calf: f64,
    #[cfg(feature = "ema_calibration")]
    cals: f64,
    fe: f64,
    se: f64,
}

impl EmaIF for Ema2 {
    type Input = f64;
    fn get(&self) -> f64 {
        self.fast // / self.calf
    }
    #[cfg(not(feature = "ema_calibration"))]
    fn update(&mut self, x: Self::Input) {
        self.fast = self.fe * x + (1.0 - self.fe) * self.fast;
        self.slow = self.se * x + (1.0 - self.se) * self.slow;
    }
    #[cfg(feature = "ema_calibration")]
    fn update(&mut self, x: Self::Input) {
        self.fast = self.fe * x + (1.0 - self.fe) * self.fast;
        self.slow = self.se * x + (1.0 - self.se) * self.slow;
        self.calf = self.fe + (1.0 - self.fe) * self.calf;
        self.cals = self.se + (1.0 - self.se) * self.cals;
    }
    #[cfg(not(feature = "ema_calibration"))]
    fn reset(&mut self) {
        self.slow = self.fast;
    }
    #[cfg(feature = "ema_calibration")]
    fn reset(&mut self) {
        self.slow = self.fast;
        self.cals = self.calf;
    }
    #[cfg(not(feature = "ema_calibration"))]
    fn trend(&self) -> f64 {
        self.fast / self.slow
    }
    #[cfg(feature = "ema_calibration")]
    fn trend(&self) -> f64 {
        self.fast / self.slow * (self.cals / self.calf)
    }
}

impl Ema2 {
    pub fn new(f: usize) -> Ema2 {
        Ema2 {
            fast: 0.0,
            slow: 0.0,
            #[cfg(feature = "ema_calibration")]
            calf: 0.0,
            #[cfg(feature = "ema_calibration")]
            cals: 0.0,
            fe: 1.0 / (f as f64),
            se: 1.0 / (f as f64),
        }
    }
    // set secondary EMA's parameter
    pub fn with_slow(mut self, s: usize) -> Ema2 {
        self.se = 1.0 / (s as f64);
        self
    }
    pub fn get_slow(&self) -> f64 {
        self.slow // / self.calf
    }
}

/// Internal errors.
/// Note: returning `Result<(), a-singleton>` is identical to returning `bool`.
#[derive(Debug, Eq, PartialEq)]
pub enum SolverError {
    // StateUNSAT = 0,
    // StateSAT,
    IOError,
    Inconsistent,
    NullLearnt,
    OutOfMemory,
    OutOfRange,
    TimeOut,
    SolverBug,
    UndescribedError,
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A Return type used by solver functions.
pub type MaybeInconsistent = Result<(), SolverError>;

/// CNF locator
#[derive(Clone, Debug)]
pub enum CNFIndicator {
    /// not specified
    Void,
    /// from a file
    File(String),
    /// embedded directly
    LitVec(usize),
}

impl Default for CNFIndicator {
    fn default() -> Self {
        CNFIndicator::Void
    }
}

impl fmt::Display for CNFIndicator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CNFIndicator::Void => write!(f, "No CNF specified)"),
            CNFIndicator::File(file) => write!(f, "CNF file({})", file),
            CNFIndicator::LitVec(n) => write!(f, "A vec({} clauses)", n),
        }
    }
}

// impl CNFIndicator {
//     pub fn to_string(&self) -> String {
//         match self {
//             CNFIndicator::Void => "(no cnf)".to_string(),
//             CNFIndicator::File(f) => f.to_string(),
//             CNFIndicator::LitVec(v) => format!("(embedded {} element vector)", v.len()).to_string(),
//         }
//     }
// }

/// Data storage about a problem.
#[derive(Clone, Debug)]
pub struct CNFDescription {
    pub num_of_variables: usize,
    pub num_of_clauses: usize,
    pub pathname: CNFIndicator,
}

impl Default for CNFDescription {
    fn default() -> CNFDescription {
        CNFDescription {
            num_of_variables: 0,
            num_of_clauses: 0,
            pathname: CNFIndicator::Void,
        }
    }
}

impl fmt::Display for CNFDescription {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let CNFDescription {
            num_of_variables: nv,
            num_of_clauses: nc,
            pathname: path,
        } = &self;
        write!(f, "CNF({}, {}, {})", nv, nc, path)
    }
}

impl<V> From<&[V]> for CNFDescription
where
    V: AsRef<[i32]>,
{
    fn from(vec: &[V]) -> Self {
        let num_of_variables = vec
            .iter()
            .map(|clause| clause.as_ref().iter().map(|l| l.abs()).max().unwrap_or(0))
            .max()
            .unwrap_or(0) as usize;
        CNFDescription {
            num_of_variables,
            num_of_clauses: vec.len(),
            pathname: CNFIndicator::LitVec(vec.len()),
        }
    }
}

/// A wrapper structure to make a CNFDescription from a file.
/// To make CNFDescription clonable, a BufReader should be separated from it.
/// If you want to make a CNFDescription which isn't connected to a file,
/// just call CNFDescription::default() directly.
#[derive(Debug)]
pub struct CNFReader {
    pub cnf: CNFDescription,
    pub reader: BufReader<File>,
}

impl TryFrom<&str> for CNFReader {
    type Error = SolverError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        CNFReader::try_from(&PathBuf::from(s))
    }
}

impl TryFrom<&PathBuf> for CNFReader {
    type Error = SolverError;
    fn try_from(path: &PathBuf) -> Result<Self, Self::Error> {
        let pathname = if path.to_string_lossy().is_empty() {
            "--".to_string()
        } else {
            Path::new(&path.to_string_lossy().into_owned())
                .file_name()
                .map_or("aStrangeNamed".to_string(), |f| {
                    f.to_string_lossy().into_owned()
                })
        };
        let fs = File::open(path).map_or(Err(SolverError::IOError), Ok)?;
        let mut reader = BufReader::new(fs);
        let mut buf = String::new();
        let mut nv: usize = 0;
        let mut nc: usize = 0;
        let mut found_valid_header = false;
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_k) => {
                    let mut iter = buf.split_whitespace();
                    if iter.next() == Some("p") && iter.next() == Some("cnf") {
                        if let Some(v) = iter.next().map(|s| s.parse::<usize>().ok().unwrap()) {
                            if let Some(c) = iter.next().map(|s| s.parse::<usize>().ok().unwrap()) {
                                nv = v;
                                nc = c;
                                found_valid_header = true;
                                break;
                            }
                        }
                    }
                    continue;
                }
                Err(e) => {
                    println!("{}", e);
                    return Err(SolverError::IOError);
                }
            }
        }
        if !found_valid_header {
            return Err(SolverError::IOError);
        }
        let cnf = CNFDescription {
            num_of_variables: nv,
            num_of_clauses: nc,
            pathname: CNFIndicator::File(pathname),
        };
        Ok(CNFReader { cnf, reader })
    }
}

impl<T> Delete<T> for Vec<T> {
    fn delete_unstable<F>(&mut self, mut filter: F)
    where
        F: FnMut(&T) -> bool,
    {
        let mut i = 0;
        while i != self.len() {
            if filter(&mut self[i]) {
                self.swap_remove(i); // self.remove(i) for stable deletion
                break;
            } else {
                i += 1;
            }
        }
    }
}

/// API for object properties.
pub trait FlagIF {
    /// return true if the flag in on.
    fn is(&self, flag: Flag) -> bool;
    /// set the flag.
    fn set(&mut self, f: Flag, b: bool);
    /// toggle the flag off.
    fn turn_off(&mut self, flag: Flag);
    /// toggle the flag on.
    fn turn_on(&mut self, flag: Flag);
}

bitflags! {
    /// Misc flags used by [`Clause`](`crate::cdb::Clause`) and [`Var`](`crate::assign::Var`).
    pub struct Flag: u16 {

        //
        //## For Clause
        //
        /// a clause is stored in DB, but is a garbage now.
        const DEAD         = 0b0000_0000_0000_0001;
        /// a clause is a generated clause by conflict analysis and is removable.
        const LEARNT       = 0b0000_0000_0000_0010;
        /// a clause is used recently in conflict analysis.
        const JUST_USED    = 0b0000_0000_0000_0100;
        /// a clause is registered in vars' occurrence list.
        const OCCUR_LINKED = 0b0000_0000_0000_1000;
        /// a clause or var is enqueued for eliminator.
        const ENQUEUED     = 0b0000_0000_0001_0000;
        /// mark to run garbage collector on the corresponding watcher lists
        const TOUCHED      = 0b0000_0000_0010_0000;
        /// for vivified clauses
        const VIVIFIED     = 0b0000_0000_0100_0000;
        /// for a clause which decreases LBD twice after vivification
        const VIVIFIED2    = 0b0000_0000_1000_0000;
        /// a given clause derived a learnt which LBD is smaller than 20.
        const DERIVE20     = 0b0010_0000_0000_0000;
        /// a temporal clause during vivification
        const VIV_ASSUMED  = 0b0100_0000_0000_0000;

        //
        //## For Var
        //
        /// a var is eliminated and managed by eliminator.
        const ELIMINATED   = 0b0000_0001_0000_0000;
        /// a var is checked during in the current conflict analysis.
        const CA_SEEN      = 0b0000_0010_0000_0000;
        /// the previous assigned value of a Var.
        const PHASE        = 0b0000_0100_0000_0000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cnf() {
        if let Ok(reader) = CNFReader::try_from("tests/sample.cnf") {
            assert_eq!(reader.cnf.num_of_variables, 250);
            assert_eq!(reader.cnf.num_of_clauses, 1065);
        } else {
            panic!("failed to load tests/sample.cnf");
        }
    }
}
