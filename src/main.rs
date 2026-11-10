// Lettore musicale minimale per Windows: disco in vinile, playlist, tema chiaro/scuro.
#![windows_subsystem = "windows"]

use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Vec2};
use lofty::prelude::*;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::f32::consts::TAU;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// Formati letti direttamente da rodio (Symphonia).
const NATIVE_EXTS: &[&str] = &["mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "aif", "aiff"];
/// Formati che Symphonia non legge: vengono decodificati tramite ffmpeg.
const FFMPEG_EXTS: &[&str] = &["opus", "wma"];

const ACCENT: Color32 = Color32::from_rgb(55, 138, 221);
const ERROR: Color32 = Color32::from_rgb(200, 70, 70);
const VERSION: &str = "1.0";
const SPIN_SPEED: f32 = 1.6; // radianti al secondo
const FF_RATE: u32 = 48_000;
const FF_CHANNELS: u16 = 2;

// ---------------------------------------------------------------------------
// Brani e metadati
// ---------------------------------------------------------------------------

struct Track {
    path: PathBuf,
    title: String,
    artist: Option<String>,
    duration: Option<Duration>,
    cover: Option<Vec<u8>>,
}

/// Legge titolo, artista, durata e copertina dai tag. Se mancano, usa il nome file.
fn read_track(path: PathBuf) -> Track {
    let mut title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Senza titolo".into());
    let mut artist = None;
    let mut duration = None;
    let mut cover = None;

    if let Ok(tagged) = lofty::read_from_path(&path) {
        let d = tagged.properties().duration();
        if !d.is_zero() {
            duration = Some(d);
        }
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(t) = tag.title() {
                if !t.trim().is_empty() {
                    title = t.to_string();
                }
            }
            artist = tag
                .artist()
                .map(|a| a.to_string())
                .filter(|a| !a.trim().is_empty());
            cover = tag.pictures().first().map(|p| p.data().to_vec());
        }
    }

    Track {
        path,
        title,
        artist,
        duration,
        cover,
    }
}

// ---------------------------------------------------------------------------
// Decodifica tramite ffmpeg (Opus, WMA e casi che rodio non gestisce)
// ---------------------------------------------------------------------------

/// Cerca ffmpeg.exe accanto al programma, altrimenti usa quello nel PATH.
fn ffmpeg_exe() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let local = dir.join("ffmpeg.exe");
            if local.exists() {
                return local;
            }
        }
    }
    PathBuf::from("ffmpeg")
}

/// Sorgente audio che legge PCM 16 bit prodotto da ffmpeg, a partire da `start`.
struct FfmpegSource {
    child: Child,
    out: BufReader<ChildStdout>,
    duration: Option<Duration>,
}

