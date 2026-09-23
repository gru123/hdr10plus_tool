use anyhow::Result;

use super::{CliOptions, ExtractArgs, input_from_either};
use crate::core::parser::{Parser, ParserOptions};
use crate::core::{av1, initialize_progress_bar};

pub struct Extractor {}

impl Extractor {
    pub fn extract_json(args: ExtractArgs, mut options: CliOptions) -> Result<()> {
        let ExtractArgs {
            input,
            input_pos,
            output,
            skip_reorder,
            limit,
        } = args;
        let input = input_from_either("extract", input, input_pos)?;

        if !options.verify && output.is_none() {
            options.verify = true
        };

        let stdin = match av1::Input::open(&input)? {
            av1::Input::Av1(mut av1) => {
                let pb = av1.progress_bar()?;
                let mut parser = Parser::new(
                    input,
                    output,
                    options,
                    pb,
                    skip_reorder,
                    ParserOptions { limit },
                );

                return parser.process_av1_input(&mut av1);
            }
            av1::Input::Other(stdin) => stdin,
        };

        let format = hevc_parser::io::format_from_path(&input)?;
        let pb = initialize_progress_bar(&format, &input)?;
        let mut parser = Parser::new(
            input,
            output,
            options,
            pb,
            skip_reorder,
            ParserOptions { limit },
        );

        parser.process_input(&format, stdin)
    }
}
