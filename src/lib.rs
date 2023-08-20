//! Benchmark, query, and compare Rust performance.

#![allow(dead_code)]

use anyhow::{bail, Result};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Row, Table};
use itertools::Itertools;
use std::{
    arch::x86_64,
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::{self, Debug, Display},
    hash::Hash,
    hint::black_box,
    mem,
    ops::Div,
    rc::Rc,
};
use Stat::*;

#[derive(Debug, Clone)]
pub struct Qry<L>
where
    L: Label,
{
    /// Run benchmarks from one or more labels.
    pub frm: Vec<Vec<L>>,
    /// Group benchmarks into one or more labels. Each label is a group.
    pub grp: Option<Vec<Vec<L>>>,
    /// Sort benchmarks by a struct label.
    pub srt: Option<L>,
    /// Apply a statisitcal function to benchmark results.
    pub sta: Option<Stat>,
    /// Transpose groups to series with the specified struct label.
    pub trn: Option<L>,
    /// Compare pairs of benchmarks as a ratio of max/min.
    pub cmp: bool,
    /// Set the number of iterations for each benchmark function.
    pub itr: u32,
}

// A benchmark function study.
pub struct Stdy<L>
where
    L: Label,
{
    /// A seed id given to inserted benchmark functions.
    pub id: RefCell<u16>,
    /// Labels mapped to benchmark ids.
    ///
    /// HashSets are used to perform search intersections.
    pub ids: RefCell<HashMap<L, HashSet<u16>>>,
    /// Benchmark ids mapped to benchmark functions.
    #[allow(clippy::type_complexity)]
    pub ops: RefCell<HashMap<u16, Op<L>>>,
}
impl<L> Stdy<L>
where
    L: Label,
{
    // Returns a new set of benchmark functions.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Stdy {
            id: RefCell::new(0),
            ids: RefCell::new(HashMap::new()),
            ops: RefCell::new(HashMap::new()),
        }
    }

    /// Returns a section.
    ///
    /// Useful for appending redundant labels.
    pub fn sec(&self, lbls: &[L]) -> Sec<L> {
        Sec::new(lbls, Rc::new(RefCell::new(self)))
    }

    /// Queries the set.
    ///
    /// Results are printed to the console.
    pub fn qry(&self, qry: Qry<L>) -> Result<()> {
        // dbg.then(|| println!("{:?}", qry));

        // Query benchmark functions.
        match self.frm(&qry.frm) {
            None => {
                println!("No matches")
            }
            Some(frm) => {
                // dbg.then(|| println!("{:?}", frm));

                // Run benchmark functions.
                let mut run = frm.run(qry.itr, &qry.srt, &qry.sta)?;
                // dbg.then(|| println!("{:?}", run));
                match &qry.grp {
                    None => {
                        println!("{}", run);
                    }
                    Some(qry_grp) => {
                        // Group benchmark results.
                        let grps = run.grp(qry_grp, &qry.srt)?;
                        // dbg.then(|| println!("{:?}", grps));

                        match &qry.trn {
                            None => {
                                println!("{}", grps);
                            }
                            Some(trn_lbl) => {
                                // Transpose groups to series.
                                let sers = grps.ser(*trn_lbl)?;
                                // dbg.then(|| println!("{:?}", sers));
                                if !qry.cmp {
                                    println!("{}", sers);
                                } else {
                                    let cmps = sers.cmp()?;
                                    // dbg.then(|| println!("{:?}", cmps));
                                    println!("{}", cmps);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Insert a benchmark function to the set.
    pub fn ins<F, O>(&self, lbls: &[L], mut f: F) -> Result<()>
    where
        F: FnMut() -> O,
        F: 'static,
    {
        if lbls.is_empty() {
            bail!("missing label: parameter 'lbls' is empty");
        }

        // Capture the benchmark function in a closure.
        // Enables the benchmark function to return a genericaly typed value
        // while benchmark returns a single timestamp value.
        // Returning a value from the benchmark function, in coordination with `black_box()`,
        // disallows the compiler from optimizing away inner logic.
        // Returns `FnMut() -> u64` to enable selecting and running benchmark functions.
        let fnc = Rc::new(RefCell::new(move || {
            // Avoid compiler over-optimization of benchmark functions by using `black_box(f())`.
            //  Explanation of how black_box works with LLVM ASM and memory.
            //      https://github.com/rust-lang/rust/blob/6a944187fb917393c9c6c39825dec3c1de29787c/compiler/rustc_codegen_llvm/src/intrinsic.rs#L339
            // `black_box` call from rust benchmark.
            //      https://github.com/rust-lang/rust/blob/cb6ab9516bbbd3859b56dd23e32fe41600e0ae02/library/test/src/lib.rs#L628
            // Record cpu cycles with assembly instructions.
            let fst = fst_cpu_cyc();
            black_box(f());
            lst_cpu_cyc() - fst
        }));

        let id = *self.id.borrow();

        // Insert a benchmark function id for each label.
        let mut ids = self.ids.borrow_mut();
        for lbl in lbls.clone() {
            let lbl_ids = ids.entry(*lbl).or_insert(HashSet::new());
            lbl_ids.insert(id);
        }

        // Insert the benchmark function.
        self.ops.borrow_mut().insert(id, Op::new(lbls, fnc));

        // Increment the id for the next insert call.
        *self.id.borrow_mut() += 1;

        Ok(())
    }

    /// Insert a benchmark function which is manually timed.
    ///
    /// The caller is expected to call `start()` and `stop()` functions
    /// on the specified `Tme` parameter.
    pub fn ins_prm<F, O>(&self, lbls: &[L], mut f: F) -> Result<()>
    where
        F: FnMut(Rc<RefCell<Tme>>) -> O,
        F: 'static,
    {
        if lbls.is_empty() {
            bail!("missing label: parameter 'lbls' is empty");
        }

        // Capture the benchmark function in a closure.
        // Enables the benchmark function to return a genericaly typed value
        // while benchmark returns a single timestamp value.
        // Returning a value from the benchmark function, in coordination with `black_box()`,
        // disallows the compiler from optimizing away inner logic.
        // Returns `FnMut() -> u64` to enable selecting and running benchmark functions.
        let fnc = Rc::new(RefCell::new(move || {
            // Avoid compiler over-optimization of benchmark functions by using `black_box(f())`.
            //  Explanation of how black_box works with LLVM ASM and memory.
            //      https://github.com/rust-lang/rust/blob/6a944187fb917393c9c6c39825dec3c1de29787c/compiler/rustc_codegen_llvm/src/intrinsic.rs#L339
            // `black_box` call from rust benchmark.
            //      https://github.com/rust-lang/rust/blob/cb6ab9516bbbd3859b56dd23e32fe41600e0ae02/library/test/src/lib.rs#L628
            // Record cpu cycles with assembly instructions.
            let tme = Rc::new(RefCell::new(Tme(0)));
            black_box(f(tme.clone()));
            let x = tme.borrow();
            x.0
        }));

        let id = *self.id.borrow();

        // Insert a benchmark function id for each label.
        let mut ids = self.ids.borrow_mut();
        for lbl in lbls.clone() {
            let lbl_ids = ids.entry(*lbl).or_insert(HashSet::new());
            lbl_ids.insert(id);
        }

        // Insert the benchmark function.
        self.ops.borrow_mut().insert(id, Op::new(lbls, fnc));

        // Increment the id for the next insert call.
        *self.id.borrow_mut() += 1;

        Ok(())
    }

    // Returns benchmark functions matching the specified labels.
    pub fn frm(&self, lblss: &Vec<Vec<L>>) -> Option<Frm<L>> {
        // Check for case where labels are empty.
        if lblss.is_empty() || lblss.iter().all(|x| x.is_empty()) {
            return None;
        }

        // Create return object.
        let mut ret = Frm::new(lblss);

        for lbls in lblss {
            // Gather benchmark ids by queried label.
            // Each label has a list of benchmark ids.
            // Ensure each id is present in each label list.
            let mut qry_lbl_ids: Vec<&HashSet<u16>> = Vec::new();
            let ids = self.ids.borrow();
            for lbl in lbls.iter() {
                if let Some(lbl_ids) = ids.get(lbl) {
                    qry_lbl_ids.push(lbl_ids);
                }
            }

            // Check for case where queried label
            // doesn't exist in root benchmark set.
            if qry_lbl_ids.len() != lbls.len() || qry_lbl_ids.is_empty() {
                // println!(
                //     "set.frm: qry_lbl_ids.len:{} != lbls.len:{}",
                //     qry_lbl_ids.len(),
                //     lbls.len()
                // );
                continue;
            }

            // Gather matched benchmark ids.
            // Intersect the id across each list for a match.
            // Find which benchmark ids are within each label list.
            let mut matched_ids: Vec<u16> = Vec::new();
            let mut matching_lbl_set = qry_lbl_ids[0].clone();
            for qry_lbl_set in qry_lbl_ids.into_iter().skip(1) {
                matching_lbl_set = &matching_lbl_set & qry_lbl_set;
            }
            matched_ids.extend(matching_lbl_set);

            // Check whether there are any matching ids.
            if matched_ids.is_empty() {
                // println!("set.frm: matched_ids.is_empty");
                continue;
            }

            // Gather benchmark functions from the matched ids.
            let all_ops = self.ops.borrow();
            for matched_id in matched_ids {
                if let Some(matched_fn) = all_ops.get(&matched_id) {
                    ret.ops.push(matched_fn.clone());
                }
            }
        }

        // Check for case where there aren't any match benchmark functions.
        if ret.ops.is_empty() {
            return None;
        }

        Some(ret)
    }
}
impl<L> fmt::Debug for Stdy<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Set")
            .field("id", &self.id)
            .field("ids", &self.ids.borrow())
            .field("ops.keys", &self.ops.borrow().keys())
            .finish()
    }
}

// Benchmark functions matching labels.
pub struct Frm<L>
where
    L: Label,
{
    /// Query labels.
    pub lbls: Vec<Vec<L>>,
    /// Benchmark functions matching labels.
    pub ops: Vec<Op<L>>,
}
impl<L> Frm<L>
where
    L: Label,
{
    // Returns a new `Frm` query.
    #[allow(clippy::ptr_arg)]
    pub fn new(lbls: &Vec<Vec<L>>) -> Self {
        Frm {
            lbls: lbls.clone(),
            ops: Vec::new(),
        }
    }

    /// Run benchmark functions.
    pub fn run(&self, itr: u32, srt: &Option<L>, sel: &Option<Stat>) -> Result<Run<L>> {
        let mut res: Vec<Dat<L>> = Vec::with_capacity(self.ops.len());

        // Calculate the overhead of running the CPU timestamp instructions.
        // Subtracting the overhead produces a more accurate measurement.
        let overhead = overhead_cpu_cyc();

        // Run each benchmark function.
        for op in self.ops.iter() {
            // Avoid compiler over-optimization of benchmark functions by using `black_box(f())`.
            //  Explanation of how black_box works with LLVM ASM and memory.
            //      https://github.com/rust-lang/rust/blob/6a944187fb917393c9c6c39825dec3c1de29787c/compiler/rustc_codegen_llvm/src/intrinsic.rs#L339
            // `black_box` call from rust benchmark.
            //      https://github.com/rust-lang/rust/blob/cb6ab9516bbbd3859b56dd23e32fe41600e0ae02/library/test/src/lib.rs#L628
            let mut benchmark = op.fnc.as_ref().borrow_mut();
            let mut vals: Vec<u64> = Vec::with_capacity(itr as usize);

            // Record benchmark function multiple times.
            // Micro-benchmarks can vary on each iteration.
            for _ in 0..itr {
                let ellapsed = benchmark();
                vals.push(ellapsed - overhead);
            }

            // Apply a statistical function to the benchmark results.
            // Overwrite benchmark results with the output of the statistical function.
            if let Some(stat) = *sel {
                match stat {
                    Mdn => {
                        let mdl = vals.len() / 2;
                        let mdn = vals.select_nth_unstable(mdl).1;
                        vals = vec![*mdn];
                    }
                    Avg => {
                        let avg = vals.iter().sum::<u64>().saturating_div(vals.len() as u64);
                        vals = vec![avg];
                    }
                    Min => {
                        let min = vals.iter().min().unwrap();
                        vals = vec![*min];
                    }
                    Max => {
                        let max = vals.iter().max().unwrap();
                        vals = vec![*max];
                    }
                    Raw => {}
                }
            }
            res.push(Dat::new(&op.lbls, vals))
        }

        // Sort benchmark results.
        if let Some(srt_lbl) = srt {
            res.sort_unstable_by_key(|dat| {
                let o_lbl = dat
                    .lbls
                    .iter()
                    .find(|x| mem::discriminant(*x) == mem::discriminant(srt_lbl));
                if let Some(lbl) = o_lbl {
                    *lbl
                } else {
                    L::default()
                }
            });
        }

        Ok(Run::new(res))
    }
}
impl<L> fmt::Debug for Frm<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lbls = self.ops.iter().fold(Vec::<String>::new(), |mut vec, x| {
            vec.push(join(&x.lbls, None));
            vec
        });
        f.debug_struct("Frm")
            .field("lbls", &self.lbls)
            .field("ops", &self.ops.len())
            .field("ops.lbls", &lbls)
            .finish()
    }
}

// A benchmark measurement run.
#[derive(Clone)]
pub struct Run<L>
where
    L: Label,
{
    /// Benchmark results.
    pub res: Vec<Dat<L>>,
}
impl<L> Run<L>
where
    L: Label,
{
    /// Returns a new run.
    pub fn new(res: Vec<Dat<L>>) -> Self {
        Run { res }
    }

    /// Group benchmark measurements.
    ///
    /// Each label is a group.
    pub fn grp(&mut self, grp_lblss: &Vec<Vec<L>>, srt: &Option<L>) -> Result<Grps<L>>
    where
        L: Label,
    {
        // Create return object.
        let mut ret = Vec::with_capacity(grp_lblss.len());

        // Create a hashmap of the run results.
        // Use the dat index as the id.
        let mut dats: HashMap<u16, Dat<L>> = HashMap::new();

        // Create a map of label to ids.
        let mut ids: HashMap<L, HashSet<u16>> = HashMap::new();

        // Populate hashmaps for later searching.
        // for (n, dat) in self.res.iter().enumerate() {
        for idx in (0..self.res.len()).rev() {
            let id = idx as u16;

            // Get and remove the dat from the run results.
            let dat = self.res.remove(idx);

            // Insert the dat id for each label.
            for lbl in dat.lbls.iter() {
                let lbl_ids = ids.entry(*lbl).or_insert(HashSet::new());
                lbl_ids.insert(id);
            }

            // Insert the id to dat.
            dats.insert(id, dat);
        }

        // Create groups.
        for grp_lbls in grp_lblss.iter() {
            // Gather ids for group labels.
            // Each label has a list of benchmark ids.
            // Ensure each id is present in each label list.
            let mut qry_lbl_ids: Vec<&HashSet<u16>> = Vec::new();
            for lbl in grp_lbls.iter() {
                if let Some(lbl_ids) = ids.get(lbl) {
                    qry_lbl_ids.push(lbl_ids);
                }
            }

            // Notify queried label doesn't exist in Frm query.
            if qry_lbl_ids.is_empty() {
                bail!(
                    "empty group: label '{}' didn't produce a group",
                    join(grp_lbls, Some('-'))
                );
            }

            // Gather matched benchmark ids.
            // Intersect the id across each list for a match.
            // Find which benchmark ids are within each label list.
            let mut matched_ids: Vec<u16> = Vec::new();
            let mut matching_lbl_set = qry_lbl_ids[0].clone();
            for qry_lbl_set in qry_lbl_ids.into_iter().skip(1) {
                matching_lbl_set = &matching_lbl_set & qry_lbl_set;
            }
            matched_ids.extend(matching_lbl_set);

            // Check whether there are any matching ids.
            if matched_ids.is_empty() {
                bail!(
                    "empty group: label '{}' didn't produce a group",
                    join(grp_lbls, Some('-'))
                );
            }

            // Gather group of dats from the matched ids.
            let mut grp_dats: Vec<Dat<L>> = Vec::new();
            for matched_id in matched_ids {
                if let Some(matched_dat) = dats.get(&matched_id) {
                    // Clone dat.
                    // Dat may be shared by multiple groups.
                    grp_dats.push(matched_dat.clone());
                }
            }

            // Check whether group dats is empty.
            if grp_dats.is_empty() {
                bail!(
                    "empty group: label '{}' didn't produce a group",
                    join(grp_lbls, Some('-'))
                );
            }

            // Sort group.
            if let Some(srt_lbl) = srt {
                grp_dats.sort_unstable_by_key(|dat| {
                    let o_lbl = dat
                        .lbls
                        .iter()
                        .find(|x| mem::discriminant(*x) == mem::discriminant(srt_lbl));
                    if let Some(lbl) = o_lbl {
                        *lbl
                    } else {
                        L::default()
                    }
                });
            }

            // Add a group.
            ret.push(Grp::new(grp_lbls, grp_dats));
        }

        Ok(Grps(ret))
    }
}
impl<L> fmt::Debug for Run<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run").field("res", &self.res.len()).finish()
    }
}
impl<L> fmt::Display for Run<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);
        for dat in self.res.iter() {
            let mut row = Vec::<String>::with_capacity(1 + dat.vals.len());
            row.push(join(&dat.lbls, None));
            for val in dat.vals.iter() {
                row.push(fmt_num(val));
            }
            tbl.add_row(row);
        }
        f.write_fmt(format_args!("{tbl}"))
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]

