//! Neve: una notte d'inverno. Nevica a tre profondità con il vento che va e viene, sugli
//! abeti con la neve sui rami e su una baita con la finestra accesa e il fumo dal camino;
//! la luna e qualche stella. (Lo stesso paesaggio dello sfondo, wallpaper/index.html.)

use super::Scena;
use crate::grid::{frattale, Caso, Griglia};

const NEVE: [u8; 3] = [225, 232, 245];
const OMBRA: [u8; 3] = [130, 146, 184];
const ABETE: [u8; 3] = [40, 110, 85];
const ABETE_CHIARO: [u8; 3] = [60, 140, 105];
const LEGNO: [u8; 3] = [140, 98, 62];
const TETTO: [u8; 3] = [170, 80, 56];
const FINESTRA: [u8; 3] = [255, 207, 115];
const FUMO: [u8; 3] = [150, 158, 175];
const LUNA: [u8; 3] = [246, 239, 200];

struct Fiocco {
    x: f32,
    y: f32,
    /// 0 lontano, 1 in mezzo, 2 vicino
    z: u8,
    v: f32,
    fase: f32,
}

/// una cella che non cambia: x, y, carattere, colore, luce
type Fissa = (i32, i32, u8, [u8; 3], f32);

pub struct Neve {
    fissi: Vec<Fissa>,
    stelle: Vec<(i32, i32, f32)>,
    fiocchi: Vec<Fiocco>,
    fumo: Vec<(f32, f32, f32)>, // x, y, età
    prossimo_fumo: f32,
    terra: Vec<f32>,
    collina: Vec<f32>,
    camino: (f32, f32),
    finestra: (i32, i32, i32, i32), // x, y, larga, alta
    caso: Caso,
}

impl Neve {
    pub fn new(w: usize, h: usize, seme: u64) -> Self {
        let mut c = Caso::new(seme);
        let (wf, hf) = (w as f32, h as f32);
        let terra: Vec<f32> =
            (0..=w).map(|i| hf * 0.8 + 1.0 * (i as f32 * 0.035 + 1.0).sin() + 0.7 * (i as f32 * 0.095 + 2.0).sin()).collect();
        let collina: Vec<f32> = (0..=w)
            .map(|i| hf * 0.67 - hf * 0.06 * ((i as f32 * 0.018 + 2.2).sin() + 1.0) - hf * 0.07 * frattale(i as f32 * 0.015 + 4.1, 2.3, 3))
            .collect();
        let mut fissi: Vec<Fissa> = Vec::new();
        let stelle = (0..w * h / 70).map(|_| (c.intero(w) as i32, c.intero(h * 11 / 20) as i32, c.tra(0.0, std::f32::consts::TAU))).collect();

        // la luna (le celle sono alte il doppio che larghe)
        let (lx, ly, lr) = (wf * c.tra(0.12, 0.24), hf * 0.17, (hf * 0.075).max(2.5));
        for y in (ly - lr) as i32..=(ly + lr) as i32 {
            for x in (lx - lr * 2.0) as i32..=(lx + lr * 2.0) as i32 {
                let (dx, dy) = ((x as f32 - lx) / 2.0, y as f32 - ly);
                let d = (dx * dx + dy * dy).sqrt() / lr;
                if d <= 1.0 {
                    let macchia = frattale(x as f32 * 0.25, y as f32 * 0.5, 2) > 0.56;
                    fissi.push((x, y, if macchia { b'%' } else { b'@' }, LUNA, if macchia { 0.7 } else { 0.95 }));
                }
            }
        }
        // le colline lontane, innevate
        for x in 0..w {
            let c0 = collina[x] as i32;
            fissi.push((x as i32, c0, b'_', OMBRA, 0.45));
            for y in c0 + 1..terra[x] as i32 {
                if frattale(x as f32 * 0.2, y as f32 * 0.6, 2) > 0.62 {
                    fissi.push((x as i32, y, b'.', OMBRA, 0.3));
                }
            }
        }
        // la baita: dove sta, per tenere lontani gli abeti vicini
        let bw = ((hf * 0.27 * 2.0).round() as i32).max(16);
        let bx = (wf * c.tra(0.52, 0.68)) as i32;
        // gli abeti: lontani (piccoli, sulle colline) e vicini (sul prato), dal fondo in avanti
        let n = (w / 14).max(6);
        let mut alberi: Vec<(f32, f32, f32, bool)> = Vec::new();
        for k in 0..n {
            let lontano = (k as f32) < n as f32 * 0.55;
            let ax = c.intero(w) as f32;
            if !lontano && (ax - bx as f32 - bw as f32 / 2.0).abs() < bw as f32 * 0.9 {
                continue;
            }
            let base = if lontano { collina[ax as usize] + 2.0 + c.tra(0.0, 3.0) } else { terra[ax as usize] + 0.5 };
            let alto = if lontano { hf * c.tra(0.07, 0.12) } else { hf * c.tra(0.17, 0.3) };
            alberi.push((ax, base, alto.max(4.0), lontano));
        }
        alberi.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(ax, base, alto, lontano) in &alberi {
            abete(&mut fissi, ax, base, alto, if lontano { 0.5 } else { 1.0 });
        }
        let (camino, finestra) = baita(&mut fissi, &terra, bx, bw);
        // la neve per terra: la superficie, sotto qualche cumulo e qualche luccichio
        for x in 0..w {
            let t0 = terra[x] as i32;
            for y in t0..h as i32 {
                let g = frattale(x as f32 * 0.04, y as f32 * 0.3, 3);
                if y == t0 {
                    fissi.push((x as i32, y, if g > 0.5 { b'~' } else { b'_' }, NEVE, 0.85));
                } else if g > 0.58 {
                    fissi.push((x as i32, y, if g > 0.66 { b':' } else { b'.' }, if g > 0.63 { NEVE } else { OMBRA }, 0.35 + (g - 0.58) * 1.5));
                }
            }
        }
        let nf = (w * h / 22).clamp(40, 2400);
        let mut neve = Neve {
            fissi,
            stelle,
            fiocchi: Vec::with_capacity(nf),
            fumo: Vec::new(),
            prossimo_fumo: 0.0,
            terra,
            collina,
            camino,
            finestra,
            caso: c,
        };
        for _ in 0..nf {
            let y = neve.caso.tra(0.0, hf);
            let f = fiocco(&mut neve.caso, wf, y);
            neve.fiocchi.push(f);
        }
        neve
    }
}

