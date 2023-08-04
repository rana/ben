//! Benchmarking for functions.
//!
//! * Measure function times in CPU cycles.
//! * Query benchmarks with user-defined labels.
//! * Aggregate and compare function statistics.
//! * Display data in command-line tables.
//! * Query benchmarks from the command-line.

#![allow(clippy::borrowed_box)]

use clap::{arg, Parser};
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
    str::FromStr,
};

/// Benchmark, query, and analyze functions
#[derive(Parser, Debug)]
pub struct Cli {
    /// Run benchmarks from one or more labels
    #[arg(
        short,
        long,
        value_name = "lbl",
        num_args = 1..,
        value_delimiter = ',',
        required = true
    )]
    frm: Vec<String>,
    /// Group benchmarks into one or more labels. Each label is a group
    #[arg(
        short,
        long,
        value_name = "lbl",
        num_args = 1..,
        value_delimiter = ','
    )]
    grp: Option<Vec<String>>,
    /// Sort benchmarks by a struct label
    #[arg(short = 's', long, value_name = "lbl[struct]")]
    srt: Option<String>,
    /// Select and apply a statisitcal function
    #[arg(short = 'x', long, value_name = "lbl[stat]")]
    sel: Option<String>,
    /// Transpose groups to series with the specified struct label
    #[arg(short = 't', long, value_name = "lbl[struct]")]
    trn: Option<String>,
    /// Compare pairs of benchmarks as a ratio of max/min
    #[arg(short = 'c', long)]
    cmp: bool,
    /// Set the number of iterations to run a benchmark function
    #[arg(short = 'i', long, value_name = "u32", default_value_t = 16)]
    itr: u32,
    /// Print debug information
    #[arg(short = 'd', long)]
    dbg: bool,
}

impl Cli {
    /// Parse command-line parameters, and query the specified `Set`.
    ///
    /// Results are printed on the console.
    pub fn prs_and_qry<L>(set: Set<L>) -> Result<()>
    where
        L: Label,
        String: From<<L as FromStr>::Err>,
    {
        let cli = Cli::parse();
        cli.dbg.then(|| println!("{:?}", cli));
        cli.qry(set)?;
        Ok(())
    }