/// Statisitcal functions for benchmark results.
pub enum Stat {
    /// Median of the benchmark results.
    #[default]
    Mdn,
    /// Minimum of the benchmark results.
    Min,
    /// Maximum of the benchmark results.
    Max,
    /// Average of the benchmark results.
    Avg,
    /// Raw benchmarks with no statisitcal functions applied.
    Raw,
}

// A group of benchmark results.
#[derive(Clone)]
pub struct Grp<L>
where
    L: Label,
{
    /// Labels for the group.
    pub lbls: Vec<L>,
    /// Benchmark results.
    pub dats: Vec<Dat<L>>,
}
impl<L> Grp<L>
where
    L: Label,
{
    /// Returns a new group.
    pub fn new(lbls: &[L], dats: Vec<Dat<L>>) -> Self {
        Grp {
            lbls: unq_srt(lbls),
            dats,
        }
    }
}
impl<L> fmt::Debug for Grp<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grp")
            .field("lbls", &self.lbls)
            .field("dats", &self.dats.len())
            .finish()
    }
}
impl<L> fmt::Display for Grp<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);
        for dat in self.dats.iter() {
            let mut row = Vec::<String>::with_capacity(1 + dat.vals.len());
            row.push(join(&dat.lbls, None));
            for val in dat.vals.iter() {
                row.push(fmt_num(val));
            }
            tbl.add_row(row);
        }
        f.write_fmt(format_args!("{tbl}"))
    }
}

