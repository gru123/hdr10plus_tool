//! HDR10+ in AV1 bitstreams
//!
//! The metadata travels in ITU-T T.35 metadata OBUs, one per temporal unit.
//! Reading, placing and writing those OBUs is `av1_parser`'s job; this module
//! only knows which of them carry HDR10+, and how to encode an entry.

use std::fs::File;
use std::io::{BufRead, BufWriter, Write, stdout};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use indicatif::ProgressBar;

use av1_parser::io::processor::{Av1Processor, Av1ProcessorOpts};
use av1_parser::io::writer::Av1Writer;
use av1_parser::io::{IoFormat as Av1Format, IoProcessor, StreamInfo};
use av1_parser::metadata::{ITU_T35_COUNTRY_CODE_US, Metadata, itu_t35_metadata_obu};
use av1_parser::{Obu, TemporalUnit};
use hdr10plus::metadata::{Hdr10PlusMetadata, Hdr10PlusMetadataEncOpts};
use hdr10plus::metadata_json::{Hdr10PlusJsonMetadata, MetadataJsonRoot};
use hevc_parser::io::IoFormat;

use super::initialize_progress_bar;

/// An AV1 input, or stdin that turned out to be something else
pub enum Input {
    Av1(Av1Input),
    /// Not AV1. For stdin, the reader still holds everything that was read
    /// to find out.
    Other(Option<Box<dyn BufRead>>),
}

pub struct Av1Input {
    path: PathBuf,
    format: Av1Format,
    stdin: Option<Box<dyn BufRead>>,
}

impl Input {
    /// Look at `path` - a file, or `-` for stdin - and tell AV1 apart from HEVC
    pub fn open(path: &Path) -> Result<Self> {
        if path == Path::new("-") {
            let probed = av1_parser::io::probe_stdin()?;

            return Ok(match probed.format {
                Some(_) => Self::Av1(Av1Input {
                    path: path.to_path_buf(),
                    format: Av1Format::RawStdin,
                    stdin: Some(probed.reader),
                }),
                None => Self::Other(Some(probed.reader)),
            });
        }

        if !av1_parser::io::is_av1_file(path, None) {
            return Ok(Self::Other(None));
        }

        Ok(Self::Av1(Av1Input {
            path: path.to_path_buf(),
            format: av1_parser::io::format_from_path(path, None)?,
            stdin: None,
        }))
    }
}

impl Av1Input {
    pub fn format(&self) -> Av1Format {
        self.format
    }

    pub fn progress_bar(&self) -> Result<ProgressBar> {
        let format = if self.stdin.is_some() {
            IoFormat::RawStdin
        } else {
            IoFormat::Raw
        };

        initialize_progress_bar(&format, &self.path)
    }

    pub fn process(
        &mut self,
        opts: Av1ProcessorOpts,
        processor: &mut dyn IoProcessor,
    ) -> Result<()> {
        let mut av1_processor = Av1Processor::new(self.format, opts);

        match self.stdin.as_mut() {
            Some(reader) => av1_processor.process_io(reader, processor),
            None => av1_processor.process_file(processor, Some(&self.path)),
        }
    }

    /// Output file name for a bitstream written from this input
    pub fn default_output(&self, stem: &str) -> PathBuf {
        let ext = if self.format == Av1Format::Ivf {
            "ivf"
        } else {
            "av1"
        };

        PathBuf::from(format!("{stem}.{ext}"))
    }

    /// `remove`: the bitstream without its HDR10+ metadata
    pub fn remove(&mut self, output: &Path) -> Result<()> {
        let progress_bar = self.progress_bar()?;
        let mut processor = BitstreamRewriter {
            input: self.path.clone(),
            progress_bar,
            validate: false,
            metadata: None,
            output: output.to_path_buf(),
            writer: None,
            warned_existing: true,
        };

        self.process(Av1ProcessorOpts::default(), &mut processor)
    }

