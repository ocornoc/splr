#![doc(html_root_url = "https://docs.rs/splr/0.3.0")]
/*!
# a SAT Solver for Propositional Logic in Rust

Splr is a pure Rustic SAT solver, based on [Glucose 4.1](https://www.labri.fr/perso/lsimon/glucose/).
It adopts various research results on SAT solvers:

- CDCL, watch literals, and so on from [Minisat](http://minisat.se) and the ancestors
- Glucose-like dynamic blocking/forcing restarts based on [EMAs](https://arxiv.org/abs/1506.08905)
- heuristics adaptation
- pre/in-process simplification based on clause subsumption and variable elimination
- Learning Rate Based Branching and Reason Side Rewarding

*Many thanks to SAT researchers.*

## Usage

Splr is a standalone program, taking a CNF file. The result will be saved to a file.

```plain
$ splr tests/sample.cnf
sample.cnf                                         250,1065 |time:     0.24
 #conflict:      12273, #decision:        13676, #propagate:          25950
  Assignment|#rem:      243, #fix:        1, #elm:        6, prg%:   2.8000
      Clause|Remv:     2337, LBD2:       46, Binc:        0, Perm:     1056
     Restart|#BLK:      100, #RST:        0, tASG:   1.1967, tLBD:   1.0378
    Conflict|eLBD:    11.92, cnfl:    18.87, bjmp:    17.84, rpc%:   0.0000
        misc|#rdc:        9, #sce:        2, stag:        0, vdcy:   0.9292
    Strategy|mode: Initial search phase before a main strategy
      Result|file: ./.ans_sample.cnf
SATISFIABLE: tests/sample.cnf

$ cat .ans_sample.cnf
c An assignment set generated by splr-0.3.0 for tests/sample.cnf
c
c sample.cnf                                 , #var:      250, #cls:     1065
c  #conflict:      17792, #decision:        20650, #propagate:          38443
c   Assignment|#rem:      243, #fix:        1, #elm:        6, prg%:   2.8000
c       Clause|Remv:    11307, LBD2:       52, Binc:        0, Perm:     1056
c      Restart|#BLK:      213, #RST:        0, eASG:   1.3606, eLBD:   1.0145
c     Conflict|eLBD:    11.00, cnfl:    15.80, bjmp:    14.65, rpc%:   0.0000
c         misc|#rdc:        3, #sce:        2, stag:        0, vdcy:      0.0
c     Strategy|mode:        initial, time:     0.36
c
s SATISFIABLE
1 2 -3 4 -5 6 7 8 9 -10 11 12 13 14 -15 16 -17 18 19 20 21 -22 23 ... 0

$ dmcr tests/sample.cnf
A valid assignment set for tests/sample.cnf is found in .ans_sample.cnf.
```

The answer file uses the following format.

- It contains a single line starting with `s` and followed by `SATISFIABLE` or `UNSATISFIABLE`.
- It ends a line of assignments separated by a space and `0` as EOL, if the problem is satisfiable.
  Otherwise it contains only `0`.
- Lines starting with `c` are comments, used for dumping statistics

### Mnemonics in progress message

| mnemonic  | meaning |
| --------- |------- |
| `v`  | the number of variables used in the given CNF file |
| `c`  | the number of clauses used in the given CNF file |
| `time`  | elapsed CPU time in seconds (or wall-clock time if CPU time is not available) |
| `#conflict` | the number of conflicts |
| `#decision` | the number of decisions |
| `#propagate` | the number of propagates (its unit is literal) |
| `#rem` | the number of remaining variables |
| `#fix` | the number of solved variables (which has been assigned a value at decision level zero) |
| `#elm` | the number of eliminated variables |
| `prg%` | the percentage of `remaining variables / total variables` |
| `Remv` | the number of learnt clauses which are not biclauses |
| `LBD2` | the number of learnt clauses which LBDs are 2 |
| `Binc` | the number of binary learnt clauses |
| `Perm` | the number of given clauses and binary learnt clauses |
| `#BLK` | the number of blocking restart |
| `#RST` | the number of restart |
| `tASG` | the trend rate of the number of assigned variables |
| `tLBD` | the trend rate of learn clause's LBD |
| `eLBD` | the EMA, Exponential Moving Average, of learn clauses' LBDs |
| `cnfl` | the EMA of decision levels to which backjumps go |
| `bjmp` | the EMA of decision levels at which conflicts occur |
| `rpc%` | a percentage of restart per conflict |
| `#rdc` | the number of `reduce` invocations |
| `#sce` | the number of satisfied clause eliminations done by `simplify` |
| `stag` | the number of stagnated periods (no progress in 10,000 conflicts) |
| `vdcy` | var activity decay rate |
| `mode` | Selected strategy's id |
| `time` | the elapsed CPU time in seconds |

## Command line options

Please check help message.

```plain
$ splr --help
splr 0.3.0
Narazaki Shuji <shujinarazaki@protonmail.com>
A pure rustic CDCL SAT solver based on Glucose

USAGE:
    splr [FLAGS] [OPTIONS] <cnf-filename>

FLAGS:
    -h, --help                         Prints help information
    -c, --certify                      Writes a DRAT UNSAT certification file
    -l, --log                          Uses Glucose-like progress report
    -V, --version                      Prints version information
    -S, --without-adaptive-strategy    Disables dynamic strategy adaptation
    -D, --without-deep-search          Disables deep search mode
    -E, --without-elim                 Disables exhaustive simplification

OPTIONS:
        --cl <clause-limit>           soft limit of #clauses (6MC/GB) [default: 0]
        --stat <dump-interval>        interval for dumpping stat data [default: 0]
        --eg <elim-grow-limit>        grow limit of #clauses by v-elim [default: 4]
        --el <elim-lit-limit>         #literals in a clause by v-elim [default: 64]
    -o, --dir <output-dirname>        output directory [default: .]
    -p, --proof <proof-filename>      filename for DRAT cert [default: proof.out]
        --ra <restart-asg-len>        length for assignment average [default: 3500]
        --rb <restart-blocking>       blocking restart threshold [default: 1.40]
        --rl <restart-lbd-len>        length for LBD average [default: 50]
        --rs <restart-step>           #conflicts between restarts [default: 50]
        --rt <restart-threshold>      forcing restart threshold [default: 0.70]
    -r, --result <result-filename>    result filename/stdout [default: ]
        --to <timeout>                CPU time limit in sec [default: 0]

ARGS:
    <cnf-filename>    a DIMACS format CNF file
```

## Correctness

While Splr comes with **ABSOLUTELY NO WARRANTY**, Splr version 0.1.0 (splr-0.1.0) was verified with the following problems:

* The first 100 problems from
  [SATLIB](https://www.cs.ubc.ca/~hoos/SATLIB/benchm.html),
  [250 variables uniform random satisfiable 3-SAT](https://www.cs.ubc.ca/~hoos/SATLIB/Benchmarks/SAT/RND3SAT/uf250-1065.tar.gz)
  : all the solutions are correct.
* The first 100 problems from
  [SATLIB](https://www.cs.ubc.ca/~hoos/SATLIB/benchm.html),
  [250 variables uniform random unsatisfiable 3-SAT](https://www.cs.ubc.ca/~hoos/SATLIB/Benchmarks/SAT/RND3SAT/uuf250-1065.tar.gz)
  : all the solutions are correct and verified with [drat-trim](http://www.cs.utexas.edu/~marijn/drat-trim/).
* [SAT Competition 2017](https://baldur.iti.kit.edu/sat-competition-2017/index.php?cat=tracks),
  [Main track](https://baldur.iti.kit.edu/sat-competition-2017/benchmarks/Main.zip)
  : with a 2000 sec timeout, splr-0.1.0 solved:
  * 72 satisfiable problems: all the solutions are correct.
  * 51 unsatisfiable problems: [Lingeling](http://fmv.jku.at/lingeling/) or Glucose completely returns the same result. And,
     * 37 certificates generated by splr-0.1.1 were verified with drat-trim.
     * The remaining 14 certificates weren't able to be verified due to [timeout](https://gitlab.com/satisfiability01/splr/issues/74#note_142021555) by drat-trim.
*/
// /// Subsumption-based clause/var elimination
/// Crate `clause` provides `clause` object and its manager `ClauseDB`
pub mod clause;
/// Crate `config` provides solver's configuration and CLI.
pub mod config;
/// Crate `eliminator` implments clause subsumption and var elimination.
pub mod eliminator;
/// Crate `propagator` implements Boolean Constraint Propagation and decision var selection.
pub mod propagator;
/// Crate `restart` provides restart heuristics.
pub mod restart;
/// Crate `solver` provides the top-level API as a SAT solver.
pub mod solver;
/// Crate `state` is a collection of internal data.
pub mod state;
/// Crate `types` provides various building blocks, including
/// some common traits.
pub mod types;
/// Crate `validator` implements a model checker.
pub mod validator;
/// Crate `var` provides `var` object and its manager `VarDB`.
pub mod var;

#[macro_use]
extern crate bitflags;
