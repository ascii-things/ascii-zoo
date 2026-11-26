//! Le scene. Ognuna riceve la griglia già pulita a ogni fotogramma e ci disegna sopra.

use crate::grid::Griglia;

mod aurora;
mod fire;
mod sea;
mod snow;
mod rain;
mod stars;
mod life;

pub trait Scena {
    /// `dt` = secondi dall'ultimo fotogramma, `t` = secondi dall'inizio della scena
    fn passo(&mut self, g: &mut Griglia, dt: f32, t: f32);
}

/// (id, nome mostrato)
pub const SCENE: &[(&str, &str)] = &[
    ("stelle", "Starfield"),
    ("pioggia", "Rain & lightning"),
    ("fuoco", "Campfire"),
    ("aurora", "Aurora"),
    ("vita", "Game of Life"),
    ("mare", "Night sea"),
    ("neve", "Snow"),
];

pub fn esiste(id: &str) -> bool {
    SCENE.iter().any(|s| s.0 == id)
}

pub fn crea(id: &str, w: usize, h: usize, seme: u64) -> Box<dyn Scena> {
    match id {
        "pioggia" => Box::new(rain::Pioggia::new(w, h, seme)),
        "fuoco" => Box::new(fire::Fuoco::new(w, h, seme)),
        "aurora" => Box::new(aurora::Aurora::new(w, h, seme)),
        "vita" => Box::new(life::Vita::new(w, h, seme)),
        "mare" => Box::new(sea::Mare::new(w, h, seme)),
        "neve" => Box::new(snow::Neve::new(w, h, seme)),
        _ => Box::new(stars::Stelle::new(w, h, seme)),
    }
}