// A list of benchmark result groups.
#[derive(Debug, Clone)]
pub struct Grps<L>(Vec<Grp<L>>)
where
    L: Label;

impl<L> Grps<L>
where
    L: Label,
{
    // Transpose groups to series with the specified transpose label.
    pub fn ser(&self, trn: L) -> Result<Sers> {
        let mut sers = Vec::<Ser>::with_capacity(1 + self.0.len());

        // Iterate through each group.
        for grp in self.0.iter() {
            // dbg.then(|| println!("group ser:{:?}", join(&grp.lbls, None)));
            // dbg.then(|| println!("group ser:{:?}", join(&grp.dats[0].lbls, None)));
            // dbg.then(|| println!("group ser:{:?}", &grp.dats));
            // dbg.then(|| println!("---"));

            // Create label series from first group.
            if sers.is_empty() {
                let mut trn_vals = Vec::with_capacity(grp.dats.len());
                for dat in grp.dats.iter() {
                    match find(&dat.lbls, trn) {
                        None => {
                            bail!(
                                "group transpose: group '{}' has data which is missing transpose label '{:#}'",
                                join(&grp.lbls, None),
                                trn
                            );
                        }
                        Some(lbl) => {
                            trn_vals.push(lbl.val()? as u64);
                        }
                    }
                }
                sers.push(Ser::new(format!("{:#}", trn), trn_vals));
            }

            // Transpose one column to one row.
            let mut vals = Vec::with_capacity(self.0.len());
            for dat in grp.dats.iter() {
                // Validate that only one column exists for the transpose.
                if dat.vals.len().eq(&0) {
                    bail!(
                        "group transpose: no rows (expect:1, actual:{})",
                        dat.vals.len()
                    );
                }
                if dat.vals.len().gt(&1) {
                    bail!(
                        "group transpose: too many rows (expect:1, actual:{})",
                        dat.vals.len()
                    );
                }
                vals.push(dat.vals[0]);
            }
            let name = join(&clone_except(&grp.dats[0].lbls, trn), None);
            sers.push(Ser::new(name, vals));
        }

        // Ensure at least one series exists.
        if sers.is_empty() {
            bail!("empty series: group transpose didn't produce any series");
        }

        // Ensure series have equal lengths.
        if sers.len() > 1 {
            let fst_vals_len = sers[0].vals.len();
            for cur in sers.iter().skip(1) {
                if cur.vals.len() != fst_vals_len {
                    bail!(
                        "uneven series lengths: ('{}' len:{}, '{}' len:{})",
                        cur.name,
                        cur.vals.len(),
                        sers[0].name,
                        fst_vals_len,
                    );
                }
            }
        }

        Ok(Sers(sers))
    }
}
impl<L> fmt::Display for Grps<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for grp in self.0.iter() {
            f.write_fmt(format_args!("{}\n", grp))?;
        }
        Ok(())
    }
}

