//! What Windows is playing, split into six frequency bands (bass first, treble last) for the
//! equalizer bars, plus a beat pulse from the bass. A background thread reads the
//! default output device through WASAPI loopback; samples are only analysed in memory, never
//! stored. The thread only captures while the UI says music is playing.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

pub const BANDS: usize = 6;
/// Crossovers between the bands, in Hz.
const SPLIT: [f32; BANDS - 1] = [120.0, 300.0, 800.0, 2000.0, 5000.0];

/// Set by the UI while music plays; the capture thread idles otherwise.
static WANT: AtomicBool = AtomicBool::new(false);
/// Loudest RMS per band since the UI last looked (f32 bits; for non-negative floats the bit
/// pattern orders like the value, so `fetch_max` works).
static PEAK: [AtomicU32; BANDS] = [const { AtomicU32::new(0) }; BANDS];
/// Set whenever audio arrived.
static FRESH: AtomicBool = AtomicBool::new(false);

/// One-pole low-pass filters at the crossovers; each band is the difference of two of them.
struct Bank {
    a: [f32; BANDS - 1],
    lp: [f32; BANDS - 1],
    sum: [f32; BANDS],
    n: u32,
}

impl Bank {
    fn new(rate: f32) -> Self {
        let a = SPLIT.map(|f| 1.0 - (-2.0 * std::f32::consts::PI * f / rate).exp());
        Bank { a, lp: [0.0; BANDS - 1], sum: [0.0; BANDS], n: 0 }
    }

    fn sample(&mut self, x: f32) {
        for k in 0..BANDS - 1 {
            self.lp[k] += self.a[k] * (x - self.lp[k]);
        }
        for i in 0..BANDS {
            let hi = if i < BANDS - 1 { self.lp[i] } else { x };
            let b = hi - if i > 0 { self.lp[i - 1] } else { 0.0 };
            self.sum[i] += b * b;
        }
        self.n += 1;
    }

    fn publish(&mut self) {
        if self.n == 0 {
            return;
        }
        for i in 0..BANDS {
            let rms = (self.sum[i] / self.n as f32).sqrt();
            PEAK[i].fetch_max(rms.to_bits(), Relaxed);
            self.sum[i] = 0.0;
        }
        self.n = 0;
        FRESH.store(true, Relaxed);
    }
}

fn capture_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    loop {
        if !WANT.load(Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        if capture().is_err() {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// Captures until music stops, an error occurs, or it is time to pick up a possibly new
/// default device.
fn capture() -> windows::core::Result<()> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let fmt = client.GetMixFormat()?;
        let (rate, channels, bits) = ((*fmt).nSamplesPerSec as f32, (*fmt).nChannels as usize, (*fmt).wBitsPerSample);
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as *const _));
        init?;
        // the shared-mode mix format is 32-bit float in practice; 16-bit PCM is handled too
        if channels == 0 || (bits != 32 && bits != 16) {
            std::thread::sleep(Duration::from_secs(5));
            return Ok(());
        }
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        let mut bank = Bank::new(rate);
        let started = Instant::now();
        while WANT.load(Relaxed) && started.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(12));
            while cap.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags) = (0u32, 0u32);
                cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 && !data.is_null() {
                    for f in 0..frames as usize {
                        let mut x = 0.0;
                        for c in 0..channels {
                            let i = f * channels + c;
                            x += if bits == 32 { *(data as *const f32).add(i) } else { *(data as *const i16).add(i) as f32 / 32768.0 };
                        }
                        bank.sample(x / channels as f32);
                    }
                }
                cap.ReleaseBuffer(frames)?;
            }
            bank.publish();
        }
        let _ = client.Stop();
        Ok(())
    }
}

pub struct Beat {
    /// Bar heights, bass first, 0..1.
    pub bands: [f32; BANDS],
    /// Average level per band over the last half second, in dB. Bars show how far a band
    /// jumps above it, so hits stand out even in loud (compressed) songs, and the much
    /// quieter treble moves as much as the bass.
    avg: [f32; BANDS],
    /// 1 right on a bass beat, then fades out.
    pub pulse: f32,
    armed: bool,
    last_beat: Instant,
    last_data: Option<Instant>,
}

impl Beat {
    pub fn new() -> Self {
        std::thread::spawn(capture_thread);
        Beat { bands: [0.0; BANDS], avg: [f32::NAN; BANDS], pulse: 0.0, armed: true, last_beat: Instant::now(), last_data: None }
    }

    /// Whether audio arrived recently (otherwise the bars fall back to a fixed loop).
    pub fn live(&self) -> bool {
        self.last_data.is_some_and(|t| t.elapsed() < Duration::from_millis(500))
    }

    /// Call every frame; `playing` is false while nothing plays, then the bars settle.
    pub fn update(&mut self, dt: f32, playing: bool) {
        WANT.store(playing, Relaxed);
        self.pulse *= (-dt * 7.0).exp();
        let now = Instant::now();
        if FRESH.swap(false, Relaxed) {
            self.last_data = Some(now);
        }
        let live = playing && self.live();
        for i in 0..BANDS {
            let raw = f32::from_bits(PEAK[i].swap(0, Relaxed));
            if !live {
                self.bands[i] = (self.bands[i] - dt * 4.0).max(0.0);
                self.avg[i] = f32::NAN;
                continue;
            }
            if raw <= 0.0 {
                // no new packet this frame
                self.bands[i] = (self.bands[i] - dt * 3.0).max(0.0);
                continue;
            }
            let db = 20.0 * (raw + 1e-6).log10();
            let avg = &mut self.avg[i];
            *avg = if avg.is_nan() { db } else { *avg + (db - *avg) * (dt * 2.5).min(1.0) };
            // steady sound sits at about a third, a hit 6 dB above the average fills the bar
            let norm = ((db - *avg + 3.0) / 9.0).clamp(0.0, 1.0);
            let b = &mut self.bands[i];
            *b = if norm > *b { *b + (norm - *b) * 0.7 } else { (*b - dt * 4.0).max(norm) };
        }
        if self.bands[0] < 0.45 {
            self.armed = true;
        } else if self.bands[0] > 0.8 && self.armed && now.duration_since(self.last_beat) > Duration::from_millis(180) {
            self.pulse = 1.0;
            self.last_beat = now;
            self.armed = false;
        }
    }
}
