use crate::*;
use Lbl::*;

#[test]
fn frm_hash() {
    // let a = Frm {
    //     lbls: vec![Alc, Arr],
    //     ops: vec![],
    // };
    // let b = Frm {
    //     lbls: vec![Alc, Arr],
    //     ops: vec![],
    // };
    // assert_eq!(a.id(), b.id())
}

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
