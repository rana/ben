#![allow(clippy::new_without_default)]
#![feature(fn_ptr_trait)]

use anyhow::{bail, Ok, Result};
use itr::rngs;
use sptr::OpaqueFnPtr;
use std::marker::PhantomData;
use std::sync::mpsc::channel;
use std::{
    arch::x86_64,
    collections::{hash_map::DefaultHasher, HashMap},
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    hint::black_box,
    mem,
};
use std::{fmt, thread};
use threadpool::ThreadPool;
use Sta::*;
mod tbl;

/// A benchmark study.
#[derive(Debug)]
pub struct Stdy<L>
where
    L: Label,
{
    pub reg_blds: HashMap<u64, RegBld<L>>,
}
impl<L> Stdy<L>
where
    L: Label,
{
    pub fn new() -> Self {
        Stdy {
            reg_blds: HashMap::new(),
        }
    }
    pub fn reg_bld(&mut self, lbls: &[L], f: fn(&mut RegBld<L>)) -> &mut Self {
        if !lbls.is_empty() {
            let reg_bld = RegBld::new(lbls, f);
            self.reg_blds.entry(reg_bld.id).or_insert(reg_bld);
        }
        self
    }
    pub fn run(&mut self, qry_bld: QryBld<L>, itr: u16) -> Result<Qry<L>> {
        // println!("--- stdy.run: itr:{}, {:?}", itr, qry_bld);
        // println!("        reg_blds:{}", self.reg_blds.len());
        // println!("qry_bld.sel_blds:{}", qry_bld.sel_blds.len());
        // println!("qry_bld.cmp_blds:{}", qry_bld.cmp_blds.len());

        // Create a runtime query from the build query.
        let qry = Qry::new();

        // Create benchmark functions from the build registry.
        let mut ben_blds: Vec<BenBld<L>> = Vec::with_capacity(qry_bld.sel_blds.len() * 16);
        for (_, sel_bld) in qry_bld.sel_blds.iter() {
            match self.reg_blds.get_mut(&sel_bld.reg_id()) {
                None => bail!(
                    "build registry: missing selection '{}'",
                    join(&sel_bld.lbls, ',')
                ),
                Some(reg_bld) => {
                    // Insert benchmark functions.
                    reg_bld.ins_ben_blds();
                    // println!("    reg_bld.ben_blds:{:?}", reg_bld.ben_blds.len());

                    // Validate BenBld exist.
                    if reg_bld.ben_blds.is_empty() {
                        bail!(
                            "empty benchmarks: no benchmarks inserted for registry build '{}'",
                            join(&sel_bld.lbls, ',')
                        );
                    }

                    // Validate identical BenBld.Lbl discriminants.
                    if reg_bld.ben_blds.len() >= 2 {
                        let fst_dis = mem::discriminant(&reg_bld.ben_blds[0].lbl);
                        if !reg_bld
                            .ben_blds
                            .iter()
                            .all(|x| mem::discriminant(&x.lbl) == fst_dis)
                        {
                            bail!(
                            "different benchmark labels: expected identical label discriminants for registry build '{}'",
                            join(&sel_bld.lbls, ',')
                        );
                        }
                    }

                    // Store benchmark functions.
                    ben_blds.extend(reg_bld.ben_blds.drain(0..));
                }
            }
        }
        // println!("    ben_blds:{:?}", ben_blds);

        // Run benchmark functions in parallel.
        let ben_cnt = ben_blds.len();
        let thd_cnt = thread::available_parallelism().unwrap().into();
        let pool = ThreadPool::new(thd_cnt);
        let (tx, rx) = channel();
        // println!("thd_cnt:{}, ben_cnt:{}", thd_cnt, ben_cnt);
        for rng in rngs(thd_cnt, ben_cnt) {
            let rng_ben_blds: Vec<BenBld<L>> = ben_blds.drain(0..rng.len()).collect();
            // println!("rng_ben_blds:{}", rng_ben_blds.len());
            let tx = tx.clone();
            pool.execute(move || {
                // Calculate the overhead of running the CPU timestamp instructions.
                // Subtracting the overhead produces a more accurate measurement.
                let overhead = overhead_cpu_cyc();

                for ben_bld in rng_ben_blds {
                    let mut vals: Vec<u64> = Vec::with_capacity(itr as usize);

                    // Record benchmark function multiple times.
                    // Benchmark times vary at each iteration.
                    for _ in 0..itr {
                        let ellapsed = ben_bld.run();
                        vals.push(ellapsed - overhead);
                    }

                    // Send the benchmark results back to the main thread.
                    let ben = Ben::new(ben_bld.lbl, vals);
                    if let Err(e) = tx.send((ben_bld.reg_id, ben)) {
                        println!("send ben error: {:?}", e);
                    }
                }
            });
        }

        // Create registrations with benchmark results.
        // Registrations may be shared by multipe selections.
        let mut regs: HashMap<u64, Reg<L>> = HashMap::with_capacity(qry_bld.sel_blds.len());
        for (reg_id, ben) in rx.iter().take(ben_cnt) {
            let reg = regs.entry(reg_id).or_insert_with(|| {
                let reg_bld = self.reg_blds.get(&reg_id).unwrap();
                Reg::new(&reg_bld.lbls)
            });
            reg.bens.push(ben);
        }
        // println!("    regs:{:?}", regs);

        // Create selections from benchmark results.
        let mut sels = HashMap::with_capacity(qry_bld.sel_blds.len());
        for sel_bld in qry_bld.sel_blds.values() {
            // Get matching registration and raw benchmark results.
            let reg = regs.get(&sel_bld.reg_id()).unwrap();

            // Apply a statistical function to each benchmark result.
            let mut sta_vals = Vec::with_capacity(reg.bens.len());
            for ben in reg.bens.iter() {
                // Clone benchmark values when necessary.
                // Multiple selections may rely on the same benchmark values.
                let val = match sel_bld.sta {
                    Mdn => {
                        let mdl = ben.vals.len() / 2;
                        *ben.vals.clone().select_nth_unstable(mdl).1
                    }
                    Avg => {
                        let len = ben.vals.len() as u64;
                        ben.vals.iter().sum::<u64>().saturating_div(len)
                    }
                    Min => *ben.vals.iter().min().unwrap(),
                    Max => *ben.vals.iter().max().unwrap(),
                };
                sta_vals.push(StaVal::new(ben.lbl, val));
            }

            // Sort vals based on lbl.
            sta_vals.sort_unstable_by_key(|x| x.lbl);

            // Store selection.
            let sel = Sel::new(&sel_bld.lbls, sel_bld.sta, sta_vals);
            sels.entry(sel_bld.id()).or_insert(sel);
        }
        // println!("    sels:{:?}", sels);

        // Create comparisons.
        let mut cmps = Vec::with_capacity(qry_bld.cmp_blds.len());
        for cmp_bld in qry_bld.cmp_blds.iter() {
            let a_sel = match sels.get(&cmp_bld.a_sel_id) {
                None => bail!("missing sel: a_sel_id {}", cmp_bld.a_sel_id),
                Some(x) => x,
            };
            let b_sel = match sels.get(&cmp_bld.b_sel_id) {
                None => bail!("missing sel: b_sel_id {}", cmp_bld.b_sel_id),
                Some(x) => x,
            };

            // Validate that labels have equal lengths.
            if a_sel.vals.len() != b_sel.vals.len() {
                bail!(
                    "uneven selection lengths: (a len:{}, b len:{})",
                    a_sel.vals.len(),
                    b_sel.vals.len()
                )
            }
            // Validate each label by index.
            // Values were previously sorted by label.
            for (idx, (a, b)) in a_sel.vals.iter().zip(b_sel.vals.iter()).enumerate() {
                if a.lbl != b.lbl {
                    bail!("unequal labels: idx:{} (a:{}, b:{})", idx, a.lbl, b.lbl)
                }
            }

            // Create comparison data.
            let hdr_lbls: Vec<L> = a_sel.vals.iter().map(|x| x.lbl).collect();
            let a_lbls: Vec<L> = a_sel.lbls.clone();
            let b_lbls: Vec<L> = b_sel.lbls.clone();
            let a_vals: Vec<u64> = a_sel.vals.iter().map(|x| x.val).collect();
            let b_vals: Vec<u64> = b_sel.vals.iter().map(|x| x.val).collect();

            // Calculate the ratio of values at each index.
            let mut ratios: Vec<f32> = Vec::with_capacity(a_vals.len());
            for (a, b) in a_vals.iter().zip(b_vals.iter()) {
                let a = *a as f32;
                let b = *b as f32;
                let (mut min, max) = if a < b { (a, b) } else { (b, a) };
                min = min.max(1.0);
                let ratio = f32_pnt_one(max / min);
                ratios.push(ratio);
            }

            // Store the comparison.
            let cmp = Cmp::new(hdr_lbls, a_lbls, b_lbls, a_vals, b_vals, ratios);
            cmps.push(cmp);
        }
        // println!("    cmps:{:?}", cmps);

        // Print comparisons.
        for cmp in cmps {
            println!("{}", cmp);
        }

        // for ben in sel.bens.iter() {
        //     // Merge labels.
        //     // &[Alc, Arr] + &[Len(16)]
        //     let mrg_lbls = mrg_unq_srt(&reg_bld.lbls, &ben.lbls);
        //     println!("    mrg_lbls:{:?}", mrg_lbls);
        //     let ellapsed = ben.run();
        //     println!("    ellapsed:{:?}", ellapsed);
        // }

        Ok(qry)
    }
}
pub struct RegBld<L>
where
    L: Label,
{
    pub id: u64,
    pub lbls: Vec<L>,
    pub f: fn(&mut RegBld<L>),
    pub ben_blds: Vec<BenBld<L>>,
}
impl<L> RegBld<L>
where
    L: Label,
{
    pub fn new(lbls: &[L], f: fn(&mut RegBld<L>)) -> Self {
        // Unique and sort the labels for consistent hash id.
        let unq_srt_lbls = unq_srt(lbls);
        // Create a hash id from the labels.
        let mut h = DefaultHasher::new();
        for lbl in unq_srt_lbls.iter() {
            lbl.hash(&mut h);
        }
        RegBld {
            id: h.finish(),
            lbls: unq_srt_lbls,
            f,
            ben_blds: Vec::new(),
        }
    }
    #[inline]
    pub fn ins_ben_blds(&mut self) {
        (self.f)(self);
    }
    pub fn ins<O>(&mut self, lbl: L, f: fn() -> O) -> &mut Self {
        // Capture a function pointer to the benchmark function.
        // Enables the benchmark function to return a genericaly typed value
        // while the benchmark returns a u64 timestamp value.
        // Returning a value from the benchmark function, in coordination with `black_box()`,
        // disallows the compiler from optimizing away inner logic.
        // Returns `fn() -> u64` to enable selecting and running benchmark functions.
        let fn_ptr = unsafe { OpaqueFnPtr::from_fn(f) };

        #[inline]
        fn ben<O>(fn_ptr: OpaqueFnPtr) -> u64 {
            let ben: fn() -> O = unsafe { fn_ptr.to_fn() };
            // Avoid compiler over-optimization of benchmark functions by using `black_box(f())`.
            //  Explanation of how black_box works with LLVM ASM and memory.
            //      https://github.com/rust-lang/rust/blob/6a944187fb917393c9c6c39825dec3c1de29787c/compiler/rustc_codegen_llvm/src/intrinsic.rs#L339
            // `black_box` call from rust benchmark.
            //      https://github.com/rust-lang/rust/blob/cb6ab9516bbbd3859b56dd23e32fe41600e0ae02/library/test/src/lib.rs#L628
            // Record cpu cycles with assembly instructions.
            // Return ellapsed cpu cycles.
            let fst = fst_cpu_cyc();
            black_box(ben());
            lst_cpu_cyc() - fst
        }

        self.ben_blds
            .push(BenBld::new(self.id, lbl, fn_ptr, ben::<O>));
        self
    }
}
impl<L> fmt::Debug for RegBld<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegBld").field("lbls", &self.lbls).finish()
    }
}
pub struct QryBld<L>
where
    L: Label,
{
    pub sel_blds: HashMap<u64, SelBld<L>>,
    pub cmp_blds: Vec<CmpBld>,
}
impl<L> QryBld<L>
where
    L: Label,
{
    pub fn new() -> Self {
        QryBld {
            sel_blds: HashMap::new(),
            cmp_blds: Vec::new(),
        }
    }
    pub fn sel(&mut self, lbls: &[L]) -> u64 {
        self.sel_sta(lbls, Mdn)
    }
    pub fn sel_sta(&mut self, lbls: &[L], sta: Sta) -> u64 {
        let sel = SelBld::new(lbls, sta);
        let sel_id = sel.id();
        self.sel_blds.entry(sel_id).or_insert(sel);
        sel_id
    }
    pub fn cmp(&mut self, a_sel_id: u64, b_sel_id: u64) {
        let cmp = CmpBld::new(a_sel_id, b_sel_id);
        self.cmp_blds.push(cmp);
    }
}
impl<L> fmt::Debug for QryBld<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QryBld")
            .field("sel_blds", &self.sel_blds.values())
            .field("cmp_blds", &self.cmp_blds)
            .finish()
    }
}
#[derive(Debug)]
pub struct SelBld<L>
where
    L: Label,
{
    pub lbls: Vec<L>,
    pub sta: Sta,
}
impl<L> SelBld<L>
where
    L: Label,
{
    pub fn new(lbls: &[L], sta: Sta) -> Self {
        SelBld {
            lbls: unq_srt(lbls),
            sta,
        }
    }
    /// Hash id for selection `labels` and `statistic`.
    pub fn id(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for lbl in self.lbls.iter() {
            lbl.hash(&mut h);
        }
        self.sta.hash(&mut h);
        h.finish()
    }
    /// Hash id for selection `labels`.
    pub fn reg_id(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for lbl in self.lbls.iter() {
            lbl.hash(&mut h);
        }
        h.finish()
    }
}
#[derive(Debug)]
pub struct CmpBld {
    pub a_sel_id: u64,
    pub b_sel_id: u64,
}
impl CmpBld {
    pub fn new(a_sel_id: u64, b_sel_id: u64) -> Self {
        CmpBld { a_sel_id, b_sel_id }
    }
}
pub struct BenBld<L>
where
    L: Label,
{
    pub reg_id: u64,
    pub lbl: L,
    pub fn_ptr: OpaqueFnPtr,
    pub f: fn(OpaqueFnPtr) -> u64,
}
impl<L> BenBld<L>
where
    L: Label,
{
    pub fn new(reg_id: u64, lbl: L, fn_ptr: OpaqueFnPtr, f: fn(OpaqueFnPtr) -> u64) -> Self {
        BenBld {
            reg_id,
            lbl,
            fn_ptr,
            f,
        }
    }
    #[inline]
    pub fn run(&self) -> u64 {
        (self.f)(self.fn_ptr)
    }
}
impl<L> fmt::Debug for BenBld<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ben")
            // .field("reg_id", &self.reg_id)
            .field("lbls", &self.lbl)
            .finish()
    }
}

