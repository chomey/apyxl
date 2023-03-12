use std::io::{stdout, Write};

use anyhow::Result;

use crate::output::Output;

#[derive(Default)]
pub struct StdOut {
    header: String,
}

impl StdOut {
    pub fn new(header: impl ToString) -> Self {
        Self {
            header: header.to_string(),
        }
    }
}

impl Output for StdOut {
    fn write(&mut self, data: &str) -> Result<()> {
        let _ = stdout().write(self.header.as_bytes())?;
        let _ = stdout().write("\n".as_bytes())?;
        let _ = stdout().write(data.as_bytes())?;
        let _ = stdout().write("\n".as_bytes())?;
        Ok(())
    }
}
