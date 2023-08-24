//! Benchmark, query, and compare Rust function performance.

#![allow(dead_code)]

mod tbl;
#[cfg(test)]
mod tst;

use crate::Sta::*;
use anyhow::{bail, Ok, Result};
use std::{
    arch::x86_64,
    cell::RefCell,
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    fmt::{self, Debug, Display},
    hash::{Hash, Hasher},
    hint::black_box,
    mem,
    rc::{Rc, Weak},
};

/// A label used to aggregate, filter, and sort benchmark functions.
pub trait Label:
    Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal
{
}

// A benchmark study.
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
    pub ops: RefCell<HashMap<u16, Op<L>>>,
    rc: Weak<RefCell<Stdy<L>>>,
}
impl<L> fmt::Debug for Stdy<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stdy")
            .field("id", &self.id)
            .field("ids", &self.ids.borrow())
            .field("ops.keys", &self.ops.borrow().keys())
            .finish()
    }
}
impl<L> Stdy<L>
where
    L: Label,
{
    // Returns a new study.
    pub fn new() -> Rc<RefCell<Stdy<L>>> {
        Rc::new_cyclic(|x| {
            RefCell::new(Stdy {
                id: RefCell::new(0),
                ids: RefCell::new(HashMap::new()),
                ops: RefCell::new(HashMap::new()),
                rc: x.clone(),
            })
        })
    }

    /// Returns a reference to the study.
    pub fn rc(&self) -> Rc<RefCell<Stdy<L>>> {
        self.rc.upgrade().unwrap()
    }

    /// Returns a section.
    ///
    /// Useful for appending redundant labels.
    pub fn sec(&self, lbls: &[L]) -> Sec<L> {
        Sec::new(lbls, self.rc().clone())
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
        self.ops.borrow_mut().insert(id, Op::new(id, lbls, fnc));

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
        self.ops.borrow_mut().insert(id, Op::new(id, lbls, fnc));

        // Increment the id for the next insert call.
        *self.id.borrow_mut() += 1;

        Ok(())
    }

    /// Returns a study query.
    ///
    /// Call `run()` on the query to materialize results.
    pub fn qry(&self) -> Rc<RefCell<Qry<L>>> {
        Qry::new(self.rc().clone())
    }
}

/// A benchmark function with labels.
#[derive(Clone)]
pub struct Op<L>
where
    L: Label,
{
    // The operation id.
    pub id: u16,
    /// Labels associated with the benchmark function.
    pub lbls: Vec<L>,
    /// The benchmark function.
    pub fnc: Rc<RefCell<dyn FnMut() -> u64>>,
}
impl<L> Op<L>
where
    L: Label,
{
    /// Returns a new operation.
    pub fn new(id: u16, lbls: &[L], fnc: Rc<RefCell<dyn FnMut() -> u64>>) -> Self {
        Op {
            id,
            lbls: unq_srt(lbls),
            fnc,
        }
    }
}
impl<L> fmt::Debug for Op<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Op")
            .field("id", &self.id)
            .field("lbls", &self.lbls)
            .finish()
    }
}

/// A benchmark query.
#[derive(Debug, Clone)]
pub struct Qry<L>
where
    L: Label,
{
    /// The parent stdy.
    pub stdy: Rc<RefCell<Stdy<L>>>,
    /// Benchmark functions.
    ///
    /// One function may be shared by multiple selections.
    pub ops: RefCell<HashMap<u16, Op<L>>>,
    /// Results of running benchmark functions.
    pub raws: RefCell<HashMap<u16, Raw<L>>>,
    /// Selections for the query.
    ///
    /// HashMap is used to ensure no duplicate selections.
    pub sels: RefCell<HashMap<u64, Rc<Sel<L>>>>,
    /// Series for the query.
    pub sers: RefCell<Vec<Rc<Ser<L>>>>,
    rc: Weak<RefCell<Qry<L>>>,
}