// A list of benchmark result series.
#[derive(Debug, Clone)]
pub struct Sers(Vec<Ser>);
impl Sers {
    /// Compares pairs of series.
    pub fn cmp(&self) -> Result<Cmps> {
        // Check whether there are enough series to compare.
        // First index is a header row, and isn't comparable.
        if self.0.len() == 2 {
            // Notify that one series cannot be compared.
            bail!("series comparison: only one series");
        }

        // Compare all combinations of series.
        let cmp_len = (1..self.0.len()).combinations(2).count();
        let mut cmps: Vec<Cmp> = Vec::with_capacity(cmp_len);
        for idxs in (1..self.0.len()).combinations(2) {
            cmps.push(self.cmp_pair(idxs[0], idxs[1]));
        }

        Ok(Cmps(cmps))
    }

    /// Compares a pair of series as a ratio of max/min.
    fn cmp_pair(&self, idx_a: usize, idx_b: usize) -> Cmp {
        let mut rows: Vec<Vec<Cell>> = Vec::with_capacity(4);

        // Add header row
        let mut h_cells: Vec<Cell> = self.0[0].vals.iter().map(Cell::new).collect();
        h_cells.insert(0, Cell::new(self.0[0].name.clone()));
        rows.push(h_cells);

        // Clone series 'a' and series 'b'.
        let a = self.0[idx_a].clone();
        let b = self.0[idx_b].clone();

        // Calculate the ratio of values at each index.
        // Determine the display formatting at the same time.
        // Lower times are better.
        let clr_best = Color::Green;
        let len = a.vals.len().min(b.vals.len());
        let mut a_cells: Vec<Cell> = Vec::with_capacity(1 + len);
        let mut b_cells: Vec<Cell> = Vec::with_capacity(1 + len);
        let mut c_cells: Vec<Cell> = Vec::with_capacity(1 + len);
        let mut a_best_cnt: u16 = 0;
        let mut b_best_cnt: u16 = 0;
        for n in 0..len {
            let mut min: f32;
            let max: f32;
            if a.vals[n] < b.vals[n] {
                min = a.vals[n] as f32;
                max = b.vals[n] as f32;
                a_cells.push(Cell::new(fmt_num(a.vals[n])).fg(clr_best));
                b_cells.push(Cell::new(fmt_num(b.vals[n])));
                a_best_cnt += 1;
            } else {
                min = b.vals[n] as f32;
                max = a.vals[n] as f32;
                if b.vals[n] < a.vals[n] {
                    a_cells.push(Cell::new(fmt_num(a.vals[n])));
                    b_cells.push(Cell::new(fmt_num(b.vals[n])).fg(clr_best));
                    b_best_cnt += 1;
                } else {
                    a_cells.push(Cell::new(fmt_num(a.vals[n])).fg(clr_best));
                    b_cells.push(Cell::new(fmt_num(b.vals[n])).fg(clr_best));
                    a_best_cnt += 1;
                    b_best_cnt += 1;
                }
            }
            min = min.max(1.0);
            let ratio = max.div(min);
            c_cells.push(Cell::new(fmt_f32(ratio)));
        }

        // Colorized series with the most best counts.
        // Larger is better.
        #[allow(clippy::comparison_chain)]
        if a_best_cnt == b_best_cnt {
            a_cells.insert(0, Cell::new(a.name).fg(clr_best));
            b_cells.insert(0, Cell::new(b.name).fg(clr_best));
        } else if a_best_cnt > b_best_cnt {
            a_cells.insert(0, Cell::new(a.name).fg(clr_best));
            b_cells.insert(0, Cell::new(b.name));
        } else {
            a_cells.insert(0, Cell::new(a.name));
            b_cells.insert(0, Cell::new(b.name).fg(clr_best));
        }
        c_cells.insert(0, Cell::new("ratio (max / min)"));

        rows.push(a_cells);
        rows.push(b_cells);
        rows.push(c_cells);

        Cmp(rows)
    }
}
impl fmt::Display for Sers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);
        for (n, ser) in self.0.iter().enumerate() {
            let mut row = Vec::<String>::with_capacity(1 + ser.vals.len());
            row.push(ser.name.clone());
            for val in ser.vals.iter() {
                row.push(fmt_num(val));
            }
            if n == 0 {
                tbl.set_header(row);
            } else {
                tbl.add_row(row);
            }
        }
        f.write_fmt(format_args!("{}", tbl))
    }
}

