//! Sound effects (legacy `AudioManager`).

use std::collections::HashMap;

use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound as Clip};
use opencity_assets::Catalog;
use opencity_core::Sound;

/// Interface sounds (Kenney Interface Sounds, CC0; `<data>/sound/ui/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ui {
    /// Something was refused or went wrong.
    Error,
    /// A message for the player.
    Notice,
    /// Something finished well (saved, discovered).
    Done,
    /// A disaster struck.
    Alarm,
    /// A tool was picked.
    Click,
}

impl Ui {
    /// The sound that suits a message, by how it reads.
    pub fn of_message(m: &str) -> Self {
        let l = m.to_lowercase();
        if ["cannot", "failed", "unreachable", "not ", "needs ", "nothing"].iter().any(|w| l.contains(w)) {
            Ui::Error
        } else if ["saved", "loaded", "opened", "founded", "imported", "borrowed", "discovered"].iter().any(|w| l.contains(w)) {
            Ui::Done
        } else {
            Ui::Notice
        }
    }
}

pub struct Audio {
    clips: HashMap<Sound, Clip>,
    ui: HashMap<Ui, Clip>,
    volume: f32,
}

impl Audio {
    pub fn disabled() -> Self {
        Self { clips: HashMap::new(), ui: HashMap::new(), volume: 0.0 }
    }

    pub async fn load(catalog: &Catalog, volume: f32) -> Self {
        let mut clips = HashMap::new();
        for (sound, file) in [
            (Sound::Destroy, "destroy.wav"),
            (Sound::PowerLine, "eline.wav"),
            (Sound::Plant, "eplant.wav"),
            (Sound::Park, "park.wav"),
            (Sound::Zone, "rci.wav"),
            (Sound::Road, "road.wav"),
            (Sound::Terrain, "terrain.wav"),
        ] {
            let path = catalog.path("sound").join(file);
            match opencity_assets::read_bytes(&path) {
                Ok(bytes) => match load_sound_from_bytes(&bytes).await {
                    Ok(c) => {
                        clips.insert(sound, c);
                    }
                    Err(e) => log::warn!("sound {}: {e}", path.display()),
                },
                Err(e) => log::warn!("sound {}: {e}", path.display()),
            }
        }
        let mut ui = HashMap::new();
        for (sound, file) in [(Ui::Error, "error.ogg"), (Ui::Notice, "notice.ogg"), (Ui::Done, "done.ogg"), (Ui::Alarm, "alarm.ogg"), (Ui::Click, "click.ogg")]
        {
            let path = catalog.path("sound").join("ui").join(file);
            // Optional: data directories without them stay quiet.
            if let Ok(bytes) = opencity_assets::read_bytes(&path) {
                match load_sound_from_bytes(&bytes).await {
                    Ok(c) => {
                        ui.insert(sound, c);
                    }
                    Err(e) => log::warn!("sound {}: {e}", path.display()),
                }
            }
        }
        Self { clips, ui, volume }
    }

    /// Plays an interface sound, a little under the tools' volume.
    pub fn play_ui(&self, s: Ui) {
        if let Some(c) = self.ui.get(&s) {
            play_sound(c, PlaySoundParams { looped: false, volume: self.volume * 0.7 });
        }
    }

    pub fn play(&self, s: Sound) {
        if let Some(c) = self.clips.get(&s) {
            play_sound(c, PlaySoundParams { looped: false, volume: self.volume });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ui;

    #[test]
    fn messages_pick_their_sound() {
        assert_eq!(Ui::of_message("Cannot open the city: no such file"), Ui::Error);
        assert_eq!(Ui::of_message("the ground is not suitable"), Ui::Error);
        assert_eq!(Ui::of_message("Saved to town.ocsave.json"), Ui::Done);
        assert_eq!(Ui::of_message("Joining http://host:8077…"), Ui::Notice);
    }
}
