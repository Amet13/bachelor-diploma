//! Word import and export commands (filled in with M7).

use proofback_store::{Error, Project, Result};

use crate::ImportArgs;
use crate::output::Style;

pub fn import(_p: &Project, a: &ImportArgs, _quiet: bool, _style: &Style) -> Result<u8> {
    Err(Error::Usage(format!(
        "{}: Word import is not available yet",
        a.file.display()
    )))
}
