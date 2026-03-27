use std::io::{self, Write};
use std::process::Command;
use std::str::FromStr;
use std::thread;

#[cfg(target_os = "linux")]
use std::path::Path;

use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionSound {
    #[default]
    None,
    Beep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionSoundConfig {
    pub sound: CompletionSound,
    pub volume_percent: u8,
}

impl CompletionSoundConfig {
    pub fn is_enabled(self) -> bool {
        self.sound != CompletionSound::None && self.volume_percent > 0
    }

    pub fn clamped_volume_percent(self) -> u8 {
        self.volume_percent.min(100)
    }
}

impl Default for CompletionSoundConfig {
    fn default() -> Self {
        Self {
            sound: CompletionSound::None,
            volume_percent: 100,
        }
    }
}

impl CompletionSound {
    pub fn from_settings_value(s: &str) -> Option<Self> {
        Self::from_str(s).ok()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Beep => "beep",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Beep,
            Self::Beep => Self::None,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

impl FromStr for CompletionSound {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "beep" => Ok(Self::Beep),
            _ => Err(()),
        }
    }
}

pub fn play_completion_sound(config: CompletionSoundConfig) {
    if !config.is_enabled() {
        debug!(?config, "completion sound skipped before audio init");
        return;
    }

    thread::spawn(move || {
        if let Err(err) = play_completion_sound_blocking(config) {
            warn!(error = %err, ?config, "failed to play completion sound");
        }
    });
}

fn play_completion_sound_blocking(config: CompletionSoundConfig) -> io::Result<()> {
    match config.sound {
        CompletionSound::None => Ok(()),
        CompletionSound::Beep => play_platform_beep(config.clamped_volume_percent()),
    }
}

#[cfg(target_os = "macos")]
fn play_platform_beep(volume_percent: u8) -> io::Result<()> {
    let volume = format!("{:.2}", f32::from(volume_percent) / 100.0);
    let sound_path = "/System/Library/Sounds/Glass.aiff";

    match Command::new("afplay")
        .args(["-v", volume.as_str(), sound_path])
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            debug!(exit_code = ?status.code(), "afplay failed; falling back to terminal bell");
            play_terminal_bell()
        }
        Err(err) => {
            debug!(error = %err, "afplay unavailable; falling back to terminal bell");
            play_terminal_bell()
        }
    }
}

#[cfg(target_os = "linux")]
fn play_platform_beep(volume_percent: u8) -> io::Result<()> {
    if try_play_with_paplay(volume_percent)? {
        return Ok(());
    }

    if try_play_with_canberra()? {
        return Ok(());
    }

    debug!("linux completion sound commands unavailable; falling back to terminal bell");
    play_terminal_bell()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn play_platform_beep(_volume_percent: u8) -> io::Result<()> {
    debug!("completion sound falls back to terminal bell on this OS");
    play_terminal_bell()
}

#[cfg(target_os = "linux")]
fn try_play_with_paplay(volume_percent: u8) -> io::Result<bool> {
    const SOUND_CANDIDATES: [&str; 3] = [
        "/usr/share/sounds/freedesktop/stereo/complete.oga",
        "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga",
        "/usr/share/sounds/freedesktop/stereo/bell.oga",
    ];

    let pulse_volume = ((u32::from(volume_percent) * 65_536) / 100)
        .max(1)
        .to_string();

    for sound_path in SOUND_CANDIDATES {
        if !Path::new(sound_path).exists() {
            continue;
        }

        match Command::new("paplay")
            .args(["--volume", pulse_volume.as_str(), sound_path])
            .status()
        {
            Ok(status) if status.success() => return Ok(true),
            Ok(status) => {
                debug!(
                    path = sound_path,
                    exit_code = ?status.code(),
                    "paplay failed for completion sound candidate"
                );
            }
            Err(err) => {
                debug!(error = %err, path = sound_path, "paplay unavailable for completion sound");
                return Ok(false);
            }
        }
    }

    Ok(false)
}

#[cfg(target_os = "linux")]
fn try_play_with_canberra() -> io::Result<bool> {
    for event_id in ["complete", "message-new-instant", "bell-terminal"] {
        match Command::new("canberra-gtk-play")
            .args(["-i", event_id, "-d", "opencode-kanban"])
            .status()
        {
            Ok(status) if status.success() => return Ok(true),
            Ok(status) => {
                debug!(
                    event_id,
                    exit_code = ?status.code(),
                    "canberra-gtk-play failed for completion sound candidate"
                );
            }
            Err(err) => {
                debug!(error = %err, "canberra-gtk-play unavailable for completion sound");
                return Ok(false);
            }
        }
    }

    Ok(false)
}

fn play_terminal_bell() -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(b"\x07")?;
    stderr.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_sound_from_settings_value_parses_supported_values() {
        assert_eq!(
            CompletionSound::from_settings_value("none"),
            Some(CompletionSound::None)
        );
        assert_eq!(
            CompletionSound::from_settings_value("beep"),
            Some(CompletionSound::Beep)
        );
        assert_eq!(
            CompletionSound::from_settings_value(" BEEP "),
            Some(CompletionSound::Beep)
        );
        assert_eq!(CompletionSound::from_settings_value("invalid"), None);
    }

    #[test]
    fn completion_sound_roundtrips_to_settings_values() {
        for sound in [CompletionSound::None, CompletionSound::Beep] {
            assert_eq!(
                CompletionSound::from_settings_value(sound.as_str()),
                Some(sound)
            );
        }
    }

    #[test]
    fn completion_sound_cycles_between_none_and_beep() {
        assert_eq!(CompletionSound::None.next(), CompletionSound::Beep);
        assert_eq!(CompletionSound::Beep.next(), CompletionSound::None);
        assert_eq!(CompletionSound::None.previous(), CompletionSound::Beep);
        assert_eq!(CompletionSound::Beep.previous(), CompletionSound::None);
    }

    #[test]
    fn completion_sound_config_reports_enabled_state() {
        assert!(!CompletionSoundConfig::default().is_enabled());
        assert!(
            !CompletionSoundConfig {
                sound: CompletionSound::Beep,
                volume_percent: 0,
            }
            .is_enabled()
        );
        assert!(
            CompletionSoundConfig {
                sound: CompletionSound::Beep,
                volume_percent: 25,
            }
            .is_enabled()
        );
    }
}
