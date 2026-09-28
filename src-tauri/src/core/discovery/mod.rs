mod detectors;
mod scanner;

use std::{path::Path, sync::atomic::AtomicBool};

use crate::models::detection::WorkspaceScanResult;

pub fn scan_workspace(root: &Path) -> Result<WorkspaceScanResult, String> {
    scanner::scan_workspace(root)
}

pub fn scan_workspace_with_cancel(
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<WorkspaceScanResult, String> {
    scanner::scan_workspace_with_cancel(root, cancelled)
}