// A series of data.
#[derive(Debug, Clone)]
pub struct Ser {
    /// Name of the series.
    pub name: String,
    /// Values for the series.
    pub vals: Vec<u64>,
}
impl Ser {
    /// Returns a new series.
    pub fn new(name: String, vals: Vec<u64>) -> Self {
        Ser { name, vals }
    }
}

// A comparison of two series.
#[derive(Debug, Clone)]
pub struct Cmp(Vec<Vec<Cell>>);
impl fmt::Display for Cmp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);
        for (n, cells) in self.0.iter().enumerate() {
            if n == 0 {
                tbl.set_header(Row::from(cells.clone()));
            } else {
                tbl.add_row(Row::from(cells.clone()));
            }
        }
        f.write_fmt(format_args!("{}", tbl))
    }
}

// A list of comparisons.
#[derive(Debug, Clone)]
pub struct Cmps(Vec<Cmp>);
impl fmt::Display for Cmps {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for cmp in self.0.iter() {
            f.write_fmt(format_args!("{}\n", cmp))?;
        }
        Ok(())
    }
}

/// Results of a benchmark function run.
#[derive(Debug, Clone)]
pub struct Dat<L>
where
    L: Label,
{
    /// Labels associated with the benchmark.
    pub lbls: Vec<L>,
    /// Values of a benchmark function run.
    pub vals: Vec<u64>,
}
impl<L> Dat<L>
where
    L: Label,
{
    /// Returns a new Dat.
    pub fn new(lbls: &[L], vals: Vec<u64>) -> Self {
        Dat {
            lbls: lbls.to_vec(),
            vals,
        }
    }
}