fn fiocco(c: &mut Caso, w: f32, y: f32) -> Fiocco {
    let z = if c.moneta(0.45) { 0 } else if c.moneta(0.6) { 1 } else { 2 };
    let v = [1.4, 2.6, 4.4][z as usize] * c.tra(0.8, 1.2);
    Fiocco { x: c.tra(-10.0, w + 10.0), y, z, v, fase: c.tra(0.0, std::f32::consts::TAU) }
}

/// un abete a tre piani, con la neve sul bordo di sopra di ogni piano
fn abete(fissi: &mut Vec<Fissa>, ax: f32, base: f32, alto: f32, luce: f32) {
    let tronco = (alto * 0.1).round().max(1.0) as i32;
    let corpo = alto as i32 - tronco;
    let lw = alto * 0.36 * 2.0;
    for j in 0..corpo {
        let u = j as f32 / corpo as f32;
        let p = ((u * 3.0) as i32).min(2) as f32;
        let q = u * 3.0 - p;
        let hw = lw * (0.25 + 0.75 * (p + 1.0) / 3.0) * (0.35 + 0.65 * q) + 0.3;
        let y = (base - alto) as i32 + j;
        for x in (ax - hw).round() as i32..=(ax + hw).round() as i32 {
            let dx = x as f32 - ax;
            let (ch, col, l) = if q < 0.22 {
                (if (x + y) % 3 == 0 { b'*' } else { b'~' }, NEVE, 0.8)
            } else if dx <= -hw + 1.0 {
                (b'/', ABETE_CHIARO, 0.75)
            } else if dx >= hw - 1.0 {
                (b'\\', ABETE_CHIARO, 0.75)
            } else {
                (if (x * 7 + y * 3) % 5 == 0 { b'A' } else { b'^' }, ABETE, 0.6)
            };
            fissi.push((x, y, ch, col, l * luce));
        }
    }
    for j in 0..tronco {
        fissi.push((ax as i32, base as i32 - tronco + j, b'|', LEGNO, 0.6 * luce));
    }
}

/// la baita: pareti di assi, il tetto coperto di neve, la finestra, la porta, il camino.
/// Restituisce dove esce il fumo e il rettangolo della finestra.
fn baita(fissi: &mut Vec<Fissa>, terra: &[f32], x0: i32, bw: i32) -> ((f32, f32), (i32, i32, i32, i32)) {
    let x1 = (x0 + bw).min(terra.len() as i32 - 1);
    let base = terra[x0.max(0) as usize].min(terra[x1.max(0) as usize]) as i32 + 1;
    let hm = ((bw as f32 * 0.2).round() as i32).max(3);
    let ht = ((bw as f32 * 0.25).round() as i32).max(4);
    for y in base - hm..base {
        for x in x0..x1 {
            fissi.push((x, y, if y % 2 == 0 { b'=' } else { b'-' }, LEGNO, 0.6));
        }
    }
    let (fx, fy, fw, fh) = (x0 + bw / 6, base - hm + hm / 4, (bw / 5).max(3), (hm / 3).max(1) + 1);
    let px = x0 + bw * 3 / 5;
    for y in base - hm * 2 / 3..base {
        for x in px..px + (bw / 8).max(2) {
            fissi.push((x, y, if x == px { b'[' } else { b'|' }, [92, 64, 44], 0.55));
        }
    }
    // il tetto: sporge ai lati; bordi / e \, neve sopra, la gronda rossa
    let cx = x0 as f32 + bw as f32 / 2.0;
    for j in 0..ht {
        let hw = (bw as f32 / 2.0 + 2.0) * (j + 1) as f32 / ht as f32;
        let y = base - hm - ht + j;
        for x in (cx - hw).round() as i32..=(cx + hw).round() as i32 {
            let dx = x as f32 - cx;
            let (ch, col) = if j == ht - 1 {
                (b'_', TETTO)
            } else if dx <= -hw + 1.0 {
                (b'/', TETTO)
            } else if dx >= hw - 1.0 {
                (b'\\', TETTO)
            } else {
                (if (x + j) % 4 == 0 { b'*' } else { b'~' }, NEVE)
            };
            fissi.push((x, y, ch, col, 0.85));
        }
    }
    // il camino, con la neve in cima
    let (kx, ky) = (x0 + bw * 7 / 10, base - hm - ht + ht / 2);
    for y in ky - 2..=ky {
        fissi.push((kx, y, b'[', TETTO, 0.7));
        fissi.push((kx + 1, y, b']', TETTO, 0.7));
    }
    fissi.push((kx, ky - 3, b'~', NEVE, 0.85));
    fissi.push((kx + 1, ky - 3, b'~', NEVE, 0.85));
    (((kx + 1) as f32, (ky - 4) as f32), (fx, fy, fw, fh))
}

