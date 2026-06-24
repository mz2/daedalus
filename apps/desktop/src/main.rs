//! Daedalus desktop entry point.
//!
//! Wires the app service to the v1 backends and reconciles persisted sessions, then either
//! opens the native GPUI window (`--features gpui`) or — in the default headless build —
//! prints a startup summary. GPUI needs a GPU/display (research R-UI); the headless path
//! keeps the build/test flow runnable anywhere.

#[cfg(not(feature = "gpui"))]
use daedalus_app::App;
use daedalus_desktop::build_app;

fn data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("DAEDALUS_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return std::path::Path::new(&home).join(".daedalus");
    }
    std::env::temp_dir().join("daedalus")
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // A multi-thread tokio runtime backs all async orchestration (backend I/O, capture).
    // It stays alive for the whole process; the GPUI loop runs on the main thread and
    // dispatches commands onto this runtime via its Handle.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let dir = data_dir();
    let app = match build_app(&dir) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("daedalus-desktop: failed to open data dir {dir:?}: {e}");
            std::process::exit(1);
        }
    };

    // Reconcile any non-terminal sessions from a prior run (FR-029/030).
    runtime.block_on(async {
        if let Err(e) = app.core().reconcile().await {
            tracing::warn!("reconcile failed: {e}");
        }
    });

    #[cfg(feature = "gpui")]
    {
        // Blocks the main thread for the lifetime of the window.
        daedalus_desktop::gpui_ui::run(app, runtime.handle().clone());
    }

    #[cfg(not(feature = "gpui"))]
    {
        runtime.block_on(headless_summary(&app));
    }
}

#[cfg(not(feature = "gpui"))]
async fn headless_summary(app: &App) {
    use daedalus_app::{AppQuery, AppQueryAsync};

    println!(
        "Daedalus (headless build) — default backend: {:?}",
        daedalus_desktop::default_backend_kind()
    );
    println!("Backends:");
    for b in app.backends().await {
        println!("  - {:?}: {:?}", b.kind, b.availability);
    }
    println!("Sessions in fleet: {}", app.fleet().len());
    println!();
    println!("The native GPUI UI requires a GPU/display — run with `--features gpui`.");
}
