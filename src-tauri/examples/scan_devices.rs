// Read-only hardware diagnostic, without opening a window or sending alerts.
#[path = "../src/providers.rs"]
mod providers;

fn main() {
    match serde_json::to_string_pretty(&providers::scan_devices()) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("Could not serialize device scan: {error}");
            std::process::exit(1);
        }
    }
}
