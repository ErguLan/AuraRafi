//! Bounded Windows PCM/WAV playback. No global mixer or audio dependency.
use raf_core::SceneGraph;
use raf_script::AudioCommand;
use std::collections::HashMap;
use std::path::Path;
pub struct RuntimeAudio {
    #[cfg(target_os = "windows")]
    voices: HashMap<String, windows::Voice>,
    enabled: bool,
}
impl RuntimeAudio {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            #[cfg(target_os = "windows")]
            voices: HashMap::new(),
        }
    }
    pub fn consume(
        &mut self,
        scene: &SceneGraph,
        root: &Path,
        commands: Vec<AudioCommand>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if !self.enabled {
            return errors;
        }
        #[cfg(target_os = "windows")]
        self.voices.retain(|_, voice| !voice.finished());
        for command in commands.into_iter().take(256) {
            let result = self.apply(scene, root, command);
            if let Err(error) = result {
                if errors.len() < 32 {
                    errors.push(error);
                }
            }
        }
        errors
    }
    fn apply(
        &mut self,
        scene: &SceneGraph,
        root: &Path,
        command: AudioCommand,
    ) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            match command {
                AudioCommand::Stop { name } => {
                    self.voices.remove(&name);
                }
                AudioCommand::SetVolume { name, volume } => {
                    if let Some(voice) = self.voices.get_mut(&name) {
                        voice.set_volume(volume)?;
                    }
                }
                AudioCommand::Play { name } => {
                    let mut matches = scene.iter().filter(|(id, n)| {
                        scene.is_valid_node(*id) && n.name == name && n.audio_source.enabled
                    });
                    let (_, node) = matches
                        .next()
                        .ok_or_else(|| format!("Audio source not found: {name}"))?;
                    if matches.next().is_some() {
                        return Err(format!("Ambiguous audio source: {name}"));
                    }
                    let source = node.audio_source.clone();
                    let root = root.canonicalize().map_err(|e| e.to_string())?;
                    let clip = root
                        .join("assets")
                        .join(&source.clip)
                        .canonicalize()
                        .map_err(|e| format!("Audio clip {}: {e}", source.clip))?;
                    if !clip.starts_with(root.join("assets")) {
                        return Err("audio clip escapes project assets".into());
                    }
                    self.voices.remove(&name);
                    if self.voices.len() >= 4 {
                        return Err("runtime audio voice budget exceeded (4)".into());
                    }
                    self.voices.insert(
                        name,
                        windows::Voice::open(&clip, source.looping, source.volume)?,
                    );
                }
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (scene, root, command);
            Err("native audio output is currently implemented only on Windows".into())
        }
    }
    pub fn stop(&mut self) {
        #[cfg(target_os = "windows")]
        self.voices.clear();
    }
    pub fn set_paused(&mut self, paused: bool) {
        #[cfg(target_os = "windows")]
        for voice in self.voices.values_mut() {
            voice.set_paused(paused);
        }
        #[cfg(not(target_os = "windows"))]
        let _ = paused;
    }
}
#[cfg(target_os = "windows")]
mod windows {
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    static DRIVER_FAILED: AtomicBool = AtomicBool::new(false);
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;
    #[repr(C)]
    struct WaveFormat {
        tag: u16,
        channels: u16,
        rate: u32,
        bytes_per_second: u32,
        block_align: u16,
        bits: u16,
        extra: u16,
    }
    #[repr(C)]
    struct WaveHeader {
        data: *mut i8,
        length: u32,
        recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: *mut WaveHeader,
        reserved: usize,
    }
    #[link(name = "winmm")]
    extern "system" {
        fn waveOutOpen(
            handle: *mut *mut c_void,
            device: u32,
            format: *const WaveFormat,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        fn waveOutPrepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveOutWrite(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveOutReset(handle: *mut c_void) -> u32;
        fn waveOutUnprepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveOutClose(handle: *mut c_void) -> u32;
        fn waveOutSetVolume(handle: *mut c_void, volume: u32) -> u32;
        fn waveOutPause(handle: *mut c_void) -> u32;
        fn waveOutRestart(handle: *mut c_void) -> u32;
    }
    pub struct Voice {
        handle: *mut c_void,
        header: Option<Box<UnsafeCell<WaveHeader>>>,
        data: Option<Box<[u8]>>,
        done: Option<Box<AtomicBool>>,
        prepared: bool,
        paused: bool,
    }
    impl Voice {
        pub fn open(path: &Path, looping: bool, volume: f32) -> Result<Self, String> {
            if DRIVER_FAILED.load(Ordering::Relaxed) {
                return Err(
                    "audio driver cleanup failed; further playback is disabled for this process"
                        .into(),
                );
            }
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(|e| e.to_string())?
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("WAV clip exceeds the 8 MiB limit".into());
            }
            if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
                return Err("runtime audio requires a PCM 16-bit WAV file".into());
            }
            let mut format = None;
            let mut data = None;
            let mut offset = 12usize;
            while offset + 8 <= bytes.len() {
                let size =
                    u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
                let start = offset + 8;
                let end = start
                    .checked_add(size)
                    .filter(|end| *end <= bytes.len())
                    .ok_or("invalid WAV chunk length")?;
                match &bytes[offset..offset + 4] {
                    b"fmt " if size >= 16 => {
                        let u16_at = |at| {
                            u16::from_le_bytes(
                                bytes[start + at..start + at + 2].try_into().unwrap(),
                            )
                        };
                        let u32_at = |at| {
                            u32::from_le_bytes(
                                bytes[start + at..start + at + 4].try_into().unwrap(),
                            )
                        };
                        format = Some(WaveFormat {
                            tag: u16_at(0),
                            channels: u16_at(2),
                            rate: u32_at(4),
                            bytes_per_second: u32_at(8),
                            block_align: u16_at(12),
                            bits: u16_at(14),
                            extra: 0,
                        });
                    }
                    b"data" => {
                        data = Some(bytes[start..end].to_vec().into_boxed_slice());
                    }
                    _ => {}
                }
                offset = end + (size & 1);
            }
            let format = format.ok_or("WAV format chunk is missing")?;
            let mut data = data.ok_or("WAV data chunk is missing")?;
            if format.tag != 1
                || format.bits != 16
                || !(1..=2).contains(&format.channels)
                || !(8000..=192000).contains(&format.rate)
                || format.block_align != format.channels * 2
                || format.bytes_per_second != format.rate * format.block_align as u32
                || data.is_empty()
                || data.len() % format.block_align as usize != 0
            {
                return Err(
                    "unsupported WAV; expected PCM 16-bit mono/stereo with valid sample alignment"
                        .into(),
                );
            }
            let mut handle = std::ptr::null_mut();
            // The callback only updates an atomic flag; it never reads driver-owned headers.
            let done = Box::new(AtomicBool::new(false));
            check(
                unsafe {
                    waveOutOpen(
                        &mut handle,
                        u32::MAX,
                        &format,
                        completion as *const () as usize,
                        done.as_ref() as *const AtomicBool as usize,
                        0x00030000,
                    )
                },
                "open",
            )?;
            let header = Box::new(UnsafeCell::new(WaveHeader {
                data: data.as_mut_ptr().cast(),
                length: data.len() as u32,
                recorded: 0,
                user: 0,
                flags: if looping { 4 | 8 } else { 0 },
                loops: if looping { u32::MAX } else { 0 },
                next: std::ptr::null_mut(),
                reserved: 0,
            }));
            let mut voice = Self {
                handle,
                header: Some(header),
                data: Some(data),
                done: Some(done),
                prepared: false,
                paused: false,
            };
            let ptr = voice.header.as_ref().unwrap().get();
            check(
                unsafe {
                    waveOutPrepareHeader(handle, ptr, std::mem::size_of::<WaveHeader>() as u32)
                },
                "prepare",
            )?;
            voice.prepared = true;
            voice.set_volume(volume)?;
            check(
                unsafe { waveOutWrite(handle, ptr, std::mem::size_of::<WaveHeader>() as u32) },
                "play",
            )?;
            Ok(voice)
        }
        pub fn set_volume(&mut self, volume: f32) -> Result<(), String> {
            if !volume.is_finite() {
                return Err("audio volume must be finite".into());
            }
            let volume = (volume.clamp(0.0, 1.0) * u16::MAX as f32) as u32;
            check(
                unsafe { waveOutSetVolume(self.handle, volume | (volume << 16)) },
                "volume",
            )
        }
        pub fn finished(&self) -> bool {
            self.done
                .as_ref()
                .is_none_or(|done| done.load(Ordering::Acquire))
        }
        pub fn set_paused(&mut self, paused: bool) {
            if self.paused == paused {
                return;
            }
            let result = unsafe {
                if paused {
                    waveOutPause(self.handle)
                } else {
                    waveOutRestart(self.handle)
                }
            };
            if result == 0 {
                self.paused = paused;
            }
        }
    }
    impl Drop for Voice {
        fn drop(&mut self) {
            unsafe {
                waveOutReset(self.handle);
                let result = if self.prepared {
                    self.header
                        .as_ref()
                        .map(|h| {
                            waveOutUnprepareHeader(
                                self.handle,
                                h.get(),
                                std::mem::size_of::<WaveHeader>() as u32,
                            )
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                if result != 0 {
                    DRIVER_FAILED.store(true, Ordering::Relaxed);
                    // The driver still owns the pointers. Preserve them rather than
                    // freeing live DMA buffers; this exceptional leak is bounded per voice.
                    if let Some(header) = self.header.take() {
                        std::mem::forget(header);
                    }
                    if let Some(data) = self.data.take() {
                        std::mem::forget(data);
                    }
                }
                if waveOutClose(self.handle) != 0 {
                    DRIVER_FAILED.store(true, Ordering::Relaxed);
                    if let Some(done) = self.done.take() {
                        std::mem::forget(done);
                    }
                }
            }
        }
    }
    unsafe extern "system" fn completion(
        _: *mut c_void,
        message: u32,
        instance: usize,
        _: usize,
        _: usize,
    ) {
        if message == 0x03BD && instance != 0 {
            // WOM_DONE. Callback allocation remains alive until the device closes.
            (*(instance as *const AtomicBool)).store(true, Ordering::Release);
        }
    }
    fn check(result: u32, operation: &str) -> Result<(), String> {
        if result == 0 {
            Ok(())
        } else {
            Err(format!("Windows audio {operation} failed ({result})"))
        }
    }
}
