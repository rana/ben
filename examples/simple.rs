#![allow(clippy::slow_vector_initialization)]

use anyhow::{bail, Result};
use ben::*;
use std::fmt;
use Lbl::*;

// clear && cargo r -q --example simple --profile release
pub fn main() -> Result<()> {
    let mut stdy = Stdy::new();
    stdy.reg_bld(&[Alc, Arr], |x| {
        x.ins(Len(16), || [0u32; 16]);
        x.ins(Len(32), || [0u32; 32]);
        x.ins(Len(64), || [0u32; 64]);
        x.ins(Len(128), || [0u32; 128]);
        x.ins(Len(256), || [0u32; 256]);
        x.ins(Len(512), || [0u32; 512]);
        x.ins(Len(1024), || [0u32; 1024]);
        x.ins(Len(2048), || [0u32; 2048]);
        x.ins(Len(4096), || [0u32; 4096]);
        x.ins(Len(8192), || [0u32; 8192]);
        x.ins(Len(16384), || [0u32; 16384]);
        x.ins(Len(32768), || [0u32; 32768]);
        x.ins(Len(65536), || [0u32; 65536]);
        x.ins(Len(131072), || [0u32; 131072]);
    });
    stdy.reg_bld(&[Alc, Vct, Mcr], |x| {
        x.ins(Len(16), || vec![0u32; 16]);
        x.ins(Len(32), || vec![0u32; 32]);
        x.ins(Len(64), || vec![0u32; 64]);
        x.ins(Len(128), || vec![0u32; 128]);
        x.ins(Len(256), || vec![0u32; 256]);
        x.ins(Len(512), || vec![0u32; 512]);
        x.ins(Len(1024), || vec![0u32; 1024]);
        x.ins(Len(2048), || vec![0u32; 2048]);
        x.ins(Len(4096), || vec![0u32; 4096]);
        x.ins(Len(8192), || vec![0u32; 8192]);
        x.ins(Len(16384), || vec![0u32; 16384]);
        x.ins(Len(32768), || vec![0u32; 32768]);
        x.ins(Len(65536), || vec![0u32; 65536]);
        x.ins(Len(131072), || vec![0u32; 131072]);
    });

    let itr: u16 = 64;
    let mut qry = QryBld::new();
    let alc_arr_id = qry.sel(&[Alc, Arr]);
    let alc_vct_mcr_id = qry.sel(&[Alc, Vct, Mcr]);
    qry.cmp(alc_arr_id, alc_vct_mcr_id);

    stdy.run(qry, itr)?;
    Ok(())
}

/// Benchmark labels.
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub enum Lbl {
    #[default]
    Alc,
    Arr,
    Vct,
    Mcr,
    Rsz,
    Len(u32),
}
impl fmt::Display for Lbl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Alc => write!(f, "alc"),
            Arr => write!(f, "arr"),
            Mcr => write!(f, "mcr"),
            Rsz => write!(f, "rsz"),
            Vct => write!(f, "vct"),
            Len(x) => {
                if f.alternate() {
                    write!(f, "len")
                } else {
                    write!(f, "len({})", x)
                }
            }
        }
    }
}
impl EnumStructVal for Lbl {
    fn val(&self) -> Result<u32> {
        match *self {
            Len(x) => Ok(x),
            _ => bail!("label '{}' isn't a struct enum", self),
        }
    }
}
impl Label for Lbl {}
