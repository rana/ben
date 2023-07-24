//! Benchmarking for functions.
//!
//! * Measure functions times in CPU cycles.
//! * Query and aggregate benchmarks with user-defined labels.
//! * Analyze function statistics.
//! * Display data in tables on the command line.

use comfy_table::Cell;
use core::arch::x86_64;
use itertools::join;
use num_format::{Locale, ToFormattedString};
use std::cell::UnsafeCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::mem;

/// `ITR_CNT` is the number iterations to run a benchmark function.
pub const ITR_CNT: usize = 16;

/// `Stdy` is a study.
///
/// A root structure for benchmarks.
#[derive(Debug)]
pub struct Stdy<L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `ids` are labels mapped to dat ids.
    pub ids: UnsafeCell<HashMap<L, HashSet<u64>>>,
    /// `dats` are dat ids mapped to dats.
    pub dats: UnsafeCell<HashMap<u64, Dat<L>>>,
    /// `lbls` are labels to apply to each benchmark function.
    pub lbls: Vec<L>,
}

impl<L> Stdy<L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `new` returns a new study.
    pub fn new(lbls: &[L]) -> Self {
        Stdy {
            ids: UnsafeCell::new(HashMap::new()),
            dats: UnsafeCell::new(HashMap::new()),
            lbls: lbls.to_vec(),
        }
    }

    /// `print` prints all dats from the study.
    pub fn print(&self) {
        println!("Stdy {:?}", self.lbls);
        unsafe {
            let dats = &*self.dats.get();
            dats.values().for_each(|x| println!("{:?}", x));
        }
    }

    /// `sec` returns a new study section.
    pub fn sec(&self, lbls: &[L]) -> SecStdy<L> {
        SecStdy::new(lbls, self)
    }

    /// `ben` benchmarks a function.
    #[inline(never)]
    pub fn ben<F, O>(&self, mut f: F, lbls: &[L])
    where
        F: FnMut() -> O,
    {
        // Record how many CPU cycles it takes to run the function
        let mut vals = Vec::with_capacity(ITR_CNT);
        let mut tmps = vec![f()]; // warm up cycle not measured
        let overhead = overhead_cpu_cyc();
        for _ in 0..ITR_CNT {
            // Avoid compiler over-optimization by using `tmps = vec![f()]`, `tmps[0] = f()`.
            // Assigning to a vector index avoids over-optimization. (Do not use array)
            // Avoid using `black_box(f())`.
            // `black_box(f())` panics with large stack allocations.
            // `black_box(f())` does not release memory on the stack.
            let fst = fst_cpu_cyc();
            tmps[0] = f();
            vals.push(lst_cpu_cyc() - fst - overhead);
        }

        // Associate study labels
        let mut all_lbls = self.lbls.clone();
        all_lbls.extend(lbls);

        // Create a Dat
        let dat = Dat::new(&all_lbls, vals);

        // Insert the data into the study
        self.ins(dat);
    }

    /// `ins` inserts Dat into the study.
    pub fn ins(&self, dat: Dat<L>) -> &Dat<L> {
        // Get the dat hash id
        let id = dat.id();

        unsafe {
            // Insert single dat id for each label
            let ids = &mut *self.ids.get();
            for lbl in dat.lbls.clone() {
                let lbl_ids = ids.entry(lbl).or_insert(HashSet::new());
                lbl_ids.insert(id);
            }

            // Insert the dat into dats
            let dats = &mut *self.dats.get();
            dats.insert(id, dat);

            dats.get(&id).unwrap()
        }
    }

    /// `qry` queries for data matching the specified labels.
    pub fn qry(&self, lbls: &[L]) -> Option<Qry<L>> {
        let mut res = Vec::<&Dat<L>>::new();

        // Gather dat ids by queried label
        // Each label has a list of dat ids
        // Ensure each label is present in each list
        let mut qry_lbl_ids: Vec<&HashSet<u64>> = Vec::new();
        unsafe {
            let ids = &*self.ids.get();
            for lbl in lbls {
                if let Some(lbl_ids) = ids.get(lbl) {
                    qry_lbl_ids.push(lbl_ids);
                }
            }
        }

        // Check for case where label doesn't exist
        if qry_lbl_ids.len() != lbls.len() {
            return None;
        }

        // Gather matched dat ids
        // Find which dat ids are within each label list
        // Intersect id across each list for a match
        let mut matched_ids: Vec<u64> = Vec::new();
        if !qry_lbl_ids.is_empty() {
            let mut matching_lbl_set = qry_lbl_ids[0].clone();
            for qry_lbl_set in qry_lbl_ids.into_iter().skip(1) {
                matching_lbl_set = &matching_lbl_set & qry_lbl_set;
            }
            matched_ids.extend(matching_lbl_set);
        }

        if matched_ids.is_empty() {
            return None;
        }

        unsafe {
            // Gather matched dats from ids
            let dats = &mut *self.dats.get();
            for matched_id in matched_ids {
                if let Some(dat) = dats.get(&matched_id) {
                    res.push(dat);
                }
            }
        }

        Some(Qry {
            lbls: lbls.to_vec(),
            res: UnsafeCell::new(res),
            stdy: self,
        })
    }

    /// `qry_srt` queries and sorts.
    pub fn qry_srt(&self, lbls: &[L], srt: L) -> Option<Qry<L>> {
        match self.qry(lbls) {
            None => None,
            Some(qry) => {
                qry.srt(srt);
                Some(qry)
            }
        }
    }

    /// `qry_srt_print` queries, sorts, and prints.
    pub fn qry_srt_print(&self, lbls: &[L], srt: L) -> Option<Qry<L>> {
        match self.qry(lbls) {
            None => None,
            Some(qry) => {
                qry.srt(srt);
                qry.print();
                Some(qry)
            }
        }
    }

    /// `TODO`
    pub fn qry_compare() {

    }
}

