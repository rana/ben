//! Provides command-line table printing.

use crate::*;
use comfy_table::{presets::UTF8_FULL, Table};

impl<L> fmt::Display for Cmp<L>
where
    L: Label,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);

        // Write header.
        let mut hdr: Vec<String> = Vec::with_capacity(1 + self.hdr_lbls.len());
        hdr.push(format!("{:#}", self.hdr_lbls[0]));
        for hdr_lbl in self.hdr_lbls.iter() {
            hdr.push(fmt_num(hdr_lbl.val().unwrap()));
        }
        tbl.set_header(hdr);

        // Write "a" values.
        let mut a_row: Vec<String> = Vec::with_capacity(1 + self.a_vals.len());
        a_row.push(join(&self.a_lbls, ','));
        for a in self.a_vals.iter() {
            a_row.push(fmt_num(a));
        }
        tbl.add_row(a_row);

        // Write "b" values.
        let mut b_row: Vec<String> = Vec::with_capacity(1 + self.b_vals.len());
        b_row.push(join(&self.b_lbls, ','));
        for b in self.b_vals.iter() {
            b_row.push(fmt_num(b));
        }
        tbl.add_row(b_row);

        // Write ratio values.
        let mut ratio_row: Vec<String> = Vec::with_capacity(1 + self.ratios.len());
        ratio_row.push("ratio (max / min)".into());
        for ratio in self.ratios.iter() {
            ratio_row.push(fmt_num(ratio));
        }
        tbl.add_row(ratio_row);

        f.write_fmt(format_args!("{}", tbl))
    }
}
