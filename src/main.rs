// Lettore musicale CP 2.0: disco in vinile, playlist, tag e copertine.
// Interfaccia moderna, tema chiaro/scuro e lingua italiana o inglese.
#![windows_subsystem = "windows"]

use eframe::egui::{
    self, Align2, Color32, FontId, Rect, RichText, Rounding, Sense, Stroke, Vec2,
};
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

const ACCENT: Color32 = Color32::from_rgb(58, 140, 235);
const ACCENT_DEEP: Color32 = Color32::from_rgb(38, 106, 189);
const PAYPAL: Color32 = Color32::from_rgb(0, 112, 186);
const ERROR: Color32 = Color32::from_rgb(214, 82, 82);
const VERSION: &str = "2.0";
const HOMEPAGE: &str = "https://github.com/gsepp86/VISUAL-IMMAGINI-CP";
const DONATE_URL: &str = "https://paypal.me/PeppeCP";
const SPIN_SPEED: f32 = 1.6; // radianti al secondo
const FF_RATE: u32 = 48_000;
const FF_CHANNELS: u16 = 2;

// ---------------------------------------------------------------------------
// Lingue
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    It,
    En,
}

struct Tr {
    window_title: &'static str,
    app_name: &'static str,
    menu_file: &'static str,
    menu_settings: &'static str,
    add_files: &'static str,
    audio_filter: &'static str,
    playlist: &'static str,
    playlist_empty: &'static str,
    no_track: &'static str,
    untitled: &'static str,
    prev: &'static str,
    play: &'static str,
    pause: &'static str,
    next: &'static str,
    show_solo: &'static str,
    show_playlist: &'static str,
    about: &'static str,
    settings: &'static str,
    appearance: &'static str,
    theme_dark: &'static str,
    theme_light: &'static str,
    language: &'static str,
    language_hint: &'static str,
    about_desc: &'static str,
    about_author: &'static str,
    donate_text: &'static str,
    donate_button: &'static str,
    source_code: &'static str,
    err_no_device: &'static str,
    err_missing_file: &'static str,
    err_ffmpeg: &'static str,
    err_playback: &'static str,
}

const TR_IT: Tr = Tr {
    window_title: "Lettore musicale CP",
    app_name: "Lettore Musicale CP",
    menu_file: "File",
    menu_settings: "Impostazioni",
    add_files: "Aggiungi file…",
    audio_filter: "Audio",
    playlist: "Playlist",
    playlist_empty: "Aggiungi dei file audio dal menu File.",
    no_track: "Nessun brano",
    untitled: "Senza titolo",
    prev: "Brano precedente",
    play: "Riproduci",
    pause: "Pausa",
    next: "Brano successivo",
    show_solo: "Mostra solo brano",
    show_playlist: "Mostra playlist",
    about: "Informazioni",
    settings: "Impostazioni",
    appearance: "Aspetto",
    theme_dark: "Scuro",
    theme_light: "Chiaro",
    language: "Lingua",
    language_hint: "La lingua si applica subito a tutta l'interfaccia.",
    about_desc: "Lettore musicale moderno e senza fronzoli: disco in vinile, playlist, tag e \
                 copertine, formati audio ampi, tema chiaro e scuro, italiano e inglese.",
    about_author: "Lettore Musicale CP, opensource, creato da Peppe CP in collaborazione con IA.",
    donate_text: "Se vuoi contribuire al mio lavoro visto il tempo che ti farò risparmiare quando \
                  stampi tanti file ti lascio in basso il pulsante per contribuire anche con una \
                  piccola donazione simbolica visto che una app così utile gratis per sempre non la \
                  trovi ovunque apprezzo il gesto ;-)",
    donate_button: "Dona con PayPal",
    source_code: "Codice sorgente:",
    err_no_device: "Nessun dispositivo audio disponibile",
    err_missing_file: "File non trovato",
    err_ffmpeg: "Formato non supportato: serve ffmpeg.exe accanto al programma o nel PATH",
    err_playback: "Impossibile riprodurre questo file",
};