#[derive(Debug)]
pub struct Qry<L>
where
    L: Label,
{
    // pub sels: HashMap<u64, SelBld<L>>,
    pub phn: PhantomData<L>,
}
impl<L> Qry<L>
where
    L: Label,
{
    pub fn new() -> Self {
        Qry { phn: PhantomData }
    }
}
#[derive(Debug)]
pub struct Reg<L>
where
    L: Label,
{
    pub lbls: Vec<L>,
    pub bens: Vec<Ben<L>>,
}
impl<L> Reg<L>
where
    L: Label,
{
    pub fn new(lbls: &[L]) -> Self {
        Reg {
            lbls: lbls.to_vec(),
            bens: Vec::new(),
        }
    }
}
#[derive(Debug)]
pub struct Ben<L>
where
    L: Label,
{
    pub lbl: L,
    pub vals: Vec<u64>,
}
impl<L> Ben<L>
where
    L: Label,
{
    pub fn new(lbl: L, vals: Vec<u64>) -> Self {
        Ben { lbl, vals }
    }
}
#[derive(Debug)]
pub struct Sel<L>
where
    L: Label,
{
    pub lbls: Vec<L>,
    pub sta: Sta,
    pub vals: Vec<StaVal<L>>,
}

impl<L> Sel<L>
where
    L: Label,
{
    pub fn new(lbls: &[L], sta: Sta, vals: Vec<StaVal<L>>) -> Self {
        Sel {
            lbls: lbls.to_vec(),
            sta,
            vals,
        }
    }
}
#[derive(Debug)]
pub struct Cmp<L>
where
    L: Label,
{
    pub hdr_lbls: Vec<L>,
    pub a_lbls: Vec<L>,
    pub b_lbls: Vec<L>,
    pub a_vals: Vec<u64>,
    pub b_vals: Vec<u64>,
    pub ratios: Vec<f32>,
}
impl<L> Cmp<L>
where
    L: Label,
{
    pub fn new(
        hdr_lbls: Vec<L>,
        a_lbls: Vec<L>,
        b_lbls: Vec<L>,
        a_vals: Vec<u64>,
        b_vals: Vec<u64>,
        ratios: Vec<f32>,
    ) -> Self {
        Cmp {
            hdr_lbls,
            a_lbls,
            b_lbls,
            a_vals,
            b_vals,
            ratios,
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
    lbl: L,
    /// A benchmark value returned from a statistical function.
    val: u64,
}
impl<L> StaVal<L>
where
    L: Label,
{
    /// Returns a new statisitcal value.
    pub fn new(lbl: L, val: u64) -> Self {
        StaVal { lbl, val }
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

/// A label used to aggregate, filter, and sort benchmark functions.
pub trait Label:
    Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Display + EnumStructVal + Send + 'static
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
// 1:32:00 -
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

pub fn f32_pnt_one(v: f32) -> f32 {
    format!("{:.1}", v).parse::<f32>().unwrap()
}