impl<L> Qry<L>
where
    L: Label,
{
    /// Returns a new query.
    pub fn new(stdy: Rc<RefCell<Stdy<L>>>) -> Rc<RefCell<Qry<L>>> {
        Rc::new_cyclic(|x| {
            RefCell::new(Qry {
                stdy,
                ops: RefCell::new(HashMap::new()),
                raws: RefCell::new(HashMap::new()),
                sels: RefCell::new(HashMap::new()),
                sers: RefCell::new(Vec::new()),
                rc: x.clone(),
            })
        })
    }

    /// Returns a reference to the query.
    pub fn rc(&self) -> Rc<RefCell<Qry<L>>> {
        self.rc.upgrade().unwrap()
    }

    /// Selects one or more benchmark functions with the specified labels.
    ///
    /// Applies a median function to the raw benchmark results.
    pub fn sel(&self, lbls: &[L]) -> Result<Rc<Sel<L>>> {
        self.sel_sta(lbls, Mdn)
    }

    /// Selects one or more benchmark functions with the specified labels.
    ///
    /// Calls to `sel` may insert ops to the query which are
    /// shared by multiple `sel` calls.
    ///
    /// Shared ops enable running a benchmark once for a query.
    pub fn sel_sta(&self, lbls: &[L], sta: Sta) -> Result<Rc<Sel<L>>> {
        if lbls.is_empty() {
            bail!("empty labels: nothing to select")
        }

        // Borrow the study.
        let stdy = self.stdy.borrow();

        // Gather benchmark ids by queried label.
        // Each label has a list of benchmark ids.
        // Ensure each id is present in each label list.
        let mut qry_lbl_ids: Vec<&HashSet<u16>> = Vec::new();
        let ids = stdy.ids.borrow();
        for lbl in lbls.iter() {
            if let Some(lbl_ids) = ids.get(lbl) {
                qry_lbl_ids.push(lbl_ids);
            }
        }

        // Check for case where queried label
        // doesn't exist in root benchmark set.
        if qry_lbl_ids.len() != lbls.len() || qry_lbl_ids.is_empty() {
            // println!(
            //     "qry.sel: qry_lbl_ids.len:{} != lbls.len:{}",
            //     qry_lbl_ids.len(),
            //     lbls.len()
            // );
            bail!("missing benchmarks: attempted selecting '{:?}'", lbls)
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
            // println!("qry.sel: matched_ids.is_empty");
            bail!("no matches: attempted selecting '{:?}'", lbls)
        }

        // Gather benchmark functions from the matched ids.
        // Insert the benchmark function into the query if needed.
        // Benchmark function may be shared by multiple select calls within the query.
        let stdy_ops = stdy.ops.borrow();
        let mut qry_ops = self.ops.borrow_mut();
        let mut sel_ids: Vec<u16> = Vec::with_capacity(matched_ids.len());
        for matched_id in matched_ids {
            if let Some(op) = stdy_ops.get(&matched_id) {
                qry_ops.entry(op.id).or_insert_with(|| op.clone());
                sel_ids.push(op.id);
            }
        }

        // Check whether there are any gathered ids.
        if sel_ids.is_empty() {
            // println!("qry.sel: sel_ids.is_empty");
            bail!("no gathered ids: attempted selecting '{:?}'", lbls)
        }

        // Store the selection in the query.
        // Use an Rc to enable query building with the return value.
        let sel = Rc::new(Sel::new(lbls, sel_ids, sta, self.rc().clone()));
        self.sels
            .borrow_mut()
            .entry(sel.id())
            .or_insert(sel.clone());

        Ok(sel)
    }

    /// Runs the query.
    pub fn run(&self, itr: u16) -> Result<()> {
        let sels = self.sels.borrow();
        let ops = self.ops.borrow();

        if sels.is_empty() || ops.is_empty() {
            bail!("no benchmark selections: nothing to run")
        }

        // Calculate the overhead of running the CPU timestamp instructions.
        // Subtracting the overhead produces a more accurate measurement.
        let overhead = overhead_cpu_cyc();

        // Run each benchmark function within the query.
        let mut raws = self.raws.borrow_mut();
        for (id, op) in ops.iter() {
            // Avoid compiler over-optimization of benchmark functions by using `black_box(f())`.
            //  Explanation of how black_box works with LLVM ASM and memory.
            //      https://github.com/rust-lang/rust/blob/6a944187fb917393c9c6c39825dec3c1de29787c/compiler/rustc_codegen_llvm/src/intrinsic.rs#L339
            // `black_box` call from rust benchmark.
            //      https://github.com/rust-lang/rust/blob/cb6ab9516bbbd3859b56dd23e32fe41600e0ae02/library/test/src/lib.rs#L628
            let mut benchmark = op.fnc.as_ref().borrow_mut();
            let mut vals: Vec<u64> = Vec::with_capacity(itr as usize);

            // Record benchmark function multiple times.
            // Benchmark times vary at each iteration.
            for _ in 0..itr {
                let ellapsed = benchmark();
                vals.push(ellapsed - overhead);
            }

            // Store the raw benchmark results.
            raws.insert(*id, Raw::new(&op.lbls, vals));
        }

        // Add a benchmark result to each selection.
        // Multiple selections may share the same benchmark result.
        // Clone each benchmark result.
        for sel in sels.values() {
            let mut sta_vals = sel.vals.borrow_mut();
            for id in sel.ids.iter() {
                match raws.get(id) {
                    None => bail!("missing result: benchmark id {}", id),
                    Some(raws) => {
                        // Apply a statistical function to benchmark result.
                        let mut raw_vals = raws.vals.clone();
                        let val = match sel.sta {
                            Mdn => {
                                let mdl = raw_vals.len() / 2;
                                *raw_vals.select_nth_unstable(mdl).1
                            }
                            Avg => {
                                let len = raw_vals.len() as u64;
                                raw_vals.iter().sum::<u64>().saturating_div(len)
                            }
                            Min => *raw_vals.iter().min().unwrap(),
                            Max => *raw_vals.iter().max().unwrap(),
                        };
                        sta_vals.push(StaVal::new(&raws.lbls, val));
                    }
                }
            }

            // Validate that each benchmark id has a result value.
            if sel.ids.len() != sta_vals.len() {
                bail!(
                    "uneven sel result: (ids.len:{}, vals.len:{})",
                    sel.ids.len(),
                    sta_vals.len()
                )
            }
        }

        // Print selections.
        // for sel in sels.values() {
        //     println!("{}", sel);
        // }
        let sers = self.sers.borrow();
        println!("sers:{}", sers.len());
        for ser in sers.iter() {
            println!("{:?}", ser);
            println!("{}", ser);
        }

        // TODO:

        Ok(())
    }
}

