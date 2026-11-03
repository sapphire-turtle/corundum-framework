/// The format a device takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OutputConfig {
    pub sample_rate: u32,
    pub channels: u16,
}