impl Scena for Neve {
    fn passo(&mut self, g: &mut Griglia, dt: f32, t: f32) {
        let (w, h) = (g.w as f32, g.h as f32);
        let vento = 0.9 * (t * 0.045).sin() + 0.35 * (t * 0.21 + 1.3).sin();

        for &(x, y, fase) in &self.stelle {
            let l = if (t * 1.1 + fase).sin() > 0.2 { 0.45 } else { 0.2 };
            g.metti(x, y, b'.', [196, 208, 245], l, 0.0);
        }
        for &(x, y, ch, col, l) in &self.fissi {
            g.metti(x, y, ch, col, l, if col == LUNA { 0.8 } else { 0.0 });
        }
        // la finestra accesa, che tremola un poco (il fuoco dentro)
        let luce = 0.85 + 0.15 * (((t * 3.1).sin() * (t * 1.7 + 1.0).sin() * 2.0).round() / 2.0);
        let (fx, fy, fw, fh) = self.finestra;
        for y in fy..fy + fh {
            for x in fx..fx + fw {
                g.metti(x, y, if x == fx + fw / 2 { b'|' } else { b'#' }, FINESTRA, luce, 0.9);
            }
        }
        // il fumo dal camino: sbuffi che salgono, si allargano e vanno col vento
        self.prossimo_fumo -= dt;
        if self.prossimo_fumo <= 0.0 {
            self.fumo.push((self.camino.0, self.camino.1, 0.0));
            self.prossimo_fumo = self.caso.tra(0.35, 0.65);
        }
        self.fumo.retain_mut(|p| {
            p.2 += dt;
            p.1 -= dt * 1.0;
            p.0 += dt * (vento * 3.0 + 0.6 * (p.2 * 1.7).sin());
            p.2 < 7.0
        });
        for &(x, y, eta) in &self.fumo {
            let l = 0.55 * (eta * 2.0).min(1.0) * (1.0 - eta / 7.0);
            let ch = if eta < 1.5 { b'o' } else if eta < 4.0 { b'O' } else { b'.' };
            let largo = (eta * 0.6) as i32;
            for dx in -largo..=largo {
                g.metti_sopra(x as i32 + dx, y as i32, ch, FUMO, l * if dx == 0 { 1.0 } else { 0.6 }, 0.0);
            }
        }
        // i fiocchi, davanti a tutto
        let wi = self.terra.len() - 1;
        for i in 0..self.fiocchi.len() {
            let f = &mut self.fiocchi[i];
            f.x += (vento + 0.45 * (t * 0.9 + f.fase).sin()) * f.v * 1.2 * dt;
            f.y += f.v * 0.5 * dt;
            let col = (f.x.max(0.0) as usize).min(wi);
            let fine = match f.z {
                0 => self.collina[col] + 3.0,
                1 => self.terra[col],
                _ => self.terra[col] + 2.0,
            };
            if f.y > fine || f.x < -14.0 || f.x > w + 14.0 {
                let sopra = -self.caso.tra(1.0, 4.0);
                let mut nuovo = fiocco(&mut self.caso, w, sopra);
                // col vento i fiocchi entrano anche dai lati
                if vento.abs() > 0.3 && self.caso.moneta(0.3) {
                    nuovo.x = if vento > 0.0 { -2.0 } else { w + 2.0 };
                    nuovo.y = self.caso.tra(0.0, h * 0.7);
                }
                self.fiocchi[i] = nuovo;
                continue;
            }
            let (ch, col, l, alone) = match f.z {
                0 => (b'.', OMBRA, 0.5, 0.0),
                1 => (b'+', NEVE, 0.6, 0.0),
                _ => (b'*', [244, 248, 255], 0.95, 0.5),
            };
            g.metti_sopra(f.x as i32, f.y as i32, ch, col, l, alone);
        }
    }
}
