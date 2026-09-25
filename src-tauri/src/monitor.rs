use crate::providers::{self, Device, ProviderStatus};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_notification::NotificationExt;

const HISTORY_AGE_MS: u64 = 24 * 60 * 60 * 1000;
const SAMPLE_INTERVAL_MS: u64 = 60 * 1000;
const NOTIFICATION_RETRY_MS: u64 = 60 * 1000;
const MAX_SAMPLES: usize = 1440;
const MAX_DEVICES: usize = 64;
const MAX_STORE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MonitorSettings {
    pub refresh_seconds: u64,
    pub low_battery_threshold: u8,
    pub notifications_enabled: bool,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            refresh_seconds: 5,
            low_battery_threshold: 20,
            notifications_enabled: false,
        }
    }
}

impl MonitorSettings {
    fn validate(&self) -> Result<(), String> {
        if !(5..=60).contains(&self.refresh_seconds) {
            return Err("Refresh interval must be between 5 and 60 seconds.".into());
        }
        if !(5..=50).contains(&self.low_battery_threshold) {
            return Err("Low battery threshold must be between 5% and 50%.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatterySample {
    pub timestamp: u64,
    pub battery: u8,
}

type History = BTreeMap<String, Vec<BatterySample>>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub devices: Vec<Device>,
    pub providers: Vec<ProviderStatus>,
    pub scanned_at: u64,
    pub history: History,
    pub settings: MonitorSettings,
    pub notification_error: Option<String>,
    pub storage_error: Option<String>,
    pub monitoring_since: u64,
    pub tray_available: bool,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedMonitor {
    #[serde(default)]
    settings: MonitorSettings,
    #[serde(default)]
    history: History,
    #[serde(default)]
    alerted_devices: HashSet<String>,
}

#[derive(Default)]
struct AlertState {
    delivered: HashSet<String>,
    last_attempt: HashMap<String, u64>,
}

struct BatteryAlert {
    id: String,
    name: String,
    battery: u8,
}

struct MonitorData {
    snapshot: MonitorSnapshot,
    alerts: AlertState,
}

impl MonitorData {
    fn saved(&self) -> SavedMonitor {
        SavedMonitor {
            settings: self.snapshot.settings.clone(),
            history: self.snapshot.history.clone(),
            alerted_devices: self.alerts.delivered.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Monitor {
    data: Arc<Mutex<MonitorData>>,
    // All scans and settings writes share this lock. The snapshot lock is never
    // held while a provider, notification daemon, or disk operation is running.
    scan_lock: Arc<Mutex<()>>,
    storage_path: Option<PathBuf>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn load_saved(path: &PathBuf) -> Result<SavedMonitor, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SavedMonitor::default())
        }
        Err(error) => return Err(format!("Could not read saved monitor data: {error}")),
    };
    if metadata.len() > MAX_STORE_BYTES {
        return Err("Saved monitor data exceeds the 10 MB size limit.".into());
    }
    let contents = fs::read(path).map_err(|error| format!("Could not read history: {error}"))?;
    let saved: SavedMonitor = serde_json::from_slice(&contents)
        .map_err(|error| format!("Saved monitor data is invalid: {error}"))?;
    saved.settings.validate()?;
    Ok(saved)
}

fn prune_history(history: &mut History, now: u64) {
    let cutoff = now.saturating_sub(HISTORY_AGE_MS);
    history.retain(|_, samples| {
        samples.retain(|sample| {
            sample.timestamp >= cutoff && sample.timestamp <= now && sample.battery <= 100
        });
        samples.sort_unstable_by_key(|sample| sample.timestamp);
        samples.dedup_by_key(|sample| sample.timestamp);
        if samples.len() > MAX_SAMPLES {
            samples.drain(..samples.len() - MAX_SAMPLES);
        }
        !samples.is_empty()
    });
    if history.len() > MAX_DEVICES {
        let mut recency: Vec<_> = history
            .iter()
            .map(|(id, samples)| (samples.last().map_or(0, |s| s.timestamp), id.clone()))
            .collect();
        recency.sort();
        for (_, id) in recency.into_iter().take(history.len() - MAX_DEVICES) {
            history.remove(&id);
        }
    }
}

fn record_history(history: &mut History, devices: &[Device], now: u64) {
    for device in devices {
        if device.connected == Some(false) {
            continue;
        }
        let Some(battery) = device.battery.filter(|battery| *battery <= 100) else {
            continue;
        };
        let samples = history.entry(device.id.clone()).or_default();
        if samples.last().is_none_or(|previous| {
            previous.battery != battery
                || now.saturating_sub(previous.timestamp) >= SAMPLE_INTERVAL_MS
        }) {
            samples.push(BatterySample {
                timestamp: now,
                battery,
            });
        }
    }
    prune_history(history, now);
}

fn low_battery_alerts(
    devices: &[Device],
    settings: &MonitorSettings,
    state: &mut AlertState,
    now: u64,
) -> Vec<BatteryAlert> {
    let mut alerts = Vec::new();
    for device in devices {
        if device.connected == Some(false) {
            continue;
        }
        let Some(battery) = device.battery.filter(|battery| *battery <= 100) else {
            continue;
        };
        if battery >= settings.low_battery_threshold {
            state.delivered.remove(&device.id);
            state.last_attempt.remove(&device.id);
        } else if settings.notifications_enabled
            && device.charging != Some(true)
            && !state.delivered.contains(&device.id)
            && state
                .last_attempt
                .get(&device.id)
                .is_none_or(|last| now.saturating_sub(*last) >= NOTIFICATION_RETRY_MS)
        {
            state.last_attempt.insert(device.id.clone(), now);
            alerts.push(BatteryAlert {
                id: device.id.clone(),
                name: device.name.clone(),
                battery,
            });
        }
    }
    // Unknown or missing readings establish no recovery. Only a successful
    // notification latches; failures retry with a cooldown instead of spamming.
    alerts
}

fn notification_delivered(state: &mut AlertState, id: String) {
    state.last_attempt.remove(&id);
    state.delivered.insert(id);
}

fn send_notification(app: &AppHandle, alert: &BatteryAlert) -> Result<(), String> {
    let title = "Pin thiết bị yếu";
    let body = format!("{} còn {}% pin.", alert.name, alert.battery);
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        // Tauri's desktop wrapper drops the asynchronous daemon result. Use
        // the same native transport directly here so failures can be retried.
        notify_rust::Notification::new()
            .summary(title)
            .body(&body)
            .appname("Device Battery")
            .show()
            .map(|_| ())
            .map_err(|error| format!("Không thể gửi thông báo pin: {error}"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|error| format!("Không thể gửi thông báo pin: {error}"))
    }
}

impl Monitor {
    pub fn new(app: &AppHandle) -> Self {
        let path = app
            .path()
            .app_data_dir()
            .map(|path| path.join("monitor.json"));
        let (storage_path, mut saved, storage_error) = match path {
            Ok(path) => match load_saved(&path) {
                Ok(saved) => (Some(path), saved, None),
                Err(error) => (Some(path), SavedMonitor::default(), Some(error)),
            },
            Err(error) => (
                None,
                SavedMonitor::default(),
                Some(format!("App data directory is unavailable: {error}")),
            ),
        };
        let started = now_ms();
        prune_history(&mut saved.history, started);
        Self {
            data: Arc::new(Mutex::new(MonitorData {
                snapshot: MonitorSnapshot {
                    devices: Vec::new(),
                    providers: Vec::new(),
                    scanned_at: 0,
                    history: saved.history,
                    settings: saved.settings,
                    notification_error: None,
                    storage_error,
                    monitoring_since: started,
                    tray_available: false,
                },
                alerts: AlertState {
                    delivered: saved.alerted_devices,
                    last_attempt: HashMap::new(),
                },
            })),
            scan_lock: Arc::new(Mutex::new(())),
            storage_path,
        }
    }

    pub fn snapshot(&self) -> Result<MonitorSnapshot, String> {
        self.data
            .lock()
            .map(|data| data.snapshot.clone())
            .map_err(|_| "Monitor state is unavailable.".into())
    }

    pub fn set_tray_available(&self, available: bool) {
        if let Ok(mut data) = self.data.lock() {
            data.snapshot.tray_available = available;
        }
    }

    fn save(&self, saved: &SavedMonitor) -> Result<(), String> {
        let Some(path) = &self.storage_path else {
            return Err(
                "App data directory is unavailable; settings and history are temporary.".into(),
            );
        };
        let directory = path.parent().ok_or("Invalid app data directory.")?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create app data directory: {error}"))?;
        let contents = serde_json::to_vec(saved).map_err(|error| error.to_string())?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, contents)
            .map_err(|error| format!("Could not save settings and history: {error}"))?;
        // rename replaces the previous file atomically on the desktop platforms.
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not replace saved settings and history: {error}"))
    }

    fn persist_and_emit(
        &self,
        app: &AppHandle,
        saved: Option<SavedMonitor>,
    ) -> Result<MonitorSnapshot, String> {
        if let Some(saved) = saved {
            let error = self.save(&saved).err();
            let mut data = self
                .data
                .lock()
                .map_err(|_| "Monitor state is unavailable.")?;
            data.snapshot.storage_error = error;
        }
        let snapshot = self.snapshot()?;
        let _ = app.emit("monitor-updated", &snapshot);
        Ok(snapshot)
    }

    pub fn refresh(&self, app: &AppHandle) -> Result<MonitorSnapshot, String> {
        let _scan = self
            .scan_lock
            .lock()
            .map_err(|_| "Device scanner is unavailable.")?;
        let result = providers::scan_devices();
        let now = now_ms();
        let (alerts, history_changed, previous_alerted) = {
            let mut data = self
                .data
                .lock()
                .map_err(|_| "Monitor state is unavailable.")?;
            let previous_history = data.snapshot.history.clone();
            let previous_alerted = data.alerts.delivered.clone();
            record_history(&mut data.snapshot.history, &result.devices, now);
            let settings = data.snapshot.settings.clone();
            let alerts = low_battery_alerts(&result.devices, &settings, &mut data.alerts, now);
            data.snapshot.devices = result.devices;
            data.snapshot.providers = result.providers;
            data.snapshot.scanned_at = now;
            (
                alerts,
                previous_history != data.snapshot.history,
                previous_alerted,
            )
        };
        if !alerts.is_empty() {
            let mut notification_error = None;
            let mut delivered = Vec::new();
            for alert in alerts {
                match send_notification(app, &alert) {
                    Ok(()) => delivered.push(alert.id),
                    Err(error) => notification_error = Some(error),
                }
            }
            let mut data = self
                .data
                .lock()
                .map_err(|_| "Monitor state is unavailable.")?;
            for id in delivered {
                notification_delivered(&mut data.alerts, id);
            }
            data.snapshot.notification_error = notification_error;
        }
        let saved = {
            let data = self
                .data
                .lock()
                .map_err(|_| "Monitor state is unavailable.")?;
            (history_changed || previous_alerted != data.alerts.delivered).then(|| data.saved())
        };
        let snapshot = self.persist_and_emit(app, saved)?;

        if let Some(tray) = app.tray_by_id("device-battery") {
            let mut tooltip = String::new();
            let mut count = 0;
            
            for device in &snapshot.devices {
                if device.connected != Some(false) {
                    let bat = device.battery.map_or("?".to_string(), |b| format!("{}%", b));
                    let charge = if device.charging == Some(true) { " ⚡" } else { "" };
                    let text = format!("{}: {}{}", device.name, bat, charge);
                    
                    tooltip.push_str(&text);
                    tooltip.push_str("\n");
                    
                    count += 1;
                }
            }
            if count == 0 {
                tooltip.push_str("Device Battery\nChưa có thiết bị nào");
            }
            let _ = tray.set_tooltip(Some(tooltip.trim_end()));
        }

        Ok(snapshot)
    }

    pub fn update_settings(
        &self,
        app: &AppHandle,
        settings: MonitorSettings,
    ) -> Result<MonitorSnapshot, String> {
        settings.validate()?;
        let _scan = self
            .scan_lock
            .lock()
            .map_err(|_| "Device scanner is unavailable.")?;
        let saved = {
            let mut data = self
                .data
                .lock()
                .map_err(|_| "Monitor state is unavailable.")?;
            if data.snapshot.settings.notifications_enabled != settings.notifications_enabled {
                data.snapshot.notification_error = None;
            }
            data.snapshot.settings = settings;
            data.saved()
        };
        self.persist_and_emit(app, Some(saved))
    }

    pub fn start_polling(&self, app: AppHandle) {
        let monitor = self.clone();
        std::thread::spawn(move || loop {
            if let Err(error) = monitor.refresh(&app) {
                eprintln!("Device Battery: {error}");
            }
            // Read settings each second so changing a 60-second interval to a
            // shorter one takes effect without waiting for the old interval.
            let mut waited = 0;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                waited += 1;
                let interval = monitor
                    .data
                    .lock()
                    .map_or(5, |data| data.snapshot.settings.refresh_seconds);
                if waited >= interval {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(battery: Option<u8>) -> Device {
        Device {
            id: "test-mouse".into(),
            name: "Test mouse".into(),
            kind: "mouse".into(),
            connection: "bluetooth".into(),
            battery,
            charging: Some(false),
            connected: Some(true),
            source: "test".into(),
            battery_note: None,
        }
    }

    fn settings() -> MonitorSettings {
        MonitorSettings {
            notifications_enabled: true,
            ..MonitorSettings::default()
        }
    }

    fn alerts(battery: Option<u8>, state: &mut AlertState, now: u64) -> Vec<BatteryAlert> {
        low_battery_alerts(&[device(battery)], &settings(), state, now)
    }

    #[test]
    fn alerts_are_strictly_below_threshold_and_only_once_until_recovery() {
        let mut state = AlertState::default();
        assert!(alerts(Some(20), &mut state, 0).is_empty());
        assert_eq!(alerts(Some(19), &mut state, 0).len(), 1);
        notification_delivered(&mut state, "test-mouse".into());
        assert!(alerts(Some(0), &mut state, NOTIFICATION_RETRY_MS).is_empty());
        assert!(alerts(Some(20), &mut state, NOTIFICATION_RETRY_MS).is_empty());
        assert_eq!(alerts(Some(0), &mut state, NOTIFICATION_RETRY_MS).len(), 1);
    }

    #[test]
    fn failed_notifications_retry_after_cooldown_and_latch_only_on_success() {
        let mut state = AlertState::default();
        assert_eq!(alerts(Some(10), &mut state, 0).len(), 1);
        assert!(state.delivered.is_empty());
        assert!(alerts(Some(9), &mut state, NOTIFICATION_RETRY_MS - 1).is_empty());
        assert_eq!(alerts(Some(8), &mut state, NOTIFICATION_RETRY_MS).len(), 1);
        notification_delivered(&mut state, "test-mouse".into());
        assert!(alerts(Some(7), &mut state, NOTIFICATION_RETRY_MS * 2).is_empty());
    }

    #[test]
    fn missing_and_unknown_readings_do_not_rearm_alerts() {
        let mut state = AlertState::default();
        alerts(Some(10), &mut state, 0);
        notification_delivered(&mut state, "test-mouse".into());
        low_battery_alerts(&[], &settings(), &mut state, NOTIFICATION_RETRY_MS);
        alerts(None, &mut state, NOTIFICATION_RETRY_MS);
        alerts(Some(101), &mut state, NOTIFICATION_RETRY_MS);
        let mut disconnected = device(Some(80));
        disconnected.connected = Some(false);
        low_battery_alerts(
            &[disconnected],
            &settings(),
            &mut state,
            NOTIFICATION_RETRY_MS,
        );
        assert!(alerts(Some(9), &mut state, NOTIFICATION_RETRY_MS).is_empty());
    }

    #[test]
    fn charging_disconnected_and_disabled_alerts_are_suppressed_without_latching() {
        let mut state = AlertState::default();
        let mut charging = device(Some(10));
        charging.charging = Some(true);
        let mut disconnected = device(Some(10));
        disconnected.connected = Some(false);
        assert!(
            low_battery_alerts(&[charging, disconnected], &settings(), &mut state, 0).is_empty()
        );
        assert!(low_battery_alerts(
            &[device(Some(10))],
            &MonitorSettings::default(),
            &mut state,
            0
        )
        .is_empty());
        assert!(state.delivered.is_empty());
        assert_eq!(alerts(Some(10), &mut state, 0).len(), 1);
    }

    #[test]
    fn history_records_changes_and_minute_samples_but_ignores_unknown_and_disconnected() {
        let mut history = History::new();
        record_history(&mut history, &[device(Some(80))], 100_000);
        record_history(&mut history, &[device(Some(80))], 105_000);
        record_history(&mut history, &[device(None)], 110_000);
        record_history(&mut history, &[device(Some(79))], 115_000);
        record_history(&mut history, &[device(Some(79))], 175_000);
        let mut disconnected = device(Some(78));
        disconnected.connected = Some(false);
        record_history(&mut history, &[disconnected], 180_000);
        assert_eq!(
            history["test-mouse"]
                .iter()
                .map(|s| s.battery)
                .collect::<Vec<_>>(),
            vec![80, 79, 79]
        );
    }

    #[test]
    fn history_retention_rejects_old_future_and_invalid_samples_and_limits_device_count() {
        let now = HISTORY_AGE_MS + 1000;
        let mut history = History::new();
        history.insert(
            "invalid".into(),
            vec![
                BatterySample {
                    timestamp: 999,
                    battery: 50,
                },
                BatterySample {
                    timestamp: now + 1,
                    battery: 50,
                },
                BatterySample {
                    timestamp: now,
                    battery: 101,
                },
            ],
        );
        for id in 0..MAX_DEVICES + 2 {
            history.insert(
                format!("device-{id}"),
                vec![BatterySample {
                    timestamp: now - id as u64,
                    battery: 50,
                }],
            );
        }
        prune_history(&mut history, now);
        assert_eq!(history.len(), MAX_DEVICES);
        assert!(!history.contains_key("invalid"));
        assert!(!history.contains_key("device-65"));
        assert!(history.contains_key("device-0"));
    }

    #[test]
    fn history_retains_only_latest_samples_and_sorts_loaded_data() {
        let mut history = History::new();
        history.insert(
            "mouse".into(),
            (0..MAX_SAMPLES + 10)
                .rev()
                .map(|timestamp| BatterySample {
                    timestamp: timestamp as u64,
                    battery: 50,
                })
                .collect(),
        );
        prune_history(&mut history, 10_000);
        assert_eq!(history["mouse"].len(), MAX_SAMPLES);
        assert_eq!(history["mouse"][0].timestamp, 10);
    }

    #[test]
    fn settings_validate_safe_ranges_and_default_notifications_off() {
        assert!(!MonitorSettings::default().notifications_enabled);
        for interval in [0, 4, 61, u64::MAX] {
            assert!(MonitorSettings {
                refresh_seconds: interval,
                ..settings()
            }
            .validate()
            .is_err());
        }
        for threshold in [0, 4, 51, 255] {
            assert!(MonitorSettings {
                low_battery_threshold: threshold,
                ..settings()
            }
            .validate()
            .is_err());
        }
        for interval in [5, 60] {
            for threshold in [5, 50] {
                assert!(MonitorSettings {
                    refresh_seconds: interval,
                    low_battery_threshold: threshold,
                    notifications_enabled: true
                }
                .validate()
                .is_ok());
            }
        }
    }
}
