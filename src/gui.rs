#![cfg_attr(all(not(test), target_os = "windows"), windows_subsystem = "windows")]

use epub2cbz::settings::Settings;
use dioxus_native::prelude::*;
use std::path::PathBuf;

pub fn run_gui() {
    dioxus_native::launch(App);
}

#[derive(Clone)]
struct LogEntry {
    text: String,
    is_error: bool,
}

#[derive(Clone, PartialEq)]
enum FileStatus {
    Pending,
    Processing,
    Done,
    Failed(String),
}

#[derive(Clone)]
struct FileItem {
    path: PathBuf,
    name: String,
    status: FileStatus,
}

#[component]
fn App() -> Element {
    let settings = use_signal(Settings::load);
    let mut files = use_signal(Vec::<FileItem>::new);
    let mut output_dir = use_signal(|| Option::<PathBuf>::None);
    let mut log_entries = use_signal(Vec::<LogEntry>::new);
    let mut is_processing = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let mut done_count = use_signal(|| 0usize);
    let mut total_count = use_signal(|| 0usize);

    let mut add_epub_paths = move |paths: Vec<PathBuf>| {
        let mut current = files.write();
        for p in paths {
            let items = epub2cbz::find_epub_files(&p, true);
            for item in items {
                let name = item
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if !current.iter().any(|f| f.path == item) {
                    current.push(FileItem {
                        path: item,
                        name,
                        status: FileStatus::Pending,
                    });
                }
            }
        }
    };

    let add_files = move |_| {
        spawn(async move {
            if let Some(picked) = rfd::AsyncFileDialog::new()
                .add_filter("EPUB files", &["epub"])
                .set_title("Select EPUB files")
                .pick_files()
                .await
            {
                let paths: Vec<PathBuf> = picked.iter().map(|f| f.path().to_path_buf()).collect();
                add_epub_paths(paths);
            }
        });
    };

    let add_folder = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new()
                .set_title("Select folder with EPUB files")
                .pick_folder()
                .await
            {
                add_epub_paths(vec![folder.path().to_path_buf()]);
            }
        });
    };

    let select_output = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new()
                .set_title("Select output directory")
                .pick_folder()
                .await
            {
                output_dir.set(Some(folder.path().to_path_buf()));
            }
        });
    };

    let clear_files = move |_| {
        files.set(Vec::new());
        log_entries.set(Vec::new());
        done_count.set(0);
        total_count.set(0);
    };

    let convert = move |_| {
        let file_list: Vec<(usize, PathBuf)> = files
            .read()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.status == FileStatus::Pending || matches!(f.status, FileStatus::Failed(_)))
            .map(|(i, f)| (i, f.path.clone()))
            .collect();
        let out = output_dir.read().clone();
        let sets = settings.read().clone();

        if file_list.is_empty() {
            return;
        }

        is_processing.set(true);
        let count = file_list.len();
        total_count.set(count);
        done_count.set(0);

        // Mark all as pending
        {
            let mut f = files.write();
            for (idx, _) in &file_list {
                f[*idx].status = FileStatus::Pending;
            }
        }

        spawn(async move {
            for (idx, epub_path) in &file_list {
                {
                    let mut f = files.write();
                    f[*idx].status = FileStatus::Processing;
                }

                let output = out
                    .clone()
                    .unwrap_or_else(|| {
                        epub_path
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf()
                    });

                let log_fn = |_msg: &str| {};

                let result = epub2cbz::process_epub(epub_path, &output, &sets, &log_fn);

                {
                    let mut f = files.write();
                    match result {
                        Ok(ref r) if r.success => {
                            f[*idx].status = FileStatus::Done;
                            log_entries.write().push(LogEntry {
                                text: format!("{}: {}", f[*idx].name, r.message),
                                is_error: false,
                            });
                        }
                        Ok(ref r) => {
                            f[*idx].status = FileStatus::Failed(r.message.clone());
                            log_entries.write().push(LogEntry {
                                text: format!("{}: {}", f[*idx].name, r.message),
                                is_error: true,
                            });
                        }
                        Err(ref e) => {
                            f[*idx].status = FileStatus::Failed(format!("{e}"));
                            log_entries.write().push(LogEntry {
                                text: format!("{}: {e}", f[*idx].name),
                                is_error: true,
                            });
                        }
                    }
                }

                let prev = *done_count.read();
                done_count.set(prev + 1);
            }
            is_processing.set(false);
        });
    };

    let done = *done_count.read();
    let total = *total_count.read();
    let pct = if total > 0 {
        (done as f64 / total as f64 * 100.0) as u32
    } else {
        0
    };

    rsx! {
        style { {include_str!("gui_style.css")} }

        div {
            class: "app",

            header { class: "toolbar",
                h1 { "epub2cbz" }
                div { class: "toolbar-buttons",
                    button {
                        onclick: add_files,
                        disabled: *is_processing.read(),
                        "Add Files"
                    }
                    button {
                        onclick: add_folder,
                        disabled: *is_processing.read(),
                        "Add Folder"
                    }
                    button {
                        onclick: select_output,
                        disabled: *is_processing.read(),
                        "Output"
                    }
                    button {
                        onclick: move |_| {
                            let current = *show_settings.read();
                            show_settings.set(!current);
                        },
                        "Settings"
                    }
                    button {
                        onclick: clear_files,
                        disabled: *is_processing.read(),
                        "Clear"
                    }
                }
            }

            if let Some(ref dir) = *output_dir.read() {
                div { class: "info-bar",
                    "Output: {dir.display()}"
                }
            }

            if *show_settings.read() {
                SettingsPanel { settings }
            }

            div { class: "file-list",
                if files.read().is_empty() {
                    div { class: "drop-hint",
                        "Use Add Files or Add Folder to select .epub files"
                    }
                }
                for (i, f) in files.read().iter().enumerate() {
                    div {
                        class: match f.status {
                            FileStatus::Pending => "file-item",
                            FileStatus::Processing => "file-item file-processing",
                            FileStatus::Done => "file-item file-done",
                            FileStatus::Failed(_) => "file-item file-failed",
                        },
                        key: "{i}",
                        span { class: "file-name", "{f.name}" }
                        span { class: "file-status",
                            match &f.status {
                                FileStatus::Pending => "pending".to_string(),
                                FileStatus::Processing => "converting...".to_string(),
                                FileStatus::Done => "done".to_string(),
                                FileStatus::Failed(msg) => msg.clone(),
                            }
                        }
                    }
                }
            }

            if total > 0 {
                div { class: "progress-bar",
                    div {
                        class: "progress-fill",
                        width: "{pct}%",
                    }
                    span { class: "progress-text", "{done}/{total}" }
                }
            }

            div { class: "convert-bar",
                button {
                    class: "convert-button",
                    onclick: convert,
                    disabled: *is_processing.read() || files.read().is_empty(),
                    if *is_processing.read() { "Converting..." } else { "Convert" }
                }
            }

            div { class: "log-output",
                for (i, entry) in log_entries.read().iter().enumerate() {
                    div {
                        key: "{i}",
                        class: if entry.is_error { "log-error" } else { "log-info" },
                        "{entry.text}"
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsPanel(settings: Signal<Settings>) -> Element {
    let mut s = settings;

    rsx! {
        div { class: "settings-panel",
            h3 { "Settings" }

            div { class: "settings-grid",
                SettingToggle { label: "Split double pages".to_string(), value: s.read().split_page_spread,
                    onchange: move |v| s.write().split_page_spread = v }
                SettingToggle { label: "Crop images".to_string(), value: s.read().crop_images,
                    onchange: move |v| s.write().crop_images = v }
                SettingToggle { label: "Resize images".to_string(), value: s.read().resize_images,
                    onchange: move |v| s.write().resize_images = v }
                SettingToggle { label: "Page spread".to_string(), value: s.read().page_spread,
                    onchange: move |v| s.write().page_spread = v }
                SettingToggle { label: "ComicInfo.xml".to_string(), value: s.read().comic_info,
                    onchange: move |v| s.write().comic_info = v }
                SettingToggle { label: "Remove duplicate cover".to_string(), value: s.read().duplicate_cover,
                    onchange: move |v| s.write().duplicate_cover = v }
                SettingToggle { label: "Simple extraction".to_string(), value: s.read().simple_extraction,
                    onchange: move |v| s.write().simple_extraction = v }
                SettingToggle { label: "DRM check".to_string(), value: s.read().drm_protection,
                    onchange: move |v| s.write().drm_protection = v }
            }

            div { class: "settings-actions",
                button {
                    onclick: move |_| {
                        let current = s.read().clone();
                        let _ = current.save();
                    },
                    "Save Settings"
                }
            }
        }
    }
}

#[component]
fn SettingToggle(label: String, value: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "setting-toggle",
            input {
                r#type: "checkbox",
                checked: value,
                onchange: move |evt: Event<FormData>| {
                    let val = evt.data().value() == "true";
                    onchange.call(val);
                },
            }
            span { "{label}" }
        }
    }
}