/// A benchmark function with labels.
#[derive(Clone)]
pub struct Op<L>
where
    L: Label,
{
    /// Labels associated with the benchmark.
    pub lbls: Vec<L>,
    /// A benchmark function.
    pub fnc: Rc<RefCell<dyn FnMut() -> u64>>,
}
impl<L> Op<L>
where
    L: Label,
{
    /// Returns a new `Op`.
    pub fn new(lbls: &[L], fnc: Rc<RefCell<dyn FnMut() -> u64>>) -> Self {
        Op {
            lbls: unq_srt(lbls),
            fnc,
        }
    }
}

/// A section of a study.
///
/// Convenient for appending redundant labels.
#[derive(Debug)]
pub struct Sec<'stdy, L>
where
    L: Label,
{
    /// Labels for the section.
    pub lbls: Vec<L>,
    /// The parent stdy.
    pub stdy: Rc<RefCell<&'stdy Stdy<L>>>,
}
impl<'stdy, L> Sec<'stdy, L>
where
    L: Label,
{
    /// Returns a new section.
    pub fn new(lbls: &[L], stdy: Rc<RefCell<&'stdy Stdy<L>>>) -> Self {
        Sec {
            lbls: unq_srt(lbls),
            stdy,
        }
    }

    /// Insert a benchmark function with the section's labels.
    pub fn ins<F, O>(&self, lbls: &[L], f: F) -> Result<()>
    where
        F: FnMut() -> O,
        F: 'static,
    {
        // Add section labels.
        let all_lbls = mrg_unq_srt(&self.lbls, lbls);

        // Insert a benchmark function.
        self.stdy.borrow().ins(&all_lbls, f)
    }

    /// Insert a benchmark function, which is manually timed,
    /// with the section's labels.
    pub fn ins_prm<F, O>(&self, lbls: &[L], f: F) -> Result<()>
    where
        F: FnMut(Rc<RefCell<Tme>>) -> O,
        F: 'static,
    {
        // Add section labels.
        let all_lbls = mrg_unq_srt(&self.lbls, lbls);

        // Insert a benchmark function.
        self.stdy.borrow().ins_prm(&all_lbls, f)
    }
}