const TR_EN: Tr = Tr {
    window_title: "Music Player CP",
    app_name: "Music Player CP",
    menu_file: "File",
    menu_settings: "Settings",
    add_files: "Add files…",
    audio_filter: "Audio",
    playlist: "Playlist",
    playlist_empty: "Add audio files from the File menu.",
    no_track: "No track",
    untitled: "Untitled",
    prev: "Previous track",
    play: "Play",
    pause: "Pause",
    next: "Next track",
    show_solo: "Show track only",
    show_playlist: "Show playlist",
    about: "About",
    settings: "Settings",
    appearance: "Appearance",
    theme_dark: "Dark",
    theme_light: "Light",
    language: "Language",
    language_hint: "The language is applied to the whole interface right away.",
    about_desc: "A modern, no-frills music player: spinning vinyl, playlist, tags and cover art, \
                 wide format support, light and dark theme, Italian and English.",
    about_author: "Music Player CP, open source, made by Peppe CP together with AI.",
    donate_text: "If you'd like to support my work, considering the time it will save you when you \
                  print lots of files, you'll find the button below to contribute with a small \
                  symbolic donation: an app this useful, free forever, isn't easy to find, so I \
                  appreciate it ;-)",
    donate_button: "Donate with PayPal",
    source_code: "Source code:",
    err_no_device: "No audio output device available",
    err_missing_file: "File not found",
    err_ffmpeg: "Unsupported format: ffmpeg.exe is required next to the program or in PATH",
    err_playback: "This file cannot be played",
};

impl Lang {
    fn tr(self) -> &'static Tr {
        match self {
            Lang::It => &TR_IT,
            Lang::En => &TR_EN,
        }
    }
}

// ---------------------------------------------------------------------------
// Brani e metadati
// ---------------------------------------------------------------------------

struct Track {
    path: PathBuf,
    /// Vuoto quando il file non ha un tag titolo: si usa la stringa tradotta.
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
        .unwrap_or_default();
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

#[derive(Clone, Copy)]
enum PlayerError {
    NoDevice,
    MissingFile,
    Ffmpeg,
    Playback,
}

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
    fn open(path: &Path, duration: Option<Duration>, start: Duration) -> Result<Self, PlayerError> {
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

        let mut child = cmd.spawn().map_err(|_| PlayerError::Ffmpeg)?;
        let stdout = child.stdout.take().ok_or(PlayerError::Playback)?;
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
    fn new() -> Result<Self, PlayerError> {
        let (stream, handle) =
            OutputStream::try_default().map_err(|_| PlayerError::NoDevice)?;
        Ok(Self {
            _stream: stream,
            handle,
            sink: None,
            duration: None,
            played: Duration::ZERO,
        })
    }

    /// Avvia un brano dall'inizio.
    fn load(&mut self, track: &Track) -> Result<(), PlayerError> {
        if !track.path.exists() {
            return Err(PlayerError::MissingFile);
        }
        let sink = Sink::try_new(&self.handle).map_err(|_| PlayerError::Playback)?;

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
    ) -> Result<(), PlayerError> {
        let sink = Sink::try_new(&self.handle).map_err(|_| PlayerError::Playback)?;
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
// Tema e stile
// ---------------------------------------------------------------------------

fn card_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(28, 30, 36)
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}

fn muted(dark: bool) -> Color32 {
    if dark {
        Color32::from_gray(150)
    } else {
        Color32::from_gray(96)
    }
}

/// Applica un aspetto curato: angoli arrotondati, tipografia più ariosa,
/// colori di accento coerenti in entrambi i temi.
fn apply_style(ctx: &egui::Context, dark: bool) {
    let mut style = (*ctx.style()).clone();
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = if dark {
        Color32::from_rgb(17, 19, 23)
    } else {
        Color32::from_rgb(244, 246, 250)
    };
    v.window_fill = card_bg(dark);
    v.faint_bg_color = if dark {
        Color32::from_rgb(23, 25, 30)
    } else {
        Color32::from_rgb(238, 241, 246)
    };
    v.extreme_bg_color = if dark {
        Color32::from_rgb(41, 44, 52)
    } else {
        Color32::from_rgb(224, 228, 235)
    };
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    v.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 6.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(70),
    };
    v.popup_shadow = v.window_shadow;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(8.0);
    }
    v.widgets.hovered.weak_bg_fill = if dark {
        Color32::from_rgb(43, 46, 55)
    } else {
        Color32::from_rgb(232, 236, 243)
    };

