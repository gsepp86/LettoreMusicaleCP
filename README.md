# Lettore musicale CP 1.0

Lettore musicale moderno e senza fronzoli per Windows. Un disco in vinile gira mentre la musica suona, la copertina dell'album diventa l'etichetta del disco, e la playlist resta a portata di mano sulla destra.

## Se vuoi contribuire al mio lavoro ti lascio in basso il pulsante per contribuire anche con una piccola donazione simbolica visto che una app così utile gratis per sempre non la trovi ovunque apprezzo il gesto ;-)

[![Donate via PayPal](https://img.shields.io/badge/Donate-PayPal-blue.svg?logo=paypal)](https://paypal.me/PeppeCP)


## Caratteristiche

- Disco in vinile animato, che gira durante la riproduzione e si ferma in pausa
- Copertina dell'album letta dai tag, mostrata al centro del disco
- Titolo, artista e durata letti dai tag del file
- Playlist laterale ridimensionabile, con il brano in riproduzione evidenziato
- Pulsante per nascondere la playlist e lasciare solo il disco
- Barra di avanzamento cliccabile e trascinabile per saltare in un punto del brano
- Brano successivo e precedente, con avanzamento automatico alla fine di ogni brano
- Tema scuro e tema chiaro, dal menu Visualizza
- Apertura dei file con doppio clic, se il lettore è associato ai file audio

## Formati audio

Legge la maggior parte dei formati più diffusi:

- **Diretti**: MP3, FLAC, WAV, OGG, M4A/AAC, AIFF
- **Tramite ffmpeg**: Opus e WMA

Per Opus e WMA serve `ffmpeg.exe`. Se usi l'installer, metti `ffmpeg.exe` nella cartella del progetto prima di compilarlo, oppure accanto al programma installato, oppure nel PATH di Windows.

## Uso

- **File → Aggiungi file…** per scegliere uno o più brani
- **Barra spaziatrice** per riproduzione e pausa
- Clic su un brano nella playlist per riprodurlo
- Clic sulla barra di avanzamento per saltare in un punto del brano
- Icona del disco in alto a destra per nascondere o mostrare la playlist
- Icona **i** in alto a destra per le informazioni sul programma

## Installazione

Scarica `LettoreMusicaleCP-Setup-1.0.0.exe` dalla sezione Actions del repository, oppure dalla pagina delle release, ed eseguilo. L'installer crea i collegamenti e può associare i file audio al lettore.

## Compilazione dal sorgente

Serve Rust con la toolchain MSVC, disponibile su [rustup.rs](https://rustup.rs).

```
cargo build --release
```

L'eseguibile si trova in `target\release\lettore-musicale.exe`.

Per creare anche l'installer, installa [Inno Setup 6](https://jrsoftware.org/isinfo.php) e compila lo script `installer\LettoreMusicaleCP.iss`.

## Tecnologia

Scritto in Rust con [egui](https://github.com/emilk/egui) per l'interfaccia, [rodio](https://github.com/RustAudio/rodio) per la riproduzione, [lofty](https://github.com/Serial-ATA/lofty-rs) per i tag e [rfd](https://github.com/PolyMeilex/rfd) per la finestra di apertura file.


## Licenza

Da definire.
