use std::io::{stdout, Write};

use crate::model::chunk::Chunk;
use anyhow::Result;

use crate::output::Output;

#[derive(Default)]
pub struct StdOut {}

impl Output for StdOut {
    fn start_chunk(&mut self, chunk: &Chunk) -> Result<()> {
        if let Some(path) = &chunk.relative_file_path {
            stdout().write("-------------------".as_bytes())?;
            stdout().write(format!("--- CHUNK: {} \n", path.to_string_lossy()).as_bytes())?;
        }
        Ok(())
    }

    fn end_chunk(&mut self, _: &Chunk) -> Result<()> {
        Ok(())
    }

    fn write_str(&mut self, data: &str) -> Result<()> {
        let _ = stdout().write(data.as_bytes())?;
        Ok(())
    }

    fn write(&mut self, data: char) -> Result<()> {
        let _ = stdout().write(&[data as u8])?;
        Ok(())
    }

    fn newline(&mut self) -> Result<()> {
        let _ = stdout().write(&[b'\n'])?;
        Ok(())
    }
}
