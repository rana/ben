#![allow(clippy::slow_vector_initialization)]

use anyhow::{bail, Result};
use ben::Stat::*;
use ben::*;
use std::fmt;
use Lbl::*;

// cargo r --example simple --profile release
pub fn main() -> Result<()> {
    let set = new_set()?;
    let itr: u32 = 64;
    set.qry(Qry {
        frm: vec![vec![Alc, Arr], vec![Alc, Vct, Mcr]],
        grp: Some(vec![vec![Alc, Arr], vec![Alc, Vct, Mcr]]),
        srt: Some(Len(0)),
        sta: Some(Mdn),
        trn: Some(Len(0)),
        cmp: true,
        itr,
    })?;
    // set.sel([Alc, Arr]);
    set.qry(Qry {
        frm: vec![vec![Alc, Arr], vec![Alc, Vct, Rsz]],
        grp: Some(vec![vec![Alc, Arr], vec![Alc, Vct, Rsz]]),
        srt: Some(Len(0)),
        sta: Some(Mdn),
        trn: Some(Len(0)),
        cmp: true,
        itr,
    })?;
    set.qry(Qry {
        frm: vec![vec![Alc, Vct, Mcr], vec![Alc, Vct, Rsz]],
        grp: Some(vec![vec![Alc, Vct, Mcr], vec![Alc, Vct, Rsz]]),
        srt: Some(Len(0)),
        sta: Some(Mdn),
        trn: Some(Len(0)),
        cmp: true,
        itr,
    })?;
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

/// Returns a set of benchmark functions ready to be run.
pub fn new_set() -> Result<Stdy<Lbl>> {
    let ret = Stdy::new();
    {
        let sec = ret.sec(&[Alc, Arr]);
        sec.ins(&[Len(16)], || [0u32; 16])?;
        sec.ins(&[Len(32)], || [0u32; 32])?;
        sec.ins(&[Len(64)], || [0u32; 64])?;
        sec.ins(&[Len(128)], || [0u32; 128])?;
        sec.ins(&[Len(256)], || [0u32; 256])?;
        sec.ins(&[Len(512)], || [0u32; 512])?;
        sec.ins(&[Len(1024)], || [0u32; 1024])?;
        sec.ins(&[Len(2048)], || [0u32; 2048])?;
        sec.ins(&[Len(4096)], || [0u32; 4096])?;
        sec.ins(&[Len(8192)], || [0u32; 8192])?;
        sec.ins(&[Len(16384)], || [0u32; 16384])?;
        sec.ins(&[Len(32768)], || [0u32; 32768])?;
        sec.ins(&[Len(65536)], || [0u32; 65536])?;
        sec.ins(&[Len(131072)], || [0u32; 131072])?;
    }
    {
        let sec = ret.sec(&[Alc, Vct, Rsz]);
        sec.ins(&[Len(16)], || {
            let mut ret = Vec::<u32>::with_capacity(16);
            ret.resize(16, 0);
            ret
        })?;
        sec.ins(&[Len(32)], || {
            let mut ret = Vec::<u32>::with_capacity(32);
            ret.resize(32, 0);
            ret
        })?;
        sec.ins(&[Len(64)], || {
            let mut ret = Vec::<u32>::with_capacity(64);
            ret.resize(64, 0);
            ret
        })?;
        sec.ins(&[Len(128)], || {
            let mut ret = Vec::<u32>::with_capacity(128);
            ret.resize(128, 0);
            ret
        })?;
        sec.ins(&[Len(256)], || {
            let mut ret = Vec::<u32>::with_capacity(256);
            ret.resize(256, 0);
            ret
        })?;
        sec.ins(&[Len(512)], || {
            let mut ret = Vec::<u32>::with_capacity(512);
            ret.resize(512, 0);
            ret
        })?;
        sec.ins(&[Len(1024)], || {
            let mut ret = Vec::<u32>::with_capacity(1024);
            ret.resize(1024, 0);
            ret
        })?;
        sec.ins(&[Len(2048)], || {
            let mut ret = Vec::<u32>::with_capacity(2048);
            ret.resize(2048, 0);
            ret
        })?;
        sec.ins(&[Len(4096)], || {
            let mut ret = Vec::<u32>::with_capacity(4096);
            ret.resize(4096, 0);
            ret
        })?;
        sec.ins(&[Len(8192)], || {
            let mut ret = Vec::<u32>::with_capacity(8192);
            ret.resize(8192, 0);
            ret
        })?;
        sec.ins(&[Len(16384)], || {
            let mut ret = Vec::<u32>::with_capacity(16384);
            ret.resize(16384, 0);
            ret
        })?;
        sec.ins(&[Len(32768)], || {
            let mut ret = Vec::<u32>::with_capacity(32768);
            ret.resize(32768, 0);
            ret
        })?;
        sec.ins(&[Len(65536)], || {
            let mut ret = Vec::<u32>::with_capacity(65536);
            ret.resize(65536, 0);
            ret
        })?;
        sec.ins(&[Len(131072)], || {
            let mut ret = Vec::<u32>::with_capacity(131072);
            ret.resize(131072, 0);
            ret
        })?;
    }
    {
        let sec = ret.sec(&[Alc, Vct, Mcr]);
        sec.ins(&[Len(16)], || vec![0u32; 16])?;
        sec.ins(&[Len(32)], || vec![0u32; 32])?;
        sec.ins(&[Len(64)], || vec![0u32; 64])?;
        sec.ins(&[Len(128)], || vec![0u32; 128])?;
        sec.ins(&[Len(256)], || vec![0u32; 256])?;
        sec.ins(&[Len(512)], || vec![0u32; 512])?;
        sec.ins(&[Len(1024)], || vec![0u32; 1024])?;
        sec.ins(&[Len(2048)], || vec![0u32; 2048])?;
        sec.ins(&[Len(4096)], || vec![0u32; 4096])?;
        sec.ins(&[Len(8192)], || vec![0u32; 8192])?;
        sec.ins(&[Len(16384)], || vec![0u32; 16384])?;
        sec.ins(&[Len(32768)], || vec![0u32; 32768])?;
        sec.ins(&[Len(65536)], || vec![0u32; 65536])?;
        sec.ins(&[Len(131072)], || vec![0u32; 131072])?;
    }
    Ok(ret)
}
