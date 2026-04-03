use anyhow::Context;
use flate2::read::ZlibDecoder;
use std::{
    any,
    ffi::CStr,
    io::{BufReader, prelude::*},
};

pub(crate) fn invoke(name_only: bool) -> anyhow::Result<()> {
    anyhow::ensure!(name_only, "only name-only is supported");

    Ok(())
}