#[derive(Debug)]
pub struct Qry<'stdy, L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `lbls` are query labels.
    pub lbls: Vec<L>,
    /// `res` are query results.
    res: UnsafeCell<Vec<&'stdy Dat<L>>>,
    /// `stdy` is the query's parent Study.
    pub stdy: &'stdy Stdy<L>,
}
impl<'stdy, L> Qry<'stdy, L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `print` prints all dats from the study.
    pub fn print(&self) {
        println!("Qry {:?}", self.lbls);
        unsafe {
            let res = &*self.res.get();
            res.iter().for_each(|x| println!("{:?}", x));
        }
    }

    /// `res` returns the results of the query.
    pub fn res(&'stdy self) -> &'stdy Vec<&'stdy Dat<L>> {
        unsafe { &*self.res.get() }
    }

    /// `qry` queries for data matching the specified labels.
    pub fn qry(&self, lbls: &[L]) -> Option<Qry<L>> {
        if lbls.is_empty() {
            return None;
        }

        let mut ret = Vec::<&Dat<L>>::new();

        unsafe {
            // Gather Dats which contain all of the search labels
            let parent_res = &mut *self.res.get();
            for dat in parent_res {
                let mut match_lbl_cnt: usize = 0;
                for lbl in lbls {
                    if dat.lbls.contains(lbl) {
                        match_lbl_cnt += 1;
                    }
                }
                if match_lbl_cnt == lbls.len() {
                    ret.push(dat);
                }
            }
        }

        if ret.is_empty() {
            return None;
        }

        Some(Qry {
            lbls: lbls.to_vec(), // Omit parent query labels
            res: UnsafeCell::new(ret),
            stdy: self.stdy,
        })
    }

    /// `qry` queries and prints.
    pub fn qry_print(&self, lbls: &[L]) -> Option<Qry<L>> {
        match self.qry(lbls) {
            None => None,
            Some(qry) => {
                qry.print();
                Some(qry)
            }
        }
    }

    /// `srt` sorts the query in ascending order with the specified `srt` label.
    ///
    /// Useful for struct labels like `Len(u32)`.
    pub fn srt(&self, srt: L) {
        // Sort by matching tuple key, or default
        unsafe {
            let res = &mut *self.res.get();
            res.sort_unstable_by_key(|dat| {
                let o_lbl = dat
                    .lbls
                    .iter()
                    .find(|x| mem::discriminant(*x) == mem::discriminant(&srt));
                if let Some(lbl) = o_lbl {
                    *lbl
                } else {
                    L::default()
                }
            });
        }
    }

    /// `lbls` returns labels matching the specified `srch` label.
    ///
    /// Useful for struct labels like `Len(u32)`.
    pub fn lbls(&self, srch: L) -> Option<Vec<L>> {
        let mut ret: Vec<L> = Vec::new();

        unsafe {
            let res = &mut *self.res.get();
            for dat in res.iter() {
                for dat_lbl in dat.lbls.iter() {
                    // Expects lbl L to be an enum to use mem::discriminant()
                    if mem::discriminant(dat_lbl) == mem::discriminant(&srch) {
                        ret.push(*dat_lbl);
                    }
                }
            }
        }

        if ret.is_empty() {
            None
        } else {
            Some(ret)
        }
    }

    /// `lbl_vals` returns values for labels matching the specified `srch` label.
    ///
    /// Useful for struct labels like `Len(u32)`.
    pub fn lbl_vals(&self, srch: L) -> Option<Vec<u64>> {
        let mut ret: Vec<u64> = Vec::new();

        unsafe {
            let res = &mut *self.res.get();
            for dat in res.iter() {
                for dat_lbl in dat.lbls.iter() {
                    // Expects lbl L to be an enum to use mem::discriminant()
                    if mem::discriminant(dat_lbl) == mem::discriminant(&srch) {
                        if let Some(dat_lbl_val) = dat_lbl.val() {
                            ret.push(dat_lbl_val);
                        }
                    }
                }
            }
        }

        if ret.is_empty() {
            None
        } else {
            Some(ret)
        }
    }

    /// `dat_lbls` returns a new Dat of labels matching the specified `srch` label.
    ///
    /// Append additional labels with `apnd`.
    ///
    /// Useful for struct labels like `Len(u32)`.
    pub fn dat_lbls(&self, srch: L, apnd: &[L]) -> Option<Dat<L>> {
        Some(Dat::new(apnd, self.lbl_vals(srch)?))
    }

    /// `ins_dat_lbls` inserts a new Dat of labels matching the specified `srch` label.
    pub fn ins_dat_lbls(&self, srch: L, apnd: &[L]) -> Option<&'stdy Dat<L>> {
        match self.dat_lbls(srch, apnd) {
            None => None,
            Some(dat) => Some(self.stdy.ins(dat)),
        }
    }

    /// `mdns` returns median values.
    pub fn mdns(&self) -> Option<Vec<u64>> {
        unsafe {
            let res = &mut *self.res.get();
            let ret = res.iter().map(|x| x.mdn()).collect::<Vec<u64>>();
            if ret.is_empty() {
                None
            } else {
                Some(ret)
            }
        }
    }

    /// `dat_mdns` returns a Dat of median values.
    ///
    /// Append additional labels with `apnd`.
    ///
    /// Filter labels with `fltr`.
    pub fn dat_mdns(&self, apnd: &[L]) -> Option<Dat<L>> {
        // Append labels
        let mut all_lbls = self.lbls.clone();
        all_lbls.extend(apnd);

        // TODO: DIF FROM EXISTING LBLS?
        // [Vec, Alc, Rsz, Prm(0), Len(16384), Cyc, Raw]
        // [Vec, Alc, Rsz, Prm(0), Len(32768), Cyc, Raw]
        // Save: [Vec, Alc, Rsz, Prm(0)]
        // , fltr: &[L]

        Some(Dat::new(&all_lbls, self.mdns()?))
    }

    /// `ins_dat_mdns` inserts a new Dat of median values.
    pub fn ins_dat_mdns(&'stdy self, apnd: &[L]) -> Option<&'stdy Dat<L>> {
        match self.dat_mdns(apnd) {
            None => None,
            Some(dat) => Some(self.stdy.ins(dat)),
        }
    }
}