    style.visuals = v;
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.slider_width = 180.0;
    style.spacing.scroll.bar_width = 8.0;

    style.text_styles = [
        (egui::TextStyle::Small, FontId::proportional(11.0)),
        (egui::TextStyle::Body, FontId::proportional(14.0)),
        (egui::TextStyle::Button, FontId::proportional(14.0)),
        (egui::TextStyle::Heading, FontId::proportional(20.0)),
        (egui::TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Icone disegnate a codice (non dipendono da font)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Icon {
    Prev,
    Next,
    /// Disco: usato per "Mostra solo brano"
    Solo,
    /// Tre righe: usato per "Mostra playlist"
    Playlist,
    /// Cerchio con la "i": informazioni sul programma
    Info,
}

/// Pulsante icona. Ritorna true quando viene cliccato.
fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, active: bool, size: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fg = ui.visuals().text_color();
    let bg = if active {
        ui.visuals().selection.bg_fill.gamma_multiply(0.85)
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), bg);
    let c = rect.center();
    let s = size / 34.0; // scala rispetto al disegno base
    let v = |x: f32, y: f32| c + Vec2::new(x * s, y * s);
    let stroke = Stroke::new(1.6 * s, fg);

    match icon {
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
            p.circle_stroke(c, 10.0 * s, stroke);
            p.circle_filled(c, 3.0 * s, fg);
        }
        Icon::Info => {
            p.circle_stroke(c, 10.0 * s, stroke);
            p.circle_filled(v(0.0, -4.5), 1.6 * s, fg);
            p.rect_filled(Rect::from_min_max(v(-1.6, -1.5), v(1.6, 6.0)), 1.0, fg);
        }
        Icon::Playlist => {
            for dy in [-6.0, 0.0, 6.0] {
                p.line_segment([v(-9.0, dy), v(9.0, dy)], stroke);
            }
        }
    }

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
        .clicked()
}

