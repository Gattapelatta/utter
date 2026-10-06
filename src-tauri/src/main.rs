// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(all(debug_assertions, target_os = "macos"))]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(outcome) = utter_lib::debug_deliver(&args) {
            println!("{outcome}");
            // `open` swallows stdout, so the matrix script can ask for a file too.
            if let Some(i) = args.iter().position(|a| a == "--out") {
                if let Some(path) = args.get(i + 1) {
                    let _ = std::fs::write(path, &outcome);
                }
            }
            return;
        }
    }
    utter_lib::run()
}
