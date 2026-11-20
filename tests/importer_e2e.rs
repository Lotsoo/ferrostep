//! End-to-end: synthetic WAV → import thread → song folder that parses back.

use std::time::{Duration, Instant};

use ferrostep::audio_dsp::synth;
use ferrostep::importer::{ImportEvent, ImportJob};
use ferrostep::library::{scanner, test_song::write_wav};
use ferrostep::parser::load_simfile;

#[test]
fn imports_a_synthetic_song() {
    let root = std::env::temp_dir().join(format!("ferrostep-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("My Test_Song.wav");
    let mut buf = synth::drum_pattern(44_100, 40.0, 128.0, 0.4, 3);
    synth::add_noise(&mut buf, 0.02, 5);
    write_wav(&source, &buf, 44_100).unwrap();

    let songs = root.join("Songs");
    let job = ImportJob::start(source, &songs, None, eframe::egui::Context::default());
    let deadline = Instant::now() + Duration::from_secs(60);
    let result = loop {
        match job
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(ImportEvent::Done(r)) => break r,
            Ok(ImportEvent::Failed(e)) => panic!("import failed: {e}"),
            Ok(ImportEvent::Cancelled) => panic!("cancelled"),
            Ok(_) => {}
            Err(e) => panic!("no result: {e}"),
        }
    };
    println!("import took {:?}, bpm {}", result.elapsed, result.bpm());
    assert_eq!(result.dir, songs.join("Imported").join("My Test Song"));
    assert!((result.bpm() - 128.0).abs() < 0.1);

    let sim = load_simfile(&result.dir.join("song.sm")).unwrap();
    assert_eq!(sim.charts.len(), 4);
    assert_eq!(sim.meta.credit, scanner::AUTO_CREDIT);
    assert!(result.dir.join(&sim.meta.music).is_file());
    // Charts written to disk match what the importer generated.
    assert_eq!(sim.charts, result.simfile.charts);

    // The library sees it as an auto-charted, playable song; no staging folder is left.
    let found = scanner::scan(&songs);
    assert_eq!(found.len(), 1);
    assert!(found[0].is_auto && found[0].playable.len() == 4);
    let leftovers: Vec<_> = std::fs::read_dir(songs.join("Imported"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(leftovers.len(), 1);

    // BPM correction rewrites the file.
    let mut result = *result;
    result.set_bpm(64.0).unwrap();
    let sim = load_simfile(&result.dir.join("song.sm")).unwrap();
    assert_eq!(sim.timing.bpms, vec![(0.0, 64.0)]);
    let _ = std::fs::remove_dir_all(&root);
}
