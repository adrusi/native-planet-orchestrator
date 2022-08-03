#[allow(unused_imports)] use crate::prelude::*;

use std::ops::{Deref, Range};
use std::str::FromStr;

pub struct MyRange<A> {
    pub inner: Range<A>
}

impl<A> FromStr for MyRange<A>
    where A: FromStr,
          A::Err: 'static + StdError + Send + Sync,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let sep_idx = s.find("..").ok_or(anyhow!("range separator not found"))?;
        let start: A = s[0..sep_idx].parse()?;
        let end: A = s[sep_idx+2..].parse()?;
        Ok(MyRange { inner: start..end })
    }
}

impl<A> Deref for MyRange<A> {
    type Target = Range<A>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}