    /// `inject`: one HDR10+ metadata OBU per temporal unit, replacing any
    /// already there
    pub fn inject(&mut self, json_in: &Path, output: &Path, validate: bool) -> Result<()> {
        println!("Parsing JSON file...");
        stdout().flush().ok();

        let metadata_list = MetadataJsonRoot::from_file(json_in)?.scene_info;

        if metadata_list.is_empty() {
            bail!("Empty HDR10+ SceneInfo array");
        }

        println!("Processing input video for frame order info...");
        stdout().flush().ok();

        let mut counter = UnitCounter {
            input: self.path.clone(),
            units: 0,
        };
        self.process(Av1ProcessorOpts::default(), &mut counter)?;

        if counter.units as usize != metadata_list.len() {
            println!(
                "\nWarning: mismatched lengths. video {}, HDR10+ JSON {}",
                counter.units,
                metadata_list.len()
            );

            if metadata_list.len() < counter.units as usize {
                println!("Metadata will be duplicated at the end to match video length\n");
            } else {
                println!("Metadata will be skipped at the end to match video length\n");
            }
        }

        println!("Rewriting file with interleaved HDR10+ metadata OBUs..");
        stdout().flush().ok();

        let progress_bar = self.progress_bar()?;
        let mut processor = BitstreamRewriter {
            input: self.path.clone(),
            progress_bar,
            validate,
            metadata: Some(metadata_list),
            output: output.to_path_buf(),
            writer: None,
            warned_existing: false,
        };

        self.process(Av1ProcessorOpts::default(), &mut processor)
    }
}

/// The ST 2094-40 message in a metadata OBU: the whole T.35 message, starting
/// at the country code, the way `Hdr10PlusMetadata::parse` takes it.
///
/// The same test the HEVC path applies to its SEI messages.
pub fn hdr10plus_t35(obu: &Obu, validate: bool) -> Option<&[u8]> {
    let t35 = Metadata::from_obu(obu)?.itu_t35()?;

    if t35.country_code != ITU_T35_COUNTRY_CODE_US {
        return None;
    }

    // terminal_provider_code, terminal_provider_oriented_code,
    // application_identifier, application_version
    let [0x00, 0x3C, 0x00, 0x01, 4, application_version, ..] = *t35.payload() else {
        return None;
    };

    let valid_version = if validate {
        application_version == 1
    } else {
        application_version <= 1
    };

    // The trailing bits of the OBU come along; the parser reads what it needs
    valid_version.then_some(t35.bytes)
}

/// A metadata OBU carrying one entry of an HDR10+ JSON
pub fn hdr10plus_obu(metadata: &Hdr10PlusJsonMetadata, validate: bool) -> Result<Obu> {
    let opts = Hdr10PlusMetadataEncOpts {
        validate,
        ..Default::default()
    };

    let t35 = Hdr10PlusMetadata::try_from(metadata)?.encode_with_opts(&opts)?;

    Ok(itu_t35_metadata_obu(&t35))
}

struct BitstreamRewriter {
    input: PathBuf,
    progress_bar: ProgressBar,
    validate: bool,

    /// `None` to remove, the entries to inject otherwise
    metadata: Option<Vec<Hdr10PlusJsonMetadata>>,
    output: PathBuf,
    writer: Option<Av1Writer<BufWriter<File>>>,
    warned_existing: bool,
}

impl IoProcessor for BitstreamRewriter {
    fn input(&self) -> &PathBuf {
        &self.input
    }

    fn update_progress(&mut self, delta: u64) {
        self.progress_bar.inc(delta);
    }

    fn process_stream_info(&mut self, info: &StreamInfo) -> Result<()> {
        let file = File::create(&self.output).expect("Can't create file");
        self.writer = Some(Av1Writer::new(
            BufWriter::with_capacity(100_000, file),
            info,
        )?);

        Ok(())
    }

    fn process_temporal_unit(&mut self, mut tu: TemporalUnit) -> Result<()> {
        // Whatever is replaced or removed goes by the lenient test, as for
        // HEVC: an existing unit validation would reject must not stay behind
        // next to the new one.
        let before = tu.obus.len();
        tu.obus.retain(|obu| hdr10plus_t35(obu, false).is_none());

        if tu.obus.len() != before && !self.warned_existing {
            self.warned_existing = true;
            println!(
                "\nWarning: Input file already has HDR10+ metadata OBUs, they will be replaced."
            );
        }

        if let Some(list) = &self.metadata {
            // Past the end of the list, the last entry is repeated
            if let Some(metadata) = list.get(tu.index as usize).or(list.last()) {
                tu.insert_metadata(hdr10plus_obu(metadata, self.validate)?);
            }
        }

        self.writer
            .as_mut()
            .expect("stream info comes first")
            .write_temporal_unit(&tu)
    }

    fn finalize(&mut self) -> Result<()> {
        self.progress_bar.finish_and_clear();

        if let Some(writer) = self.writer.as_mut() {
            writer.flush()?;
        }

        Ok(())
    }
}

struct UnitCounter {
    input: PathBuf,
    units: u64,
}

impl IoProcessor for UnitCounter {
    fn input(&self) -> &PathBuf {
        &self.input
    }

    fn update_progress(&mut self, _delta: u64) {}

    fn process_temporal_unit(&mut self, _tu: TemporalUnit) -> Result<()> {
        self.units += 1;
        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        Ok(())
    }
}