/// A label used to aggregate, filter, and sort benchmark functions.
pub trait Label:
    Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal
{
}

/// Returns an enum's struct value.
///
/// For example, enum `Len(3)` returns `3`.
pub trait EnumStructVal {
    /// `val` returns an inner struct value from an enum.
    fn val(&self) -> Result<u32>;
}

/// Measures the ellapsed time of processor instructions.
///
/// # Examples
/// ```
/// t.start();
/// // your benchmark code
/// t.stop();
/// ```
pub struct Tme(pub u64);
impl Tme {
    /// Starts the processor timer.
    pub fn start(&mut self) {
        self.0 = fst_cpu_cyc();
    }
    /// Stops the processor timer.
    pub fn stop(&mut self) {
        self.0 = lst_cpu_cyc() - self.0;
    }
}

/// Returns a starting timestamp from the processor.
///
/// Call before the thing you would like to measure,
/// and the paired function `lst_cpu_cyc()`.
#[inline]
pub fn fst_cpu_cyc() -> u64 {
    // See https://www.felixcloutier.com/x86/rdtsc
    unsafe {
        // Ensure in-order execution of the RDTSC instruction.
        x86_64::_mm_mfence();
        x86_64::_mm_lfence();
        // Read the timestamp register.
        x86_64::_rdtsc()
    }
}