    /// Query the specified `Set` with command-line parameters.
    ///
    /// Results are printed on the console.
    pub fn qry<L>(&self, set: Set<L>) -> Result<()>
    where
        L: Label,
        String: From<<L as FromStr>::Err>,
    {
        self.dbg.then(|| println!("{:?}", set));

        // Query benchmark functions.
        let frm_lbls = Lbls::try_from(&self.frm)?;
        match set.frm(&frm_lbls) {
            None => {
                println!("No matches")
            }
            Some(frm) => {
                self.dbg.then(|| println!("{:?}", frm));

                // Run benchmark functions.
                let run = frm.run(self.itr, &self.srt, &self.sel)?;
                self.dbg.then(|| println!("{:?}", run));
                match &self.grp {
                    None => {
                        println!("{}", run);
                    }
                    Some(grp_lbl_strs) => {
                        // Group benchmark results.
                        let grp_lbls = Lbls::try_from(grp_lbl_strs)?;
                        let grps = run.grp(&grp_lbls)?;
                        self.dbg.then(|| println!("{:?}", grps));

                        match &self.trn {
                            None => {
                                println!("{}", grps);
                            }
                            Some(trn_str) => {
                                // Transpose groups to series.
                                let trn_lbl = L::from_str(trn_str)?;
                                let sers = grps.ser(trn_lbl)?;
                                self.dbg.then(|| println!("{:?}", sers));
                                if !self.cmp {
                                    println!("{}", sers);
                                } else {
                                    let cmps = sers.cmp()?;
                                    self.dbg.then(|| println!("{:?}", cmps));
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
}

/// A label used to aggregate, filter, and sort benchmark functions.
pub trait Label:
    Debug
    + Copy
    + Eq
    + PartialEq
    + Ord
    + PartialOrd
    + Hash
    + Default
    + Display
    + EnumStructVal
    + FromStr
{
}

// A set of benchmark functions.
pub struct Set<L>
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
impl<L> Set<L>
where
    L: Label,
{
    // Returns a new set of benchmark functions.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Set {
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

    /// Insert a benchmark function to the set.
    pub fn ins<F, O>(&self, lbls: &[L], mut f: F) -> Result<()>
    where
        F: FnMut() -> O,
        F: 'static,
    {
        if lbls.is_empty() {
            return Err("missing label: parameter 'lbls' is empty".to_string());
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
            return Err("missing label: parameter 'lbls' is empty".to_string());
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
    pub fn frm(&self, lbls: &Lbls<L>) -> Option<Frm<L>> {
        // Check for case where labels are empty.
        if lbls.0.is_empty() {
            // println!("set.frm: lbls.is_empty");
            return None;
        }

        let mut ret = Frm::new(lbls);

        // Gather benchmark ids by queried label.
        // Each label has a list of benchmark ids.
        // Ensure each id is present in each label list.
        let mut qry_lbl_ids: Vec<&HashSet<u16>> = Vec::new();
        let ids = self.ids.borrow();
        for lbl in lbls.0.iter() {
            if let Some(lbl_ids) = ids.get(lbl) {
                qry_lbl_ids.push(lbl_ids);
            }
        }

        // Check for case where queried label
        // doesn't exist in root benchmark set.
        if qry_lbl_ids.len() != lbls.0.len() || qry_lbl_ids.is_empty() {
            // println!(
            //     "set.frm: qry_lbl_ids.len:{} != lbls.len:{}",
            //     qry_lbl_ids.len(),
            //     lbls.len()
            // );
            return None;
        }

        // Gather matched benchmark ids.
        // Find which benchmark ids are within each label list.
        // Intersect the id across each list for a match.
        let mut matched_ids: Vec<u16> = Vec::new();
        let mut matching_lbl_set = qry_lbl_ids[0].clone();
        for qry_lbl_set in qry_lbl_ids.into_iter().skip(1) {
            matching_lbl_set = &matching_lbl_set & qry_lbl_set;
        }
        matched_ids.extend(matching_lbl_set);

        // Check whether there are any matching ids.
        if matched_ids.is_empty() {
            // println!("set.frm: matched_ids.is_empty");
            return None;
        }

        // Gather benchmark functions from the matched ids.
        let all_ops = self.ops.borrow();
        for matched_id in matched_ids {
            if let Some(matched_fn) = all_ops.get(&matched_id) {
                ret.ops.push(matched_fn.clone());
            }
        }

        Some(ret)
    }
}
impl<L> fmt::Debug for Set<L>
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
    pub lbls: Lbls<L>,
    /// Benchmark functions matching labels.
    pub ops: Vec<Op<L>>,
}
impl<L> Frm<L>
where
    L: Label,
{
    // Returns a new `Frm` query.
    pub fn new(lbls: &Lbls<L>) -> Self {
        Frm {
            lbls: lbls.clone(),
            ops: Vec::new(),
        }
    }

    /// Run benchmark functions.
    pub fn run(&self, itr: u32, srt: &Option<String>, sel: &Option<String>) -> Result<Run<L>>
    where
        String: From<<L as FromStr>::Err>,
    {
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

            // Find the median value.
            // Overwrite values with the median value.
            if let Some(sel) = sel {
                let sel = sel.trim();
                if sel == "mdn" {
                    let mdl = vals.len() / 2;
                    let mdn = vals.select_nth_unstable(mdl).1;
                    vals = vec![*mdn];
                } else if sel == "avg" {
                    let avg = vals.iter().sum::<u64>().saturating_div(vals.len() as u64);
                    vals = vec![avg];
                } else if sel == "min" {
                    let min = vals.iter().min().unwrap();
                    vals = vec![*min];
                } else if sel == "max" {
                    let max = vals.iter().max().unwrap();
                    vals = vec![*max];
                }
            }
            res.push(Dat::new(&op.lbls, vals))
        }

        // Sort benchmark results.
        if let Some(srt) = srt {
            let srt_lbl = L::from_str(srt)?;
            res.sort_unstable_by_key(|dat| {
                let o_lbl = dat
                    .lbls
                    .0
                    .iter()
                    .find(|x| mem::discriminant(*x) == mem::discriminant(&srt_lbl));
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
            vec.push(x.lbls.join(None));
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
    pub fn grp(&self, lbls: &Lbls<L>) -> Result<Grps<L>>
    where
        L: Label,
        String: From<<L as FromStr>::Err>,
    {
        let mut ret = Vec::with_capacity(lbls.0.len());
        for grp_lbl in lbls.0.iter() {
            // Group data by label.
            let mut grp_dats = Vec::new();
            for dat in self.res.iter() {
                for dat_lbl in dat.lbls.0.iter() {
                    if *grp_lbl == *dat_lbl {
                        grp_dats.push(dat.clone());
                        break;
                    }
                }
            }

            // Notify of an empty group.
            if grp_dats.is_empty() {
                return Err(format!(
                    "empty group: label '{}' didn't produce a group",
                    grp_lbl
                ));
            }

            // Add a group.
            ret.push(Grp::new(&[*grp_lbl], grp_dats));
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
            row.push(dat.lbls.join(None));
            for val in dat.vals.iter() {
                row.push(fmt_num(val));
            }
            tbl.add_row(row);
        }
        f.write_fmt(format_args!("{tbl}"))
    }
}

// A group of benchmark results.
#[derive(Clone)]
pub struct Grp<L>
where
    L: Label,
{
    /// Labels for the group.
    pub lbls: Lbls<L>,
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
            lbls: Lbls::new(lbls),
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
            row.push(dat.lbls.join(None));
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
            // Create label series from first group.
            if sers.is_empty() {
                let mut trn_vals = Vec::with_capacity(grp.dats.len());
                for dat in grp.dats.iter() {
                    match dat.lbls.find(trn) {
                        None => {
                            return Err(format!(
                                "group transpose: group '{}' has data which is missing transpose label '{:#}'",
                                grp.lbls,
                                trn
                            ));
                        }
                        Some(lbl) => {
                            trn_vals.push(lbl.val()?);
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
                    return Err(format!(
                        "group transpose: no rows (expect:1, actual:{})",
                        dat.vals.len()
                    ));
                }
                if dat.vals.len().gt(&1) {
                    return Err(format!(
                        "group transpose: too many rows (expect:1, actual:{})",
                        dat.vals.len()
                    ));
                }
                vals.push(dat.vals[0]);
            }
            let name = format!("{}", grp.dats[0].lbls.clone_except(trn));
            sers.push(Ser::new(name, vals));
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
            return Err("series comparison: only one series".to_string());
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
        let len = a.vals.len();
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

    // Returns the series as a list of strings.
    pub fn to_vec_strs(&self) -> Vec<String> {
        let mut ret = Vec::with_capacity(1 + self.vals.len());
        ret.push(self.name.clone());
        for val in self.vals.iter() {
            ret.push(fmt_num(val));
        }
        ret
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
#[derive(Debug, Clone)] //, Eq, PartialEq)]
pub struct Dat<L>
where
    L: Label,
{
    /// Labels associated with the benchmark.
    pub lbls: Lbls<L>,
    /// Values of a benchmark function run.
    pub vals: Vec<u64>,
}
impl<L> Dat<L>
where
    L: Label,
{
    /// Returns a new Dat.
    ///
    /// `lbls` are sorted and de-duplicated.
    pub fn new(lbls: &Lbls<L>, vals: Vec<u64>) -> Self {
        Dat {
            lbls: lbls.clone(),
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
    pub lbls: Lbls<L>,
    /// A benchmark function.
    pub fnc: Rc<RefCell<dyn FnMut() -> u64>>,
}
impl<L> Op<L>
where
    L: Label,
{
    /// Returns a new `Op`.
    ///
    /// `lbls` are sorted and de-duplicated.
    pub fn new(lbls: &[L], fnc: Rc<RefCell<dyn FnMut() -> u64>>) -> Self {
        Op {
            lbls: Lbls::new(lbls),
            fnc,
        }
    }
}

/// A section of a set.
///
/// Convenient for appending redundant labels.
#[derive(Debug)]
pub struct Sec<'set, L>
where
    L: Label,
{
    /// Labels for the section.
    pub lbls: Lbls<L>,
    /// The parent set.
    pub set: Rc<RefCell<&'set Set<L>>>,
}
impl<'set, L> Sec<'set, L>
where
    L: Label,
{
    /// Returns a new section.
    pub fn new(lbls: &[L], set: Rc<RefCell<&'set Set<L>>>) -> Self {
        Sec {
            lbls: Lbls::new(lbls),
            set,
        }
    }

    /// Insert a benchmark function with the section's labels.
    pub fn ins<F, O>(&self, lbls: &[L], f: F) -> Result<()>
    where
        F: FnMut() -> O,
        F: 'static,
    {
        // Add section labels.
        let mut all_lbls = self.lbls.clone();
        all_lbls.0.extend(lbls);

        // Insert a benchmark function.
        self.set.borrow().ins(&all_lbls.0, f)
    }

    /// Insert a benchmark function, which is manually timed,
    /// with the section's labels.
    pub fn ins_prm<F, O>(&self, lbls: &[L], f: F) -> Result<()>
    where
        F: FnMut(Rc<RefCell<Tme>>) -> O,
        F: 'static,
    {
        // Add section labels.
        let mut all_lbls = self.lbls.clone();
        all_lbls.0.extend(lbls);

        // Insert a benchmark function.
        self.set.borrow().ins_prm(&all_lbls.0, f)
    }
}

/// A list of labels.
pub struct Lbls<L>(Vec<L>)
where
    L: Label;
impl<L> Lbls<L>
where
    L: Label,
{
    /// Returns a new list of labels.
    pub fn new(lbls: &[L]) -> Self {
        let mut lbls = lbls.to_vec();

        // Deduplicate labels
        lbls.dedup();

        // Sort labels
        lbls.sort_unstable();

        Lbls(lbls)
    }

    /// Finds a matching label.
    ///
    /// Useful for struct labels, e.g. Len(u32).
    pub fn find(&self, l: L) -> Option<L> {
        for cur in self.0.iter() {
            if mem::discriminant(&l) == mem::discriminant(cur) {
                return Some(*cur);
            }
        }
        None
    }

    /// Join labels into one string with a separator.
    pub fn join(&self, mut sep: Option<char>) -> String {
        let sep = sep.get_or_insert(',');
        self.0.iter().enumerate().fold(
            String::with_capacity(self.0.len() * 8),
            |mut str, (n, lbl)| {
                str.push_str(lbl.to_string().as_str());
                if n != self.0.len() - 1 {
                    str.push(*sep);
                }
                str
            },
        )
    }

    /// Finds a matching label.
    ///
    /// Useful for struct labels, e.g. Len(u32).
    pub fn clone_except(&self, l: L) -> Lbls<L> {
        let mut ret = self.0.clone();
        let len = ret.len();
        for n in 0..len {
            if mem::discriminant(&l) == mem::discriminant(&ret[n]) {
                ret.remove(n);
                break;
            }
        }
        Lbls(ret)
    }
}
impl<L> Clone for Lbls<L>
where
    L: Label,
{
    /// Returns a copy of the labels.
    fn clone(&self) -> Self {
        let len = self.0.len();
        let mut ret = Vec::with_capacity(len);
        for n in 0..len {
            ret.push(self.0[n]);
        }
        Lbls(ret)
    }
}
impl<L> TryFrom<&Vec<String>> for Lbls<L>
where
    L: Label,
    String: From<<L as FromStr>::Err>,
{
    type Error = String;

    /// Returns a list of labels.
    fn try_from(v: &Vec<String>) -> Result<Self> {
        let mut ret = Vec::with_capacity(v.len());
        for s in v {
            ret.push(L::from_str(s)?);
        }
        Ok(Lbls(ret))
    }
}
impl<L> fmt::Display for Lbls<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let sep = if f.alternate() { '-' } else { ',' };
        f.write_str(self.join(Some(sep)).as_str())
    }
}
impl<L> fmt::Debug for Lbls<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

/// Returns an enum's struct value.
///
/// For example, enum `Len(3)` returns `3`.
pub trait EnumStructVal {
    /// `val` returns an inner struct value from an enum.
    fn val(&self) -> Result<u64>;
}

/// Measures the ellapsed time of processor instructions.
///
/// # Examples
/// ```
/// t.start();
/// // your benchmark code
/// t.stop();
/// ```
pub struct Tme(u64);
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
/// '.0' suffix is removed.
///
/// Comma separator for values to the left of the floating point.
pub fn fmt_f32(v: f32) -> String {
    let mut s = format!("{:.1}", v);
    if s.ends_with('0') {
        s.drain(s.len() - 2..);
    }
    fmt_num(s)
}

/// A Result with a String error.
pub type Result<T> = std::result::Result<T, String>;
