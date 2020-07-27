A modern SAT Solver for Propositional Logic in Rust
----

Splr is a pure [Rust](https://www.rust-lang.org)ic modern SAT solver, based on [Glucose 4.1](https://www.labri.fr/perso/lsimon/glucose/).
It adopts various research results on SAT solvers:

- *CDCL*, *watch literals*, *LBD* and so on from Glucose, [Minisat](http://minisat.se) and the ancestors
- Glucose-like *dynamic blocking/forcing restarts* based on [EMAs](https://arxiv.org/abs/1506.08905)
- pre/in-process simplification based on clause subsumption and variable elimination
- compile-time selection of a variant of *Learning Rate Based Branching* with *Reason Side Rewarding* and EVSIDS
- *chronological backtrack* aka *chronoBT*
- Glucose-like heuristics adaptation
- [CaDiCaL](https://github.com/arminbiere/cadical)-like extended phase saving
- CaDiCaL-like search stabilization (not activated by default)
- Clause vivification as pre/in-processor

*Many thanks to SAT researchers.*

Please check [ChangeLog](ChangeLog.md) about recent updates.

## Correctness

Though Splr comes with **ABSOLUTELY NO WARRANTY**, I'd like to show some results.

#### Version 0.4.1

* [SAT Race 2019](http://sat-race-2019.ciirc.cvut.cz), [Benchmarks](http://satcompetition.org/sr2019benchmarks.zip),  splr-0.4.1(be30d17, 7064c9) solved with a 400 sec (soft) timeout:
  * 48 satisfiable problems: all the solutions were correct.
  * 7 unsatisfiable problems: all were verified with [Grad](https://www21.in.tum.de/~lammich/grat/).

![](https://user-images.githubusercontent.com/997855/82614843-c14b6480-9c03-11ea-9fe9-1a4d367d7290.png)

## Install

Just run `cargo install splr --features cli` after installing the latest [cargo](https://www.rust-lang.org/tools/install).
Two executables will be installed:

- `splr` -- the solver
- `dmcr` -- a very simple model checker to verify a *satisfiable* assignment set which was generated by `splr`.

## Usage

Splr is a standalone program, taking a CNF file. The result will be saved to a file, which format is
defined by [SAT competition 2011 rules](http://www.satcompetition.org/2011/rules.pdf).

```plain
$ splr tests/sample.cnf
sample.cnf                                         250,1065 |time:     1.24
 #conflict:      38892, #decision:        47095, #propagate:          85990
  Assignment|#rem:      243, #fix:        1, #elm:        6, prg%:   2.8000
      Clause|Remv:    19886, LBD2:      114, Binc:        0, Perm:     1056
   Stabilize|#BLK:      257, #RST:      512, tASG:   1.3309, tLBD:   0.9605
    Conflict|eLBD:     9.27, cnfl:    12.84, bjmp:    11.83, rpc%:   1.3165
        misc|#stb:        0, #smp:        1, 2smp:    36681, vdcy:   0.9800
    Strategy|mode: Initial search phase before a main strategy
      Result|file: ./.ans_sample.cnf
s SATISFIABLE: tests/sample.cnf
```

```plain
$ cat .ans_sample.cnf
c This file was generated by splr-0.4.1 for tests/sample.cnf
c
c CNF file(sample.cnf), #var:      250, #cls:     1065
c  #conflict:      38892, #decision:        47095, #propagate:          85990
c   Assignment|#rem:      243, #fix:        1, #elm:        6, prg%:   2.8000
c       Clause|Remv:    19886, LBD2:      114, Binc:        0, Perm:     1056
c      Restart|#BLK:      257, #RST:      512, eASG:   1.3309, eLBD:   0.9605
c     Conflict|eLBD:     9.27, cnfl:    12.84, bjmp:    11.83, rpc%:   1.3190
c         misc|#stb:        0, #smp:        1, 2smp:    36681, vdcy:   0.9800
c     Strategy|mode:        Initial, time:     1.24
c
s SATISFIABLE
v 1 2 3 4 -5 6 7 -8 -9 10 11 -12 -13 -14 15 16 -17 18 ... 0
```

```plain
$ dmcr tests/sample.cnf
A valid assignment set for tests/sample.cnf is found in .ans_sample.cnf.
```

If you want to certificate unsatisfiability, use `splr --certificate` and recommend to use [Grid](https://www21.in.tum.de/~lammich/grat/).

1. Run splr with certificate option.

```plain
$ splr -c tests/unsat.cnf
unsat.cnf                                            83,570 |time:     0.00
 #conflict:          0, #decision:            0, #propagate:              0
  Assignment|#rem:       19, #fix:       64, #elm:        0, prg%:  77.1084
      Clause|Remv:        0, LBD2:        0, Binc:      126, Perm:      127
     Restart|#BLK:        0, #RST:        0, tASG:      NaN, tLBD:      NaN
    Conflict|eLBD:     0.00, cnfl:     0.00, bjmp:     0.00, rpc%:      NaN
        misc|#stb:        0, #smp:        0, 2smp:    40000, vdcy:   0.7500
    Strategy|mode: Initial search phase before a main strategy
      Result|file: ./.ans_unsat.cnf
 Certificate|file: proof.out
s UNSATISFIABLE: tests/unsat.cnf
```

2. Trim comments from the output

```plain
$ egrep -v '^[cs]' < proof.out > proof.drat
```

3. Convert the drat file to a grat file.

```plain
$ gratgen tests/unsat.cnf proof.drat -o proof.grat
c sizeof(cdb_t) = 4
c sizeof(cdb_t*) = 8
c Using RAT run heuristics
c Parsing formula ... 0ms
c Parsing proof (ASCII format) ... 0ms
c Forward pass ... 0ms
c Starting Backward pass
c Single threaded mode
c Waiting for aux-threads ...done
c Lemmas processed by threads: 0 mdev: nan
c Finished Backward pass: 0ms
c Writing combined proof ... 0ms
s VERIFIED
c Timing statistics (ms)
c Parsing:  1
c Checking: 0
c   * bwd:  0
c Writing:  0
c Overall:  2
c   * vrf:  2

c Lemma statistics
c RUP lemmas:  0
c RAT lemmas:  0
c   RAT run heuristics:   0
c Total lemmas:  0

c Size statistics (bytes)
c Number of clauses: 981
c Clause DB size:  25372
c Item list:       15696
c Pivots store:    4096
```

4. Verify it with `gratchk`

```plain
$ gratchk unsat tests/unsat.cnf proof.grat
gratchk unsat tests/unsat.cnf proof.grat gratchktests/unsat.cnf proof.grat
c Reading cnf
c Reading proof
c Done
c Verifying unsat
s VERIFIED UNSAT
$
```

### Calling Splr from Rust programs

Since 0.4.0, you can use Splr in your programs.

```rust
use splr::*;
use std::convert::TryFrom;

fn main() {
    let v: Vec<Vec<i32>> = vec![vec![1, 2], vec![-1, 3], vec![1, -3], vec![-1, 2]];
    match Certificate::try_from(v) {
        Ok(Certificate::SAT(ans)) => println!("s SATISFIABLE: {:?}", ans),
        Ok(Certificate::UNSAT) => println!("s UNSATISFIABLE"),
        Err(e) => panic!("s UNKNOWN; {}", e),
    }
}
```

### All solutions SAT solver

```rust
use splr::*;
use std::{convert::TryFrom, env::args};

fn main() {
    let cnf = args().nth(1).expect("takes an arg");
    let assigns: Vec<i32> = Vec::new();
    println!("#solutions: {}", run(&cnf, &assigns));
}

#[cfg(feature = "incremental_solver")]
fn run(cnf: &str, assigns: &[i32]) -> usize {
    let mut solver = Solver::try_from(cnf).expect("panic at loading a CNF");
    for n in assigns.iter() {
        solver.add_assignment(*n).expect("panic at assertion");
    }
    let mut count = 0;
    loop {
        match solver.solve() {
            Ok(Certificate::SAT(ans)) => {
                count += 1;
                println!("s SATISFIABLE({}): {:?}", count, ans);
                let ans = ans.iter().map(|i| -i).collect::<Vec<i32>>();
                match solver.add_clause(ans) {
                    Err(SolverError::Inconsistent) => {
                        println!("c no answer due to level zero conflict");
                        break;
                    }
                    Err(e) => {
                        println!("s UNKNOWN; {:?}", e);
                        break;
                    }
                    Ok(_) => solver.reset(),
                }
            }
            Ok(Certificate::UNSAT) => {
                println!("s UNSATISFIABLE");
                break;
            }
            Err(e) => {
                println!("s UNKNOWN; {}", e);
                break;
            }
        }
    }
    count
}
```

Since 0.4.1, `Solver` has `iter()`. So you can iterate on satisfiable '`solution: Vec<i32>`'s as:

```rust
#[cfg(feature = "incremental_solver")]
for (i, v) in Solver::try_from(cnf).expect("panic").iter().enumerate() {
    println!("{}-th answer: {:?}", i, v);
}
```

### Mnemonics used in the progress message

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
| `#stb` | the number of stabilization mode flips |
| `#smp` | the number of `clause and var simplification` invocations |
| `2smp` | the number of literals to invoke the simplifier again |
| `vdcy` | var activity decay rate |
| `mode` | Selected strategy's id |
| `time` | the elapsed CPU time in seconds |

## Command line options

Please check the help message.

* The 'switch' in help message below is either '1' or '0' to or not to use a module.
* Splr can't handle compressed CNF files so far.

```plain
$ splr --help
splr 0.4.1
Narazaki Shuji <shujinarazaki@protonmail.com>
A modern CDCL SAT solver in Rust

USAGE:
    splr [FLAGS] [OPTIONS] <cnf-file>

FLAGS:
    -h, --help        Prints help information
    -C, --no-color    Disable coloring
    -q, --quiet       Disable any progress message
    -c, --certify     Writes a DRAT UNSAT certification file
    -l, --log         Uses Glucose-like progress report
    -V, --version     Prints version information

OPTIONS:
        --ADP <adaptive>          Strategy adaptation switch [default: 1]
        --cbt <cbt-thr>           Level threshold to use chronoBT [default: 100]
        --cl <clause-limit>       Soft limit of #clauses (6MC/GB) [default: 0]
        --stat <dump-int>         Interval for dumping stat data [default: 0]
        --PRO <elim>              Pre/in-processor switch [default: 1]
        --ecl <elim-cls-lim>      Max #lit for clause subsume [default: 100]
        --evl <elim-grw-lim>      Grow limit of #cls in var elim [default: 0]
        --et <elim-trigger>       #cls to start simplification [default: 40000]
        --evo <elim-var-occ>      Max #cls for var elimination [default: 10000]
    -o, --dir <output-dir>        Output directory [default: .]
    -p, --proof <proof-file>      Cert. file in DRAT format [default: proof.out]
        --RDC <reduce>            Clause reduction switch [default: 1]
        --RPH <rephase>           Rephase switch [default: 1]
    -r, --result <result-file>    Result filename/stdout [default: ]
        --RSR <rsr>               Reason-Side Rewarding switch [default: 1]
        --ral <rst-asg-len>       Length for assignment average [default: 3500]
        --rab <rst-asg-thr>       Blocking restart threshold [default: 1.40]
        --rll <rst-lbd-len>       Length of LBD fast EMA [default: 50]
        --rls <rst-lbd-slw>       Length of LBD slow EMA [default: 10000]
        --rlt <rst-lbd-thr>       Forcing restart threshold [default: 0.70]
        --rss <rst-stb-scl>       Stabilizer scaling [default: 2.0]
        --rs <rst-step>           #conflicts between restarts [default: 50]
        --STB <stabilize>         Stabilization switch [default: 1]
    -t, --timeout <timeout>       CPU time limit in sec [default: 5000.0]
        --vri <vrw-dcy-beg>       Initial var reward decay [default: 0.75]
        --vrm <vrw-dcy-end>       Maximum var reward decay [default: 0.98]

ARGS:
    <cnf-file>    CNF file in DIMACS format
```

## License

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.

----
2020, Narazaki Shuji