impl FfmpegSource {
    fn open(path: &Path, duration: Option<Duration>, start: Duration) -> Result<Self, String> {
        let ss = format!("{:.3}", start.as_secs_f64());
        let mut cmd = Command::new(ffmpeg_exe());
        cmd.args(["-nostdin", "-loglevel", "error", "-ss", ss.as_str(), "-vn", "-i"])
            .arg(path)
            .args(["-f", "s16le", "-acodec", "pcm_s16le", "-ac", "2", "-ar", "48000", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // nessuna finestra di console
        }

        let mut child = cmd.spawn().map_err(|_| {
            "Formato non supportato: serve ffmpeg.exe accanto al programma o nel PATH".to_string()
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "ffmpeg: uscita non disponibile".to_string())?;
        Ok(Self {
            child,
            out: BufReader::with_capacity(64 * 1024, stdout),
            duration,
        })
    }
}

impl Iterator for FfmpegSource {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        let mut b = [0u8; 2];
        self.out.read_exact(&mut b).ok()?;
        Some(i16::from_le_bytes(b))
    }
}

impl Source for FfmpegSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        FF_CHANNELS
    }
    fn sample_rate(&self) -> u32 {
        FF_RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Riproduzione
// ---------------------------------------------------------------------------

struct Player {
    _stream: OutputStream, // va tenuto in vita, altrimenti l'audio si ferma
    handle: OutputStreamHandle,
    sink: Option<Sink>,
    duration: Option<Duration>,
    played: Duration,
}

impl Player {
    fn new() -> Result<Self, String> {
        let (stream, handle) =
            OutputStream::try_default().map_err(|e| format!("Nessun dispositivo audio: {e}"))?;
        Ok(Self {
            _stream: stream,
            handle,
            sink: None,
            duration: None,
            played: Duration::ZERO,
        })
    }

    /// Avvia un brano dall'inizio.
    fn load(&mut self, track: &Track) -> Result<(), String> {
        if !track.path.exists() {
            return Err(format!("File non trovato: {}", track.path.display()));
        }
        let sink = Sink::try_new(&self.handle).map_err(|e| e.to_string())?;

        let ext = track
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Prima rodio; se non riesce o il formato è noto per essere escluso, ffmpeg.
        let native = if FFMPEG_EXTS.contains(&ext.as_str()) {
            None
        } else {
            File::open(&track.path)
                .ok()
                .and_then(|f| Decoder::new(BufReader::new(f)).ok())
        };

        let duration = match native {
            Some(decoder) => {
                let d = track.duration.or(decoder.total_duration());
                sink.append(decoder);
                d
            }
            None => {
                let src = FfmpegSource::open(&track.path, track.duration, Duration::ZERO)?;
                sink.append(src);
                track.duration
            }
        };

        self.sink = Some(sink); // il sink precedente viene rilasciato e si ferma
        self.duration = duration;
        self.played = Duration::ZERO;
        Ok(())
    }

    /// Ricarica il brano corrente a partire da `start` tramite ffmpeg (usato
    /// quando il decoder non sa cercare una posizione).
    fn load_at(
        &mut self,
        path: &Path,
        duration: Option<Duration>,
        start: Duration,
    ) -> Result<(), String> {
        let sink = Sink::try_new(&self.handle).map_err(|e| e.to_string())?;
        let src = FfmpegSource::open(path, duration, start)?;
        sink.append(src);
        if self.sink.as_ref().map_or(false, |s| s.is_paused()) {
            sink.pause();
        }
        self.sink = Some(sink);
        self.duration = duration;
        self.played = start;
        Ok(())
    }

    /// Ricerca diretta nel decoder corrente. Ritorna false se non è supportata.
    fn try_seek(&mut self, target: Duration) -> bool {
        match self.sink.as_mut() {
            Some(sink) => {
                if sink.try_seek(target).is_ok() {
                    self.played = target;
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }

    fn toggle(&mut self) {
        if let Some(sink) = &self.sink {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    fn is_playing(&self) -> bool {
        self.sink
            .as_ref()
            .map_or(false, |s| !s.is_paused() && !s.empty())
    }

    fn is_finished(&self) -> bool {
        self.sink.as_ref().map_or(false, |s| s.empty())
    }
}

// ---------------------------------------------------------------------------
// Icone disegnate a codice (non dipendono da font)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Icon {
    Prev,
    Play,
    Pause,
    Next,
    /// Disco: usato per "Mostra solo brano"
    Solo,
    /// Tre righe: usato per "Mostra playlist"
    Playlist,
    /// Cerchio con la "i": informazioni sul programma
    Info,
}

/// Pulsante icona. Ritorna true quando viene cliccato.
fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let fg = ui.visuals().text_color();
    let bg = if active {
        ui.visuals().selection.bg_fill
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };

    let p = ui.painter();
    p.rect_filled(rect, 6.0, bg);
    let c = rect.center();
    let v = |x: f32, y: f32| c + Vec2::new(x, y);
    let stroke = Stroke::new(1.6_f32, fg);

    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![v(-5.0, -8.0), v(-5.0, 8.0), v(8.0, 0.0)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(v(-7.0, -8.0), v(-3.0, 8.0)), 1.0, fg);
            p.rect_filled(Rect::from_min_max(v(3.0, -8.0), v(7.0, 8.0)), 1.0, fg);
        }
        Icon::Prev => {
            p.rect_filled(Rect::from_min_max(v(-9.0, -8.0), v(-7.0, 8.0)), 1.0, fg);
            p.add(egui::Shape::convex_polygon(
                vec![v(-5.0, 0.0), v(8.0, -8.0), v(8.0, 8.0)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Next => {
            p.rect_filled(Rect::from_min_max(v(7.0, -8.0), v(9.0, 8.0)), 1.0, fg);
            p.add(egui::Shape::convex_polygon(
                vec![v(5.0, 0.0), v(-8.0, -8.0), v(-8.0, 8.0)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Solo => {
            p.circle_stroke(c, 10.0, stroke);
            p.circle_filled(c, 3.0, fg);
        }
        Icon::Info => {
            p.circle_stroke(c, 10.0, stroke);
            p.circle_filled(v(0.0, -4.5), 1.6, fg);
            p.rect_filled(Rect::from_min_max(v(-1.6, -1.5), v(1.6, 6.0)), 1.0, fg);
        }
        Icon::Playlist => {
            for dy in [-6.0, 0.0, 6.0] {
                p.line_segment([v(-9.0, dy), v(9.0, dy)], stroke);
            }
        }
    }

    resp.on_hover_text(tooltip).clicked()
}

// ---------------------------------------------------------------------------
// Barra di avanzamento cliccabile
// ---------------------------------------------------------------------------

/// Disegna la barra e ritorna (frazione trascinata in corso, frazione da applicare).
fn seek_bar(ui: &mut egui::Ui, shown: f32, drag_now: Option<f32>, width: f32) -> (Option<f32>, Option<f32>) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::click_and_drag());
    let track_bg = ui.visuals().extreme_bg_color;
    let p = ui.painter();

    let bar = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 6.0));
    p.rect_filled(bar, 3.0, track_bg);
    let filled = Rect::from_min_size(bar.min, Vec2::new(bar.width() * shown, bar.height()));
    p.rect_filled(filled, 3.0, ACCENT);
    p.circle_filled(
        egui::pos2(bar.min.x + bar.width() * shown, bar.center().y),
        6.0,
        ACCENT,
    );

    let mut dragging = drag_now;
    let mut commit = None;
    if let Some(pos) = resp.interact_pointer_pos() {
        let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        if resp.dragged() {
            dragging = Some(f);
        }
        if resp.clicked() {
            commit = Some(f);
        }
    }
    if resp.drag_stopped() {
        commit = drag_now;
        dragging = None;
    }
    (dragging, commit)
}

// ---------------------------------------------------------------------------
// Interfaccia
// ---------------------------------------------------------------------------

struct App {
    player: Option<Player>,
    init_error: Option<String>,
    tracks: Vec<Track>,
    current: Option<usize>,
    show_list: bool,
    dark: bool,
    angle: f32,
    last_frame: Instant,
    message: Option<String>,
    cover_tex: Option<egui::TextureHandle>,
    cover_for: Option<usize>,
    seek_drag: Option<f32>,
    show_info: bool,
}

impl App {
    fn new() -> Self {
        let (player, init_error) = match Player::new() {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e)),
        };
        let mut app = Self {
            player,
            init_error,
            tracks: Vec::new(),
            current: None,
            show_list: true,
            dark: true,
            angle: 0.0,
            last_frame: Instant::now(),
            message: None,
            cover_tex: None,
            cover_for: None,
            seek_drag: None,
            show_info: false,
        };
        app.open_from_args();
        app
    }

    /// Apre i file passati da Windows (doppio clic su un file audio associato).
    fn open_from_args(&mut self) {
        let paths: Vec<PathBuf> = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if paths.is_empty() {
            return;
        }
        for p in paths {
            self.tracks.push(read_track(p));
        }
        self.play_index(0);
    }

    fn add_files(&mut self) {
        let exts: Vec<&str> = NATIVE_EXTS
            .iter()
            .chain(FFMPEG_EXTS.iter())
            .copied()
            .collect();
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Audio", &exts[..])
            .pick_files()
        {
            for path in paths {
                self.tracks.push(read_track(path));
            }
            if self.current.is_none() && !self.tracks.is_empty() {
                self.play_index(0);
            }
        }
    }

    fn play_index(&mut self, index: usize) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        match player.load(&self.tracks[index]) {
            Ok(()) => {
                self.current = Some(index);
                self.message = None;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Salta a una posizione del brano corrente.
    fn seek(&mut self, target: Duration) {
        let Some(idx) = self.current else {
            return;
        };
        let path = self.tracks[idx].path.clone();
        let duration = self.tracks[idx].duration;
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if player.try_seek(target) {
            return;
        }
        if let Err(e) = player.load_at(&path, duration, target) {
            self.message = Some(e);
        }
    }

    fn next(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let idx = self.current.map_or(0, |i| (i + 1) % self.tracks.len());
        self.play_index(idx);
    }

    fn prev(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let idx = match self.current {
            Some(i) if i > 0 => i - 1,
            _ => self.tracks.len() - 1,
        };
        self.play_index(idx);
    }

    fn title(&self) -> String {
        self.current
            .map(|i| self.tracks[i].title.clone())
            .unwrap_or_else(|| "Nessun brano".into())
    }

    fn artist(&self) -> Option<String> {
        self.current.and_then(|i| self.tracks[i].artist.clone())
    }
}

fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Decodifica la copertina e la carica come texture quadrata.
fn load_cover(ctx: &egui::Context, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img
        .resize_to_fill(512, 512, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("cover", color, egui::TextureOptions::LINEAR))
}

/// Disegna la copertina come disco circolare che ruota con il vinile.
fn draw_cover_disc(p: &egui::Painter, tex: egui::TextureId, c: egui::Pos2, r: f32, angle: f32) {
    use egui::epaint::Vertex;
    const N: usize = 64;

    let mut mesh = egui::Mesh::with_texture(tex);
    mesh.vertices.push(Vertex {
        pos: c,
        uv: egui::pos2(0.5, 0.5),
        color: Color32::WHITE,
    });
    for i in 0..=N {
        let t = i as f32 / N as f32 * TAU;
        // La posizione ruota con angle, la UV resta fissa: l'immagine gira col disco.
        let rot = t + angle;
        mesh.vertices.push(Vertex {
            pos: c + Vec2::new(rot.cos(), rot.sin()) * r,
            uv: egui::pos2(0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin()),
            color: Color32::WHITE,
        });
    }
    for i in 0..N as u32 {
        mesh.indices.extend_from_slice(&[0, i + 1, i + 2]);
    }
    let _ = p.add(egui::Shape::mesh(mesh));
}

fn draw_vinyl(
    ui: &mut egui::Ui,
    angle: f32,
    size: f32,
    dark: bool,
    cover: Option<&egui::TextureHandle>,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let r = size * 0.5;

    // Corpo del disco
    let body = if dark {
        Color32::from_gray(22)
    } else {
        Color32::from_gray(40)
    };
    p.circle_filled(c, r, body);

    // Solchi fissi
    for k in 1..=7 {
        let rr = r * (0.42 + 0.075 * k as f32);
        p.circle_stroke(c, rr, Stroke::new(1.0_f32, Color32::from_white_alpha(16)));
    }

    // Riflessi che ruotano: sono ciò che rende visibile la rotazione
    for k in 0..3 {
        let a = angle + k as f32 * TAU / 3.0;
        let dir = Vec2::new(a.cos(), a.sin());
        p.line_segment(
            [c + dir * r * 0.36, c + dir * r * 0.93],
            Stroke::new(r * 0.10, Color32::from_white_alpha(22)),
        );
    }

    // Etichetta: copertina se presente, altrimenti colore di accento
    match cover {
        Some(tex) => draw_cover_disc(p, tex.id(), c, r * 0.32, angle),
        None => {
            p.circle_filled(c, r * 0.30, ACCENT);
        }
    }

    // Puntino che gira sull'etichetta
    let dot = c + Vec2::new(angle.cos(), angle.sin()) * r * 0.20;
    p.circle_filled(dot, r * 0.035, Color32::WHITE);
    p.circle_filled(c, r * 0.04, Color32::BLACK); // foro del perno
}

/// Finestra "Informazioni", aperta dal pulsante con la "i".
fn info_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Informazioni")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading("Lettore Musicale CP");
            ui.label(format!("Versione {VERSION}"));
            ui.add_space(6.0);
            ui.label(
                "Lettore musicale moderno e senza fronzoli: disco in vinile, playlist, \
                 tag e copertine, formati audio ampi, tema chiaro e scuro.",
            );
            ui.add_space(6.0);
            ui.label("Lettore Musicale CP, opensource, creato da Peppe CP in collaborazione con IA.");
            ui.add_space(6.0);
            ui.label("Vuoi contribuire al progetto con una donazione?");
            ui.hyperlink_to("paypal.me/PeppeCP", "https://paypal.me/PeppeCP");
            ui.add_space(6.0);
            ui.label("Codice sorgente:");
            ui.hyperlink("https://github.com/gsepp86/LettoreMusicaleCP");
        });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        // Ricarica la copertina solo quando cambia il brano
        if self.cover_for != self.current {
            self.cover_for = self.current;
            self.cover_tex = self
                .current
                .and_then(|i| self.tracks[i].cover.as_deref())
                .and_then(|bytes| load_cover(ctx, bytes));
        }

        // Tempo trascorso dall'ultimo frame
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        let playing = self.player.as_ref().map_or(false, |p| p.is_playing());
        if playing {
            self.angle = (self.angle + dt * SPIN_SPEED) % TAU;
            if let Some(p) = self.player.as_mut() {
                p.played += Duration::from_secs_f32(dt);
            }
            ctx.request_repaint();
        }

        // Passa al brano successivo quando il precedente finisce
        if self.current.is_some() && self.player.as_ref().map_or(false, |p| p.is_finished()) {
            self.next();
        }

        // Barra spaziatrice: play/pausa
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            if let Some(p) = self.player.as_mut() {
                p.toggle();
            }
        }

        // Menu in alto: File e Visualizza. A destra, l'icona "solo brano".
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Aggiungi file…").clicked() {
                        self.add_files();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Visualizza", |ui| {
                    ui.label("Tema");
                    ui.radio_value(&mut self.dark, true, "Scuro");
                    ui.radio_value(&mut self.dark, false, "Chiaro");
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (icon, tip) = if self.show_list {
                        (Icon::Solo, "Mostra solo brano")
                    } else {
                        (Icon::Playlist, "Mostra playlist")
                    };
                    if icon_button(ui, icon, tip, !self.show_list) {
                        self.show_list = !self.show_list;
                    }
                    if icon_button(ui, Icon::Info, "Informazioni", self.show_info) {
                        self.show_info = !self.show_info;
                    }
                });
            });
        });

        info_window(ctx, &mut self.show_info);

        // Playlist a destra
        if self.show_list {
            egui::SidePanel::right("playlist")
                .resizable(true)
                .default_width(280.0)
                .show(ctx, |ui| {
                    ui.heading("Playlist");
                    ui.separator();
                    if self.tracks.is_empty() {
                        ui.label("Aggiungi dei file audio dal menu File.");
                    }
                    let mut clicked = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, t) in self.tracks.iter().enumerate() {
                            let selected = self.current == Some(i);
                            let text = match &t.artist {
                                Some(a) => format!("{a} — {}", t.title),
                                None => t.title.clone(),
                            };
                            if ui.selectable_label(selected, text).clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                    if let Some(i) = clicked {
                        self.play_index(i);
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                let avail = ui.available_height();
                let size = (avail - 130.0)
                    .min(ui.available_width() - 16.0)
                    .clamp(120.0, 360.0);
                draw_vinyl(ui, self.angle, size, self.dark, self.cover_tex.as_ref());

                ui.add_space(10.0);
                ui.label(egui::RichText::new(self.title()).size(20.0).strong());
                if let Some(a) = self.artist() {
                    ui.label(a);
                }

                // Avanzamento: cliccabile e trascinabile
                let (played, total) = self
                    .player
                    .as_ref()
                    .map(|p| (p.played, p.duration))
                    .unwrap_or((Duration::ZERO, None));
                let frac_now = match total {
                    Some(t) if !t.is_zero() => {
                        (played.as_secs_f32() / t.as_secs_f32()).clamp(0.0, 1.0)
                    }
                    _ => 0.0,
                };
                let shown = self.seek_drag.unwrap_or(frac_now);
                let bar_w = (ui.available_width() - 16.0).min(280.0);
                let (dragging, commit) = seek_bar(ui, shown, self.seek_drag, bar_w);
                self.seek_drag = dragging;
                if let (Some(f), Some(t)) = (commit, total) {
                    self.seek(Duration::from_secs_f32(t.as_secs_f32() * f));
                }

                let total_str = total.map_or("--:--".into(), fmt_time);
                ui.label(format!("{} / {}", fmt_time(played), total_str));

                // Comandi con icone
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space(((w - 130.0) / 2.0).max(0.0));
                    if icon_button(ui, Icon::Prev, "Brano precedente", false) {
                        self.prev();
                    }
                    let is_playing = self.player.as_ref().map_or(false, |p| p.is_playing());
                    let (icon, tip) = if is_playing {
                        (Icon::Pause, "Pausa")
                    } else {
                        (Icon::Play, "Riproduci")
                    };
                    if icon_button(ui, icon, tip, false) {
                        if self.current.is_some() {
                            if let Some(p) = self.player.as_mut() {
                                p.toggle();
                            }
                        } else if !self.tracks.is_empty() {
                            self.play_index(0);
                        }
                    }
                    if icon_button(ui, Icon::Next, "Brano successivo", false) {
                        self.next();
                    }
                });

                if let Some(msg) = &self.message {
                    ui.add_space(6.0);
                    ui.colored_label(ERROR, msg);
                }
                if let Some(err) = &self.init_error {
                    ui.add_space(6.0);
                    ui.colored_label(ERROR, err);
                }
            });
        });
    }
}

/// Icona della finestra: un disco in vinile disegnato a codice (nessun file esterno).
fn disc_icon(size: u32) -> egui::IconData {
    let s = size as f32;
    let c = s / 2.0;
    let r = s / 2.0 - 1.0;
    let label = r * 0.38;
    let hole = r * 0.07;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            let d = (dx * dx + dy * dy).sqrt();
            // bordo esterno con antialiasing
            let alpha = ((r + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u8;

            let (cr, cg, cb) = if d <= hole {
                (14, 14, 16)
            } else if d <= label {
                (55, 138, 221)
            } else {
                // solchi: variazione sottile del nero
                let g = (d * 1.6).sin() * 0.5 + 0.5;
                let v = 20 + (g * 18.0) as u8;
                (v, v, v + 4)
            };
            rgba.extend_from_slice(&[cr, cg, cb, alpha]);
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lettore musicale")
            .with_icon(disc_icon(256))
            .with_inner_size([520.0, 400.0])
            .with_min_inner_size([400.0, 340.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Lettore musicale",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}
