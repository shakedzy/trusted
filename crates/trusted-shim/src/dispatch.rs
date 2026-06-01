use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use crate::adapter::Tool;
use crate::run::run_shim;

pub fn shim_from_argv0(argv0: &OsString) -> Option<Tool> {
    Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(Tool::from_argv0)
}

pub async fn dispatch_shim(argv0: &OsString, args: Vec<OsString>) -> Result<i32> {
    let tool = shim_from_argv0(argv0).expect("shim tool name");
    run_shim(tool, args).await
}