/// Returns an ending timestamp from the processor.
///
/// Call after `fst_cpu_cyc()`, and the thing
/// you would like to measure.
#[inline]
pub fn lst_cpu_cyc() -> u64 {
    // See https://www.felixcloutier.com/x86/rdtscp
    unsafe {
        let mut aux: u32 = 0;
        // Read the timestamp register.
        // RDTSCP waits until all previous instructions have executed, and all previous loads are globally visible.
        // RDTSCP guarantees that the execution of all the code we wanted to measure is completed.
        let ret = x86_64::__rdtscp(&mut aux as *mut u32);
        // Ensure in-order execution of the RDTSCP instruction.
        // Instructions after RDTSCP only occur after RDTSCP.
        x86_64::_mm_lfence();
        ret
    }
}

/// Measures the running time of x86 timestamp instructions.
///
/// Returns the minimum of three runs.
///
/// Overhead is variable, within a range, and appears  
/// subject to procesor micro-op conditions.
#[inline]
pub fn overhead_cpu_cyc() -> u64 {
    let mut fst = fst_cpu_cyc();
    let mut overhead = lst_cpu_cyc() - fst;
    fst = fst_cpu_cyc();
    overhead = overhead.min(lst_cpu_cyc() - fst);
    fst = fst_cpu_cyc();
    overhead = overhead.min(lst_cpu_cyc() - fst);
    fst = fst_cpu_cyc();
    overhead = overhead.min(lst_cpu_cyc() - fst);
    overhead
}

/// Formats a number with with commas.
///
/// Supports unsigned integers, signed integers, and floating-points.
pub fn fmt_num<N>(n: N) -> String
where
    N: ToString,
{
    let mut s = n.to_string();

    // Insert commas from right to left.

    // Set the index of the first comma to write.
    let mut idx = match s.find('.') {
        // Set index for floating point
        Some(n) => n.saturating_sub(3),
        // Set index for integer
        None => s.len().saturating_sub(3),
    };

    // Find the left side limit
    // Support negative numbers
    let lim = match s.find('-') {
        // Negative number
        Some(_) => 1,
        // Positive number
        None => 0,
    };

    while idx > lim {
        s.insert(idx, ',');
        idx = idx.saturating_sub(3);
    }
    s
}

/// Returns a formatted f32 rounded to one decimal place.
///
/// Decimal place is removed if the value is greater than or equal to 10.0;
/// less than or equal to -10.0, or ends with '.0'.
///
/// Commas are added for values with more than three digits to the left
/// of the floating point.
pub fn fmt_f32(v: f32) -> String {
    let mut s = format!("{:.1}", v);
    if v >= 10.0 || v <= -10.0 || s.ends_with(".0") {
        s.drain(s.len() - 2..);
    }
    fmt_num(s)
}

/// Finds a matching label.
///
/// Useful for struct labels, e.g. Len(u32).
pub fn find<L>(lbls: &[L], l: L) -> Option<L>
where
    L: Label,
{
    for cur in lbls.iter() {
        if mem::discriminant(&l) == mem::discriminant(cur) {
            return Some(*cur);
        }
    }
    None
}

/// Join labels into one string with a separator.
pub fn join<L>(lbls: &Vec<L>, mut sep: Option<char>) -> String
where
    L: Label,
{
    let sep = sep.get_or_insert(',');
    lbls.iter().enumerate().fold(
        String::with_capacity(lbls.len() * 8),
        |mut str, (n, lbl)| {
            str.push_str(lbl.to_string().as_str());
            if n != lbls.len() - 1 {
                str.push(*sep);
            }
            str
        },
    )
}

/// Finds a matching label.
///
/// Useful for struct labels, e.g. Len(u32).
pub fn clone_except<L>(lbls: &[L], l: L) -> Vec<L>
where
    L: Label,
{
    let mut ret = lbls.to_vec();
    let len = ret.len();
    for n in 0..len {
        if mem::discriminant(&l) == mem::discriminant(&ret[n]) {
            ret.remove(n);
            break;
        }
    }
    ret
}

/// Returns a unique and sorted list of labels.
pub fn unq_srt<L>(lbls: &[L]) -> Vec<L>
where
    L: Label,
{
    let mut ret = lbls.to_vec();

    // Deduplicate labels.
    ret.dedup();

    // Sort labels.
    ret.sort_unstable();

    ret
}

/// Merges and returns a unique and sorted list of labels.
pub fn mrg_unq_srt<L>(a: &[L], b: &[L]) -> Vec<L>
where
    L: Label,
{
    let mut ret = a.to_vec();

    // Merge lists of labels.
    ret.extend(b);

    // Deduplicate labels.
    ret.dedup();

    // Sort labels.
    ret.sort_unstable();

    ret
}