/// A selection of one or more benchmark functions.
#[derive(Debug, Clone)]
pub struct Sel<L>
where
    L: Label,
{
    /// The parent query.
    qry: Rc<RefCell<Qry<L>>>,
    /// Label qualifiers used to select benchmark functions.
    ///
    /// These labels may differ from exact benchmark function labels.
    lbls: Vec<L>,
    /// Benchmark ids matching the label qualifiers.
    ids: Vec<u16>,
    /// Benchmark result values based on the statistical function.
    vals: RefCell<Vec<StaVal<L>>>,
    /// A statistical function selecting a single value from raw benchmarks.
    sta: Sta,
    ser: Option<Rc<Ser<L>>>,
}
impl<L> Sel<L>
where
    L: Label,
{
    /// Returns a new benchmark selection.
    pub fn new(lbls: &[L], ids: Vec<u16>, sta: Sta, qry: Rc<RefCell<Qry<L>>>) -> Self {
        let ids_len = ids.len();
        Sel {
            qry,
            lbls: unq_srt(lbls),
            ids,
            vals: RefCell::new(Vec::with_capacity(ids_len)),
            sta,
            ser: None,
        }
    }

    /// A unique hash id for the selection.
    pub fn id(&self) -> u64 {
        let mut s = DefaultHasher::new();
        self.hash(&mut s);
        s.finish()
    }

    /// Transforms the selection into a series using the specified label.
    ///
    /// The label is required to be a struct enum e.g., Len(0).
    pub fn ser(&self, lbl: L) -> Result<Rc<Ser<L>>> {
        // println!("-- ser: start");
        // let mut sta_vals = self.vals.borrow().clone();

        // println!("-- sta_vals:{}", &sta_vals.len());
        // if sta_vals.is_empty() {
        //     bail!("empty selection: no values to transform")
        // }

        // // Get name from labels.
        // // alc,vct,mcr,len(131072) -> alc,vct,mcr
        // let nam_lbls = clone_except(&sta_vals[0].lbls, lbl);
        // let nam = join(&nam_lbls, ',');

        // // Sort vals based on lbl.
        // sta_vals.sort_unstable_by_key(|x| {
        //     let o_lbl = x
        //         .lbls
        //         .iter()
        //         .find(|x| mem::discriminant(*x) == mem::discriminant(&lbl));
        //     if let Some(lbl) = o_lbl {
        //         *lbl
        //     } else {
        //         L::default()
        //     }
        // });

        // // Create series.
        // let mut lbls: Vec<L> = Vec::with_capacity(sta_vals.len());
        // let mut vals: Vec<u64> = Vec::with_capacity(sta_vals.len());
        // for sta_val in sta_vals.iter() {
        //     // Extract struct enum labels.
        //     match find(&sta_val.lbls, lbl) {
        //         None => {
        //             bail!(
        //                 "missing label: statistic value '{}' doesn't have the series label '{:#}'",
        //                 join(&sta_val.lbls, ','),
        //                 lbl
        //             );
        //         }
        //         Some(lbl) => {
        //             lbls.push(lbl);
        //         }
        //     }
        //     // Extract values.
        //     vals.push(sta_val.val);
        // }

        // Store the series in the query.
        // Use an Rc to enable query building with the return value.
        let ser = Rc::new(Ser::new(lbl));
        let qry = self.qry.borrow();
        let mut sers = qry.sers.borrow_mut();
        sers.push(ser.clone());
        self.ser = Some(ser.clone());
        println!("qry.sers:{}", sers.len());

        Ok(ser)
    }
}
impl<L> Hash for Sel<L>
where
    L: Label,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ids.hash(state);
        self.sta.hash(state);
    }
}

