#[allow(unused_imports)] use crate::prelude::*;

use async_std::path::PathBuf as APathBuf;
use std::os::unix::prelude::OsStrExt;
use std::path::Path as SPath;
use std::str;
use libarchive::archive::{ExtractOption, ExtractOptions, ReadCompression, ReadFilter, ReadFormat};
use libarchive::{reader, writer};
use tokio::task;

pub fn extract_file_sync(src_path: &SPath, dst_path: &SPath, options: &ExtractOptions) -> Result<usize> {
    let mut src_builder = reader::Builder::new();
    src_builder.support_compression(ReadCompression::All);
    src_builder.support_filter(ReadFilter::All);
    src_builder.support_format(ReadFormat::All);

    let mut src = src_builder.open_file(src_path)?;

    let dst = writer::Disk::new();
    dst.set_options(options);

    // The libarchive crate uses &str in the function signature, and then immediately converts it back to an OsStr.
    // No reason to reject destination paths that happen to not be valid utf8 here.
    let dst_path = unsafe {
        str::from_utf8_unchecked(dst_path.as_os_str().as_bytes())
    };

    Ok(dst.write(&mut src, Some(dst_path))?)
}

pub async fn extract_file(src_path: APathBuf, dst_path: APathBuf, options: ExtractOptions) -> Result<usize> {
    task::spawn_blocking(move || {
        extract_file_sync(src_path.as_ref(), dst_path.as_ref(), &options)
    }).await?
}

pub fn safe_extract_options() -> ExtractOptions {
    use ExtractOption::*;

    let mut result = ExtractOptions::new();
    result.add(NoOverwrite);
    result.add(SecureSymlinks);
    result.add(SecureNoDotDot);
    result.add(SecureNoAbsolutePaths);

    result
}