/// Pulsante principale di riproduzione: cerchio pieno con accento.
fn play_button(ui: &mut egui::Ui, playing: bool, tooltip: &str) -> bool {
    let size = 46.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fill = if resp.hovered() {
        ACCENT_DEEP.linear_multiply(1.0)
    } else {
        ACCENT
    };
    let p = ui.painter();
    p.circle_filled(rect.center(), size * 0.5, fill);
    let c = rect.center();
    let v = |x: f32, y: f32| c + Vec2::new(x, y);
    if playing {
        p.rect_filled(Rect::from_min_max(v(-6.0, -8.0), v(-2.0, 8.0)), 1.5, Color32::WHITE);
        p.rect_filled(Rect::from_min_max(v(2.0, -8.0), v(6.0, 8.0)), 1.5, Color32::WHITE);
    } else {
        p.add(egui::Shape::convex_polygon(
            vec![v(-5.0, -9.0), v(-5.0, 9.0), v(9.0, 0.0)],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
        .clicked()
}

// ---------------------------------------------------------------------------
// Barra di avanzamento cliccabile
// ---------------------------------------------------------------------------

/// Disegna la barra e ritorna (frazione trascinata in corso, frazione da applicare).
fn seek_bar(
    ui: &mut egui::Ui,
    shown: f32,
    drag_now: Option<f32>,
    width: f32,
    dark: bool,
) -> (Option<f32>, Option<f32>) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::click_and_drag());
    let track_bg = if dark {
        Color32::from_rgb(48, 51, 60)
    } else {
        Color32::from_rgb(218, 222, 230)
    };
    let p = ui.painter();

    let hot = resp.hovered() || resp.dragged();
    let bar = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), if hot { 8.0 } else { 6.0 }));
    p.rect_filled(bar, Rounding::same(4.0), track_bg);
    let filled = Rect::from_min_size(bar.min, Vec2::new(bar.width() * shown, bar.height()));
    p.rect_filled(filled, Rounding::same(4.0), ACCENT);
    let handle = egui::pos2(bar.min.x + bar.width() * shown, bar.center().y);
    p.circle_filled(handle, if hot { 7.0 } else { 5.0 }, ACCENT);
    p.circle_filled(handle, if hot { 3.0 } else { 2.0 }, Color32::WHITE);

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
    init_error: Option<PlayerError>,
    tracks: Vec<Track>,
    current: Option<usize>,
    show_list: bool,
    dark: bool,
    lang: Lang,
    applied_lang: Option<Lang>,
    show_settings: bool,
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
            lang: Lang::It,
            applied_lang: None,
            show_settings: false,
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

    fn tr(&self) -> &'static Tr {
        self.lang.tr()
    }

    fn error_text(&self, err: PlayerError, path: Option<&Path>) -> String {
        let t = self.tr();
        match err {
            PlayerError::NoDevice => t.err_no_device.to_string(),
            PlayerError::MissingFile => match path {
                Some(p) => format!("{}: {}", t.err_missing_file, p.display()),
                None => t.err_missing_file.to_string(),
            },
            PlayerError::Ffmpeg => t.err_ffmpeg.to_string(),
            PlayerError::Playback => t.err_playback.to_string(),
        }
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
            .add_filter(self.tr().audio_filter, &exts[..])
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
        let result = match self.player.as_mut() {
            Some(player) => player.load(&self.tracks[index]),
            None => return,
        };
        match result {
            Ok(()) => {
                self.current = Some(index);
                self.message = None;
            }
            Err(e) => {
                let path = self.tracks[index].path.clone();
                self.message = Some(self.error_text(e, Some(&path)));
            }
        }
    }

    /// Salta a una posizione del brano corrente.
    fn seek(&mut self, target: Duration) {
        let Some(idx) = self.current else {
            return;
        };
        let path = self.tracks[idx].path.clone();
        let duration = self.tracks[idx].duration;
        let seeked = self
            .player
            .as_mut()
            .map_or(true, |player| player.try_seek(target));
        if seeked {
            return;
        }
        let result = match self.player.as_mut() {
            Some(player) => player.load_at(&path, duration, target),
            None => return,
        };
        if let Err(e) = result {
            self.message = Some(self.error_text(e, Some(&path)));
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

    fn track_title(&self, index: usize) -> String {
        let title = &self.tracks[index].title;
        if title.trim().is_empty() {
            self.tr().untitled.to_string()
        } else {
            title.clone()
        }
    }

    fn title(&self) -> String {
        self.current
            .map(|i| self.track_title(i))
            .unwrap_or_else(|| self.tr().no_track.to_string())
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

    // Ombra morbida sotto il disco
    p.circle_filled(c + Vec2::new(0.0, r * 0.06), r, Color32::from_black_alpha(if dark { 90 } else { 30 }));
    p.circle_filled(c + Vec2::new(0.0, r * 0.02), r, Color32::from_black_alpha(if dark { 60 } else { 18 }));

    // Corpo del disco
    let body = if dark {
        Color32::from_gray(21)
    } else {
        Color32::from_gray(38)
    };
    p.circle_filled(c, r, body);

    // Bordo luminoso
    p.circle_stroke(c, r - 1.0, Stroke::new(1.0_f32, Color32::from_white_alpha(26)));

    // Solchi fissi
    for k in 1..=8 {
        let rr = r * (0.36 + 0.07 * k as f32);
        p.circle_stroke(c, rr, Stroke::new(1.0_f32, Color32::from_white_alpha(14)));
    }

    // Riflessi che ruotano: sono ciò che rende visibile la rotazione
    for k in 0..3 {
        let a = angle + k as f32 * TAU / 3.0;
        let dir = Vec2::new(a.cos(), a.sin());
        p.line_segment(
            [c + dir * r * 0.36, c + dir * r * 0.93],
            Stroke::new(r * 0.10, Color32::from_white_alpha(20)),
        );
    }

    // Etichetta: copertina se presente, altrimenti colore di accento
    match cover {
        Some(tex) => draw_cover_disc(p, tex.id(), c, r * 0.32, angle),
        None => {
            p.circle_filled(c, r * 0.30, ACCENT);
            p.circle_stroke(c, r * 0.30, Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
        }
    }

    // Puntino che gira sull'etichetta
    let dot = c + Vec2::new(angle.cos(), angle.sin()) * r * 0.20;
    p.circle_filled(dot, r * 0.035, Color32::WHITE);
    p.circle_filled(c, r * 0.04, Color32::BLACK); // foro del perno
}

/// Bottone pieno usato per le donazioni e i link esterni.
fn pill_link(ui: &mut egui::Ui, label: &str, url: &str, fill: Color32) {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(15.0),
        Color32::WHITE,
    );
    let size = galley.size() + Vec2::new(30.0, 16.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let bg = if resp.hovered() {
        fill.linear_multiply(1.15)
    } else {
        fill
    };
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), bg);
    p.galley(rect.center() - galley.size() * 0.5, galley, Color32::WHITE);
    if resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(url)
        .clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

/// Controllo segmentato per le scelte delle impostazioni.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, options: &[(T, &str)]) {
    ui.horizontal(|ui| {
        for (value, label) in options {
            let selected = *current == *value;
            let text = RichText::new(*label).size(14.0);
            let text = if selected {
                text.color(Color32::WHITE).strong()
            } else {
                text.color(ui.visuals().text_color())
            };
            let button = egui::Button::new(text)
                .fill(if selected {
                    ACCENT
                } else {
                    ui.visuals().widgets.inactive.weak_bg_fill
                })
                .rounding(Rounding::same(9.0))
                .min_size(Vec2::new(108.0, 28.0));
            if ui.add(button).clicked() {
                *current = *value;
            }
        }
    });
}

/// Finestra delle impostazioni: tema e lingua.
fn settings_window(ctx: &egui::Context, open: &mut bool, dark: &mut bool, lang: &mut Lang) {
    let t = lang.tr();
    egui::Window::new(t.settings)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(310.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(2.0);
            ui.label(RichText::new(t.appearance).size(15.0).strong());
            ui.add_space(4.0);
            segmented(
                ui,
                dark,
                &[(true, t.theme_dark), (false, t.theme_light)],
            );
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(RichText::new(t.language).size(15.0).strong());
            ui.add_space(4.0);
            segmented(ui, lang, &[(Lang::It, "Italiano"), (Lang::En, "English")]);
            ui.add_space(6.0);
            ui.label(RichText::new(t.language_hint).small().weak());
            ui.add_space(4.0);
        });
}