/// `Dat` is a name, values, and labels for data.
///
/// Adding, removing, or reordering a Dat label or Dat value changes the hash id.
///
/// If a label or value needs to be changed, create and add a new Dat to a Study,
/// and remove the original Dat from a study.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Dat<L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `lbls` are Dat labels.
    pub lbls: Vec<L>,
    /// `vals` are Dat values.
    pub vals: Vec<u64>,
}
impl<L> Dat<L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `new` returns a new Dat.
    ///
    /// `lbls` are sorted and de-duplicated for consistent hash ids.
    pub fn new(lbls: &[L], vals: Vec<u64>) -> Self {
        let mut lbls = lbls.to_vec();

        // Deduplicate labels
        lbls.dedup();

        // Sort labels
        // Label order affects the hash id
        lbls.sort_unstable();

        Dat { lbls, vals }
    }
    /// `id` returns a hash id.
    ///
    /// Adding, removing, or reordering a Dat label or Dat value changes the hash id.
    ///
    /// If a label or value needs to be changed, create and add a new Dat to a Study,
    /// and remove the original Dat from a study.
    pub fn id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    /// `srt` returns a sorted clone of Dat.
    pub fn srt(&self) -> Vec<u64> {
        let mut ret = self.vals.clone();
        ret.sort_unstable();
        ret
    }

    /// `mdn` returns the median value of Dat.
    pub fn mdn(&self) -> u64 {
        let srt = self.srt();
        srt[srt.len().saturating_div(2)]
    }

    /// `avg` returns the average value of Dat.
    pub fn avg(&self) -> u64 {
        self.vals
            .iter()
            .sum::<u64>()
            .saturating_div(self.vals.len() as u64)
    }

    /// `lbls_dif` returns labels that are in `self`, but not in the other labels.
    ///
    /// Labels are sorted.
    pub fn lbls_dif(&self, lbls: &[L]) -> Vec<L> {
        let lhs = self.lbls.iter().fold(HashSet::<L>::new(), |mut hs, x| {
            hs.insert(*x);
            hs
        });
        let rhs = lbls.iter().fold(HashSet::<L>::new(), |mut hs, x| {
            hs.insert(*x);
            hs
        });

        let mut ret = lhs.difference(&rhs).copied().collect::<Vec<L>>();

        ret.sort_unstable();

        ret
    }

    // `lbls_dif` returns labels that are in `self`, but not in the other labels.
    ///
    /// Labels are sorted.
    pub fn lbls_dif_str(&self, lbls: &[L]) -> String {
        join(self.lbls_dif(lbls), " ")
    }

    /// `vals_row` returns vals as a display row.
    pub fn vals_row(&self) -> Vec<Cell> {
        self.vals
            .iter()
            .map(|x| Cell::new(x.to_formatted_string(&Locale::en)))
            .collect::<Vec<Cell>>()
    }

    /// `vals_row_prpnd` returns vals as a display row with prepended strings.
    pub fn vals_row_prpnd<T>(&self, prpnd: &[T]) -> Vec<Cell>
    where
        T: Display,
    {
        let mut ret = prpnd.iter().map(Cell::new).collect::<Vec<Cell>>();

        ret = self.vals.iter().fold(ret, |mut ret, x| {
            ret.push(Cell::new(x.to_formatted_string(&Locale::en)));
            ret
        });

        ret
    }

    /// `vals_row_apnd` returns vals as a display row with appended strings.
    pub fn vals_row_apnd(&self, apnd: &[&str]) -> Vec<Cell> {
        let mut ret = self
            .vals
            .iter()
            .map(|x| Cell::new(x.to_formatted_string(&Locale::en)))
            .collect::<Vec<Cell>>();

        ret.extend(apnd.iter().map(Cell::new));

        ret
    }

    /// `vals_row_dif_lbls` returns vals as a display row with dif labels.
    pub fn vals_row_dif_lbls(&self, lbls: &[L]) -> Vec<Cell> {
        let lbls_dif = self.lbls_dif_str(lbls);
        self.vals_row_prpnd(&[lbls_dif])
    }
}

