//! Sound effects (legacy `AudioManager`).

use std::collections::HashMap;

use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound as Clip};
use opencity_assets::Catalog;
use opencity_core::Sound;

pub struct Audio {
    clips: HashMap<Sound, Clip>,
    volume: f32,
}

impl Audio {
    pub fn disabled() -> Self {
        Self { clips: HashMap::new(), volume: 0.0 }
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
        Self { clips, volume }
    }

    pub fn play(&self, s: Sound) {
        if let Some(c) = self.clips.get(&s) {
            play_sound(c, PlaySoundParams { looped: false, volume: self.volume });
        }
    }
}