/// Finestra "Informazioni", aperta dal pulsante con la "i".
fn info_window(ctx: &egui::Context, open: &mut bool, lang: Lang) {
    let t = lang.tr();
    egui::Window::new(t.about)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(400.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(t.app_name).size(19.0).strong());
                ui.label(RichText::new(format!("v{VERSION}")).color(ACCENT).strong().size(15.0));
            });
            ui.add_space(8.0);
            ui.label(t.about_desc);
            ui.add_space(8.0);
            ui.label(RichText::new(t.about_author).weak());
            ui.add_space(14.0);

            egui::Frame::none()
                .fill(Color32::from_black_alpha(0))
                .show(ui, |ui| {
                    ui.label(t.donate_text);
                    ui.add_space(10.0);
                    pill_link(ui, t.donate_button, DONATE_URL, PAYPAL);
                });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(t.source_code).weak());
                ui.hyperlink_to(HOMEPAGE, HOMEPAGE);
            });
            ui.add_space(2.0);
        });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_style(ctx, self.dark);

        // Titolo della finestra allineato alla lingua scelta
        if self.applied_lang != Some(self.lang) {
            self.applied_lang = Some(self.lang);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tr().window_title.to_string(),
            ));
        }

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

        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(10.0, 4.0)))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.label(
                        RichText::new(self.tr().app_name)
                            .strong()
                            .size(15.0)
                            .color(ACCENT),
                    );
                    ui.separator();
                    ui.menu_button(self.tr().menu_file, |ui| {
                        if ui.button(self.tr().add_files).clicked() {
                            self.add_files();
                            ui.close_menu();
                        }
                    });
                    if ui.button(self.tr().menu_settings).clicked() {
                        self.show_settings = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (icon, tip) = if self.show_list {
                            (Icon::Solo, self.tr().show_solo)
                        } else {
                            (Icon::Playlist, self.tr().show_playlist)
                        };
                        if icon_button(ui, icon, tip, !self.show_list, 28.0) {
                            self.show_list = !self.show_list;
                        }
                        if icon_button(ui, Icon::Info, self.tr().about, self.show_info, 28.0) {
                            self.show_info = !self.show_info;
                        }
                    });
                });
            });

        settings_window(ctx, &mut self.show_settings, &mut self.dark, &mut self.lang);
        info_window(ctx, &mut self.show_info, self.lang);

        // Playlist a destra
        if self.show_list {
            egui::SidePanel::right("playlist")
                .resizable(true)
                .default_width(250.0)
                .frame(
                    egui::Frame::none()
                        .fill(if self.dark {
                            Color32::from_rgb(21, 23, 28)
                        } else {
                            Color32::from_rgb(250, 251, 253)
                        })
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
                )
                .show(ctx, |ui| {
                    ui.label(RichText::new(self.tr().playlist).size(16.0).strong());
                    ui.add_space(6.0);
                    if self.tracks.is_empty() {
                        ui.label(RichText::new(self.tr().playlist_empty).weak());
                    }
                    let mut clicked = None;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for i in 0..self.tracks.len() {
                                let selected = self.current == Some(i);
                                let text = match &self.tracks[i].artist {
                                    Some(a) => format!("{a} — {}", self.track_title(i)),
                                    None => self.track_title(i),
                                };
                                if ui
                                    .selectable_label(selected, RichText::new(text).size(13.0))
                                    .clicked()
                                {
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
                ui.add_space(6.0);
                let avail = ui.available_height();
                let size = (avail - 150.0)
                    .min(ui.available_width() - 24.0)
                    .clamp(120.0, 300.0);
                draw_vinyl(ui, self.angle, size, self.dark, self.cover_tex.as_ref());

                ui.add_space(10.0);
                ui.label(RichText::new(self.title()).size(19.0).strong());
                if let Some(a) = self.artist() {
                    ui.label(RichText::new(a).size(14.0).color(muted(self.dark)));
                }
                ui.add_space(8.0);

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
                let bar_w = (ui.available_width() - 40.0).clamp(160.0, 280.0);
                let (dragging, commit) = seek_bar(ui, shown, self.seek_drag, bar_w, self.dark);
                self.seek_drag = dragging;
                if let (Some(f), Some(t)) = (commit, total) {
                    self.seek(Duration::from_secs_f32(t.as_secs_f32() * f));
                }

                let total_str = total.map_or("--:--".to_string(), fmt_time);
                ui.label(
                    RichText::new(format!("{} / {}", fmt_time(played), total_str))
                        .size(13.0)
                        .color(muted(self.dark)),
                );

                // Comandi con icone
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space(((w - 170.0) / 2.0).max(0.0));
                    if icon_button(ui, Icon::Prev, self.tr().prev, false, 36.0) {
                        self.prev();
                    }
                    let is_playing = self.player.as_ref().map_or(false, |p| p.is_playing());
                    let (tip, icon_playing) = if is_playing {
                        (self.tr().pause, true)
                    } else {
                        (self.tr().play, false)
                    };
                    if play_button(ui, icon_playing, tip) {
                        if self.current.is_some() {
                            if let Some(p) = self.player.as_mut() {
                                p.toggle();
                            }
                        } else if !self.tracks.is_empty() {
                            self.play_index(0);
                        }
                    }
                    if icon_button(ui, Icon::Next, self.tr().next, false, 36.0) {
                        self.next();
                    }
                });

                ui.add_space(6.0);
                if let Some(msg) = &self.message {
                    ui.colored_label(ERROR, msg);
                }
                if let Some(err) = self.init_error {
                    ui.colored_label(ERROR, self.error_text(err, None));
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
                (58, 140, 235)
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
            .with_title(Lang::It.tr().window_title)
            .with_icon(disc_icon(256))
            .with_inner_size([560.0, 460.0])
            .with_min_inner_size([420.0, 380.0]),
        ..Default::default()
    };
    eframe::run_native(
        Lang::It.tr().window_title,
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}
