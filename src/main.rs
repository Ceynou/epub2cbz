#[cfg(feature = "cli")]
mod cli;

#[cfg(feature = "gui")]
mod gui;

fn main() {
    #[cfg(feature = "gui")]
    {
        if std::env::args().len() <= 1 {
            gui::run_gui();
            return;
        }
    }

    #[cfg(feature = "cli")]
    {
        if let Err(e) = cli::run_cli() {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }

    #[cfg(not(any(feature = "cli", feature = "gui")))]
    {
        eprintln!("No features enabled. Build with --features cli or --features gui");
        std::process::exit(1);
    }
}
