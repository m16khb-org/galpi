use crate::adapters::inbound::tauri::TauriEvents;
use crate::adapters::outbound::chatgpt::{ChatGptOAuthAdapter, SystemClock, TauriBrowser};
use crate::adapters::outbound::desktop::DesktopAdapter;
use crate::adapters::outbound::recording::NativeRecorder;
#[cfg(windows)]
use crate::adapters::outbound::secrets::CredentialManager;
use crate::adapters::outbound::secrets::SecretStore;
#[cfg(not(windows))]
use crate::adapters::outbound::secrets::SettingsFile;
use crate::adapters::outbound::settings::LocalSettingsStore;
use crate::application::chatgpt::ChatGptAccounts;
use crate::application::ports::{
    ArtifactPort, ChatGptAuthPort, ChatGptEvents, ChatGptStore, ClockPort, EnginePort, JobEvents,
    RecordingEvents, RecordingPort, RefinementPort, SettingsPort, TranscriptImportPort,
    TranscriptionPort,
};
use crate::application::use_cases::Application;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let events = Arc::new(TauriEvents::new(app.handle().clone()));
            let job_events: Arc<dyn JobEvents> = events.clone();
            let chatgpt_events: Arc<dyn ChatGptEvents> = events.clone();
            let recording_events: Arc<dyn RecordingEvents> = events;
            let desktop = Arc::new(DesktopAdapter::new(app.handle().clone(), job_events));
            let engine: Arc<dyn EnginePort> = desktop.clone();
            let transcription: Arc<dyn TranscriptionPort> = desktop.clone();
            let imports: Arc<dyn TranscriptImportPort> = desktop.clone();
            let refinement: Arc<dyn RefinementPort> = desktop.clone();
            let artifacts: Arc<dyn ArtifactPort> = desktop;
            let recording: Arc<dyn RecordingPort> = Arc::new(NativeRecorder::new(recording_events));
            // Windows has an OS credential store; macOS keeps secrets in the
            // settings file until the app ships a stable signature.
            #[cfg(windows)]
            let secrets: Arc<dyn SecretStore> = Arc::new(CredentialManager::default());
            #[cfg(not(windows))]
            let secrets: Arc<dyn SecretStore> = Arc::new(SettingsFile);
            // One store behind both ports: two objects over the same
            // settings.json would race each other's read-modify-write.
            let local_settings = Arc::new(LocalSettingsStore::new(app.handle(), secrets)?);
            let settings: Arc<dyn SettingsPort> = local_settings.clone();
            let chatgpt_store: Arc<dyn ChatGptStore> = local_settings;
            let clock: Arc<dyn ClockPort> = Arc::new(SystemClock);
            let chatgpt_auth: Arc<dyn ChatGptAuthPort> = Arc::new(ChatGptOAuthAdapter::new(
                Arc::new(TauriBrowser::new(app.handle().clone())),
                chatgpt_events,
                clock.clone(),
            )?);
            app.manage(Application::new(
                engine,
                transcription,
                imports,
                artifacts,
                recording,
                settings,
                refinement,
                ChatGptAccounts::new(chatgpt_auth, chatgpt_store, clock),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::adapters::inbound::tauri::diagnose_environment,
            crate::adapters::inbound::tauri::prepare_environment,
            crate::adapters::inbound::tauri::hugging_face_token_stored,
            crate::adapters::inbound::tauri::save_hugging_face_token,
            crate::adapters::inbound::tauri::load_assistant_settings,
            crate::adapters::inbound::tauri::save_assistant_api_key,
            crate::adapters::inbound::tauri::save_assistant_settings,
            crate::adapters::inbound::tauri::save_engine_preset,
            crate::adapters::inbound::tauri::save_compute_device,
            crate::adapters::inbound::tauri::refine_transcript,
            crate::adapters::inbound::tauri::start_transcription,
            crate::adapters::inbound::tauri::import_transcript,
            crate::adapters::inbound::tauri::cancel_job,
            crate::adapters::inbound::tauri::open_artifact,
            crate::adapters::inbound::tauri::reveal_output_directory,
            crate::adapters::inbound::tauri::start_recording,
            crate::adapters::inbound::tauri::stop_recording,
            crate::adapters::inbound::tauri::cancel_recording,
            crate::adapters::inbound::tauri::load_chatgpt_settings,
            crate::adapters::inbound::tauri::save_chatgpt_preferences,
            crate::adapters::inbound::tauri::sign_in_with_chatgpt,
            crate::adapters::inbound::tauri::cancel_chatgpt_sign_in,
            crate::adapters::inbound::tauri::list_chatgpt_models,
            crate::adapters::inbound::tauri::sign_out_of_chatgpt,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            eprintln!("failed to run Galpi: {error}");
            std::process::exit(1);
        });
}
