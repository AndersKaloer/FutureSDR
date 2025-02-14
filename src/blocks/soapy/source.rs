use std::cmp;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use crate::anyhow::{Context, Result};
use crate::blocks::soapy::config;
use crate::blocks::soapy::SoapyDevBuilder;
use crate::blocks::soapy::SoapyDevice;
use crate::blocks::soapy::SoapyDirection;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub type SoapySource = SoapyDevice<soapysdr::RxStream<Complex32>>;

impl SoapySource {
    fn new(init_cfg: config::SoapyInitConfig) -> Block {
        let mut chans = init_cfg.chans.clone();
        if chans.is_empty() {
            chans.push(0);
        }

        let mut siob = StreamIoBuilder::new();

        for i in 0..chans.len() {
            if i == 0 {
                // Never number the first output port for compatibility with single port instances
                siob = siob.add_output::<Complex32>("out");
            } else {
                siob = siob.add_output::<Complex32>(&format!("out{}", i + 1));
            }
        }

        Block::new(
            BlockMetaBuilder::new("SoapySource").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::on_freq_port)
                .add_input("gain", Self::on_gain_port)
                .add_input("sample_rate", Self::on_sample_rate_port)
                .add_input("cmd", Self::on_cmd_port)
                .build(),
            SoapySource {
                dev: None,
                init_cfg: Arc::new(Mutex::new(init_cfg)),
                chans,
                stream: None,
            },
        )
    }

    #[message_handler]
    fn on_cmd_port(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.base_cmd_handler(p, &SoapyDirection::Rx)
    }

    #[message_handler]
    fn on_freq_port(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_freq(p, &SoapyDirection::Rx)
    }

    #[message_handler]
    fn on_gain_port(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_gain(p, &SoapyDirection::Rx)
    }

    #[message_handler]
    fn on_sample_rate_port(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_sample_rate(p, &SoapyDirection::Rx)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for SoapySource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let outs = sio.outputs_mut();
        let bufs: Vec<&mut [Complex32]> = outs.iter_mut().map(|b| b.slice::<Complex32>()).collect();

        let min_out_len = bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let stream = self.stream.as_mut().unwrap();
        let n = cmp::min(min_out_len, stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        if let Ok(len) = stream.read(&bufs, 1_000_000) {
            for i in 0..outs.len() {
                sio.output(i).produce(len);
            }
        }
        io.call_again = true;
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        super::SOAPY_INIT.lock().await;
        soapysdr::configure_logging();
        if let Err(e) = self.apply_init_config(&SoapyDirection::Rx) {
            warn!("SoapySource::new() apply_init_config error: {}", e);
        }

        let dev = self.dev.as_ref().context("no dev")?;
        let cfg_mtx = &self.init_cfg.clone();
        let cfg = cfg_mtx.lock().unwrap();

        self.stream = Some(dev.rx_stream::<Complex32>(&self.chans)?);
        self.stream
            .as_mut()
            .context("no stream")?
            .activate(cfg.activate_time)?;

        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.stream
            .as_mut()
            .context("no stream")?
            .deactivate(None)?;
        Ok(())
    }
}

/// Build a [SoapySource].
///
/// Most logic is implemented in the shared [`SoapyDevBuilder`].
///
/// # Inputs
///
/// - **Message** `cmd`: a [`Pmt`] representing a configuration update or other command. See: [`SoapyConfig`](super::SoapyConfig) and `SoapyDevice::base_cmd_handler()`.
///
/// # Outputs
///
/// `out`: Samples received from device.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::SoapySourceBuilder;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let source = fg.add_block(
///     SoapySourceBuilder::new()
///         .filter("device=hackrf")
///         .sample_rate(1e6)
///         .freq(100e9)
///         .gain(10.0)
///         .build()
/// );
/// ```
pub type SoapySourceBuilder = SoapyDevBuilder<SoapySource>;

impl SoapyDevBuilder<SoapySource> {
    pub fn new() -> Self {
        Self {
            init_cfg: config::SoapyInitConfig::default(),
            _phantom: PhantomData,
        }
    }

    pub fn build(mut self) -> Block {
        self.fixup();
        SoapySource::new(self.init_cfg)
    }
}

impl Default for SoapyDevBuilder<SoapySource> {
    fn default() -> Self {
        Self::new()
    }
}