// A series.
#[derive(Debug, Clone)]
pub struct Ser<L>
where
    L: Label,
{
    /// Label for the series.
    lbl: L,
    /// Name of the series.
    nam: String,
    /// Labels for the series.
    lbls: Vec<L>,
    /// Values for the series.
    vals: Vec<u64>,
}
impl<L> Ser<L>
where
    L: Label,
{
    /// Returns a new series.
    // pub fn new(lbl: L, nam: String, lbls: Vec<L>, vals: Vec<u64>) -> Self {
    //     Ser {
    //         lbl,
    //         nam,
    //         lbls,
    //         vals,
    //     }
    // }
    pub fn new(lbl: L) -> Self {
        Ser {
            lbl,
            nam: "".into(),
            lbls: Vec::new(),
            vals: Vec::new(),
        }
    }
}

/// Results of running a single benchmark function multiple times.
#[derive(Debug, Clone)]
pub struct Raw<L>
where
    L: Label,
{
    /// Benchmark labels.
    ///
    /// These may be different from selection labels.
    ///
    /// Benchmark labels may have more labels than selection labels.
    lbls: Vec<L>,
    /// Time values returned from running a benchmark function multiple times.
    vals: Vec<u64>,
}
impl<L> Raw<L>
where
    L: Label,
{
    /// Returns a new raw benchmark result.
    pub fn new(lbls: &[L], vals: Vec<u64>) -> Self {
        Raw {
            lbls: lbls.to_vec(),
            vals,
        }
    }
}

/// A statisitcal value derived from a raw benchmark result.
#[derive(Debug, Clone)]
pub struct StaVal<L>
where
    L: Label,
{
    /// Benchmark labels.
    ///
    /// These may be different from selection labels.
    ///
    /// Benchmark labels may have more labels than selection labels.
    lbls: Vec<L>,
    /// A benchmark value returned from a statistical function.
    val: u64,
}
impl<L> StaVal<L>
where
    L: Label,
{
    /// Returns a new statisitcal value.
    pub fn new(lbls: &[L], val: u64) -> Self {
        StaVal {
            lbls: lbls.to_vec(),
            val,
        }
    }
}

/// A statistical function selecting a single value from raw benchmark results.
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub enum Sta {
    /// Median benchmark value.
    #[default]
    Mdn,
    /// Minimum benchmark value.
    Min,
    /// Maximum benchmark value.
    Max,
    /// Average benchmark value.
    Avg,
}

/// A section of a study.
///
/// Convenient for appending redundant labels.
#[derive(Debug)]
pub struct Sec<L>
where
    L: Label,
{
    /// The parent stdy.
    pub stdy: Rc<RefCell<Stdy<L>>>,
    /// Labels for the section.
    pub lbls: Vec<L>,
}
impl<L> Sec<L>
where
    L: Label,
{
    /// Returns a new section.
    pub fn new(lbls: &[L], stdy: Rc<RefCell<Stdy<L>>>) -> Self {
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
pub fn join<L>(lbls: &Vec<L>, sep: char) -> String
where
    L: Label,
{
    lbls.iter().enumerate().fold(
        String::with_capacity(lbls.len() * 8),
        |mut str, (n, lbl)| {
            str.push_str(lbl.to_string().as_str());
            if n != lbls.len() - 1 {
                str.push(sep);
            }
            str
        },
    )
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