/// `SecStdy` is a study section.
///
/// Automatically append section labels with the section `ben` function.
#[derive(Debug)]
pub struct SecStdy<'stdy, L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `lbls` are section labels.
    pub lbls: Vec<L>,
    /// `stdy` is the section's parent Study.
    pub stdy: &'stdy Stdy<L>,
}
impl<'stdy, L> SecStdy<'stdy, L>
where
    L: Debug + Copy + Eq + PartialEq + Ord + PartialOrd + Hash + Default + Display + EnumStructVal,
{
    /// `new` returns a new study section.
    pub fn new(lbls: &[L], stdy: &'stdy Stdy<L>) -> Self {
        SecStdy {
            lbls: lbls.to_vec(),
            stdy,
        }
    }

    /// `ben` benchmarks a function, and appends section labels.
    pub fn ben<F, O>(&self, f: F, lbls: &[L])
    where
        F: FnMut() -> O,
    {
        // Add section labels
        let mut all_lbls = self.lbls.clone();
        all_lbls.extend(lbls);

        // Run the function benchmark
        self.stdy.ben(f, &all_lbls);
    }
}

/// `fst_cpu_cyc` gets a starting timestamp from the processor.
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
/// `lst_cpu_cyc` gets an ending timestamp from the processor.
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
/// `overhead_cpu_cyc` measures the overhead of running timestamp instructions.
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

#[inline]
pub fn black_box<T>(v: T) -> T {
    unsafe {
        let ret = std::ptr::read_volatile(&v);
        std::mem::forget(v);
        ret
    }
}

/// Returns an enum's struct value.
///
/// For example, enum `Len(3)` returns `3`.
pub trait EnumStructVal {
    /// `val` returns an inner struct value from an enum.
    fn val(&self) -> Option<u64>;
}
