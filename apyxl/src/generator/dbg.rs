use anyhow::Result;

use crate::generator::Generator;
use crate::model::Api;
use crate::output::Output;

/// A generator that writes out the model in a readable format.
#[derive(Default)]
pub struct Dbg {}

impl Generator for Dbg {
    fn generate(&mut self, api: &Api, output: &mut dyn Output) -> Result<()> {
        output.write_str(&format!("{:#?}\n", api))
    }
}
