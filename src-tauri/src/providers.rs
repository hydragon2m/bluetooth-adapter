//! Read-only OS telemetry. A USB HID endpoint proves neither a radio link nor a
//! battery protocol. Unknown values must stay unknown; no vendor reports are sent.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub connection: String,
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    pub connected: Option<bool>,
    pub source: String,
    pub battery_note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub devices: Vec<Device>,
    pub providers: Vec<ProviderStatus>,
}

pub fn scan_devices() -> ScanResult {
    #[cfg(target_os = "linux")]
    return linux::scan();

    #[cfg(not(target_os = "linux"))]
    ScanResult {
        devices: Vec::new(),
        providers: vec![ProviderStatus {
            id: "platform".into(),
            name: "Native device provider".into(),
            available: false,
            message: "Bản này hiện chỉ đọc thiết bị trên Linux. Provider cho hệ điều hành này chưa được triển khai.".into(),
        }],
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Device, ProviderStatus, ScanResult};
    use serde_json::Value;
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        io::Read,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    const BLUEZ_OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
    const UNKNOWN_BATTERY: &str = "Kernel không cung cấp phần trăm pin cho thiết bị này. Có thể cần provider theo giao thức của hãng.";

    struct HidDevice {
        path: PathBuf,
        device: Device,
    }

    pub(super) fn scan() -> ScanResult {
        let (hid, hid_status) = read_hid(Path::new("/sys/bus/hid/devices"));
        let mut devices = BTreeMap::new();
        for item in &hid {
            merge_device(&mut devices, item.device.clone());
        }
        let power_status =
            read_power_supplies(Path::new("/sys/class/power_supply"), &hid, &mut devices);
        let bluez_status = match read_bluez().and_then(|json| parse_bluez(&json)) {
            Ok(bluetooth) => {
                let count = bluetooth.len();
                for device in bluetooth {
                    merge_device(&mut devices, device);
                }
                status("bluez", "BlueZ Bluetooth", true, format!("Đã đọc {count} thiết bị Bluetooth đã ghép đôi hoặc đang kết nối. Pin cần giao diện BlueZ Battery1; giao diện này không cung cấp trạng thái sạc."))
            }
            Err(message) => status("bluez", "BlueZ Bluetooth", false, message),
        };
        let mut devices: Vec<_> = devices.into_values().collect();
        devices.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        ScanResult {
            devices,
            providers: vec![hid_status, power_status, bluez_status],
        }
    }

    fn status(id: &str, name: &str, available: bool, message: String) -> ProviderStatus {
        ProviderStatus {
            id: id.into(),
            name: name.into(),
            available,
            message,
        }
    }

    fn read_attribute(path: &Path, name: &str) -> Option<String> {
        fs::read_to_string(path.join(name))
            .ok()
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    }

    fn parse_uevent(text: &str) -> HashMap<&str, &str> {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key, value.trim()))
            .collect()
    }

    fn classify(name: &str) -> String {
        let name = name.to_lowercase();
        if name.contains("mouse") || name.contains("trackball") {
            "mouse"
        } else if name.contains("keyboard") {
            "keyboard"
        } else if ["headset", "headphone", "earbud"]
            .iter()
            .any(|word| name.contains(word))
        {
            "headset"
        } else if [
            "gamepad",
            "joystick",
            "game controller",
            "xbox",
            "dualsense",
            "dualshock",
        ]
        .iter()
        .any(|word| name.contains(word))
        {
            "controller"
        } else {
            "unknown"
        }
        .into()
    }

    fn bluetooth_address(text: &str) -> Option<String> {
        let normalized = text.trim().replace('-', ":").to_lowercase();
        let parts: Vec<_> = normalized.split(':').collect();
        (parts.len() == 6
            && parts
                .iter()
                .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit())))
        .then_some(normalized)
    }

    fn hid_device(text: &str, path: &Path) -> Option<Device> {
        let attributes = parse_uevent(text);
        let hid_id = *attributes.get("HID_ID")?;
        let bus = hid_id.split(':').next()?;
        // USB (0x03) and Bluetooth (0x05) only; do not show ACPI/I2C laptop input hardware.
        if bus != "0003" && bus != "0005" {
            return None;
        }
        let name = attributes
            .get("HID_NAME")
            .copied()
            .filter(|name| !name.is_empty())
            .unwrap_or("Unknown HID device")
            .to_owned();
        let unique = attributes.get("HID_UNIQ").copied().unwrap_or("");
        let physical = attributes.get("HID_PHYS").copied().unwrap_or("");
        let address = if bus == "0005" {
            bluetooth_address(unique)
        } else {
            None
        };
        // Interface numbers and the kernel's .000N HID instance counter change on
        // re-enumeration. Prefer serial/MAC; otherwise identify a physical USB port.
        let physical = physical
            .rsplit_once("/input")
            .map(|(base, _)| base)
            .unwrap_or(physical);
        let parent = path.parent().unwrap_or(path);
        let fallback = if bus == "0003"
            && parent
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(':'))
        {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        }
        .to_string_lossy();
        let identity = if !unique.is_empty() {
            unique
        } else if !physical.is_empty() {
            physical
        } else {
            &fallback
        };
        let id = address
            .map(|address| format!("bluetooth:{address}"))
            .unwrap_or_else(|| format!("hid:{}:{identity}", hid_id.to_lowercase()));
        Some(Device {
            id,
            kind: classify(&name),
            name,
            connection: if bus == "0005" { "bluetooth" } else { "usb" }.into(),
            battery: None,
            charging: None,
            connected: None,
            source: "Linux HID".into(),
            battery_note: Some(if bus == "0003" {
                "Đã phát hiện đầu USB. Receiver được cắm không xác nhận thiết bị không dây đang kết nối. Chưa có phần trăm pin từ hệ điều hành."
            } else { UNKNOWN_BATTERY }.into()),
        })
    }

    fn read_hid(root: &Path) -> (Vec<HidDevice>, ProviderStatus) {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                return (
                    Vec::new(),
                    status(
                        "linux-hid",
                        "Linux HID discovery",
                        false,
                        format!("Không đọc được thiết bị HID: {error}"),
                    ),
                )
            }
        };
        let mut hid = Vec::new();
        let mut unreadable = 0;
        for entry in entries {
            let Ok(entry) = entry else {
                unreadable += 1;
                continue;
            };
            let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            let Ok(text) = fs::read_to_string(path.join("uevent")) else {
                unreadable += 1;
                continue;
            };
            if let Some(device) = hid_device(&text, &path) {
                hid.push(HidDevice { path, device });
            }
        }
        let message = if unreadable > 0 {
            format!("Chỉ đọc được một phần HID: {unreadable} mục không đọc được. USB là kết nối với máy; giao thức không dây và trạng thái thiết bị có thể chưa xác định.")
        } else {
            "Khám phá HID chỉ đọc. USB là kết nối với máy; giao thức không dây và trạng thái thiết bị có thể chưa xác định.".into()
        };
        (
            hid,
            status("linux-hid", "Linux HID discovery", true, message),
        )
    }

    fn battery_percentage(value: &str) -> Option<u8> {
        value
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|value| *value <= 100)
    }

    fn charging_status(value: &str) -> Option<bool> {
        match value {
            "Charging" => Some(true),
            "Discharging" | "Not charging" | "Full" => Some(false),
            _ => None,
        }
    }

    fn peripheral_supply(
        supply_type: Option<&str>,
        scope: Option<&str>,
        matched_hid: bool,
    ) -> bool {
        supply_type == Some("Battery")
            && scope != Some("System")
            && (scope == Some("Device") || matched_hid)
    }

    fn read_power_supplies(
        root: &Path,
        hid: &[HidDevice],
        devices: &mut BTreeMap<String, Device>,
    ) -> ProviderStatus {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                return status(
                    "linux-power",
                    "Linux peripheral batteries",
                    false,
                    format!("Không đọc được thông tin nguồn pin: {error}"),
                )
            }
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let parent = fs::canonicalize(path.join("device")).ok();
            let matched = hid.iter().find(|item| {
                canonical.starts_with(&item.path)
                    || parent
                        .as_ref()
                        .is_some_and(|parent| parent.starts_with(&item.path))
            });
            let supply_type = read_attribute(&path, "type");
            let scope = read_attribute(&path, "scope");
            if !peripheral_supply(supply_type.as_deref(), scope.as_deref(), matched.is_some()) {
                continue;
            }
            count += 1;
            let name = read_attribute(&path, "model_name")
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
            let serial = read_attribute(&path, "serial_number");
            let id = serial
                .as_deref()
                .and_then(bluetooth_address)
                .map(|address| format!("bluetooth:{address}"))
                .unwrap_or_else(|| format!("power-supply:{}", entry.file_name().to_string_lossy()));
            let mut device = matched.map(|item| item.device.clone()).unwrap_or(Device {
                id,
                kind: classify(&name),
                name,
                connection: "unknown".into(),
                battery: None,
                charging: None,
                connected: None,
                source: "Linux power supply".into(),
                battery_note: None,
            });
            // `present` is battery presence, and `online` is supply state. Neither
            // is a reliable radio-connection indicator.
            let present = read_attribute(&path, "present").as_deref() != Some("0");
            device.battery = if present {
                read_attribute(&path, "capacity").and_then(|value| battery_percentage(&value))
            } else {
                None
            };
            device.charging = if present {
                read_attribute(&path, "status").and_then(|value| charging_status(&value))
            } else {
                None
            };
            device.source = "Linux power supply".into();
            device.battery_note = device.battery.is_none().then(|| {
                "Kernel nhận diện pin của thiết bị này nhưng chưa cung cấp phần trăm pin hiện tại hợp lệ."
                    .into()
            });
            merge_device(devices, device);
        }
        status("linux-power", "Linux peripheral batteries", true, format!("Đã đọc {count} nguồn pin thiết bị ngoại vi. Không bao gồm pin máy tính; giá trị chưa được hỗ trợ hiển thị chưa xác định."))
    }

    fn merge_device(devices: &mut BTreeMap<String, Device>, incoming: Device) {
        let Some(existing) = devices.get_mut(&incoming.id) else {
            devices.insert(incoming.id.clone(), incoming);
            return;
        };
        if incoming.source == "BlueZ" {
            existing.name = incoming.name;
            existing.connection = incoming.connection;
            existing.connected = incoming.connected;
            // BlueZ may cache Battery1 after disconnect: do not expose stale charge
            // as current, including charge inherited from a lingering sysfs object.
            if incoming.connected == Some(false) {
                existing.battery = None;
                existing.charging = None;
                existing.battery_note = incoming.battery_note.clone();
            }
        }
        if existing.kind == "unknown" && incoming.kind != "unknown" {
            existing.kind = incoming.kind;
        }
        if incoming.battery.is_some() {
            existing.battery = incoming.battery;
            existing.battery_note = incoming.battery_note;
        }
        if incoming.charging.is_some() {
            existing.charging = incoming.charging;
        }
        if !existing
            .source
            .split(" + ")
            .any(|source| source == incoming.source)
        {
            existing.source = format!("{} + {}", existing.source, incoming.source);
        }
    }

    fn read_bluez() -> Result<String, String> {
        let executable = ["/usr/bin/busctl", "/bin/busctl"].into_iter().find(|path| Path::new(path).is_file()).ok_or("Không tìm thấy busctl. Cần busctl của systemd để đọc Bluetooth; khám phá Linux HID vẫn hoạt động.")?;
        // No shell, no service auto-start, no pairing/scanning/writes. busctl has a
        // D-Bus timeout; the parent also enforces a wall-clock deadline and output cap.
        let mut child = Command::new(executable)
            .args([
                "--system",
                "--json=short",
                "--timeout=2",
                "--auto-start=no",
                "call",
                "org.bluez",
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Không chạy được truy vấn BlueZ chỉ đọc: {error}"))?;
        let stdout = child.stdout.take().ok_or("Không đọc được phản hồi BlueZ")?;
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(BLUEZ_OUTPUT_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        let result = loop {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    break if exit.success() {
                        Ok(())
                    } else {
                        Err("BlueZ không khả dụng hoặc bị từ chối truy cập. Kiểm tra dịch vụ Bluetooth và hỗ trợ JSON của busctl.".to_owned())
                    }
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(15)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(
                        "Truy vấn BlueZ quá thời gian chờ; các provider khác vẫn hoạt động.".to_owned(),
                    );
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("Truy vấn BlueZ thất bại: {error}"));
                }
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| "Không xử lý được phản hồi BlueZ")?
            .map_err(|error| format!("Không đọc được phản hồi BlueZ: {error}"))?;
        result?;
        if bytes.len() as u64 > BLUEZ_OUTPUT_LIMIT {
            return Err("Phản hồi BlueZ vượt giới hạn dung lượng.".into());
        }
        String::from_utf8(bytes).map_err(|_| "BlueZ trả về UTF-8 không hợp lệ".into())
    }

    fn property<'a>(properties: &'a Value, name: &str) -> Option<&'a Value> {
        properties.get(name)?.get("data")
    }

    fn parse_bluez(json: &str) -> Result<Vec<Device>, String> {
        let response: Value =
            serde_json::from_str(json).map_err(|_| "BlueZ trả về JSON không hợp lệ")?;
        if response.get("type").and_then(Value::as_str) != Some("a{oa{sa{sv}}}") {
            return Err("Kiểu phản hồi BlueZ chưa được hỗ trợ.".into());
        }
        let objects = response
            .get("data")
            .and_then(|data| data.get(0))
            .and_then(Value::as_object)
            .ok_or("Định dạng JSON của BlueZ chưa được hỗ trợ.")?;
        let mut devices = BTreeMap::<String, Device>::new();
        for interfaces in objects.values() {
            let Some(properties) = interfaces.get("org.bluez.Device1") else {
                continue;
            };
            let connected = property(properties, "Connected").and_then(Value::as_bool);
            let paired = property(properties, "Paired").and_then(Value::as_bool) == Some(true);
            if connected != Some(true) && !paired {
                continue;
            }
            let Some(address) = property(properties, "Address")
                .and_then(Value::as_str)
                .and_then(bluetooth_address)
            else {
                continue;
            };
            let name = property(properties, "Alias")
                .or_else(|| property(properties, "Name"))
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or("Bluetooth device")
                .to_owned();
            let icon = property(properties, "Icon")
                .and_then(Value::as_str)
                .unwrap_or("");
            let kind = match icon {
                "input-mouse" => "mouse".into(),
                "input-keyboard" => "keyboard".into(),
                "audio-headset" | "audio-headphones" => "headset".into(),
                "input-gaming" => "controller".into(),
                _ => classify(&name),
            };
            let battery = if connected == Some(true) {
                interfaces
                    .get("org.bluez.Battery1")
                    .and_then(|properties| property(properties, "Percentage"))
                    .and_then(Value::as_u64)
                    .filter(|percentage| *percentage <= 100)
                    .map(|percentage| percentage as u8)
            } else {
                None
            };
            let device = Device {
                id: format!("bluetooth:{address}"),
                name,
                kind,
                connection: "bluetooth".into(),
                battery,
                charging: None,
                connected,
                source: "BlueZ".into(),
                battery_note: battery.is_none().then(|| {
                    if connected != Some(true) {
                        "Thiết bị chưa kết nối; không hiển thị giá trị pin lưu cũ như dữ liệu hiện tại."
                            .into()
                    } else {
                        "BlueZ chưa cung cấp phần trăm pin Battery1 hợp lệ cho thiết bị này.".into()
                    }
                }),
            };
            // One peripheral can be remembered by several Bluetooth adapters.
            // An inactive adapter must not hide another adapter's live connection.
            if let Some(existing) = devices.get(&device.id) {
                let keep_live_reading = existing.connected == Some(true)
                    && (device.connected != Some(true) || device.battery.is_none());
                if keep_live_reading {
                    continue;
                }
            }
            devices.insert(device.id.clone(), device);
        }
        Ok(devices.into_values().collect())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        fn hid_fixture(interface: u8, instance: u8) -> Device {
            hid_device(&format!("HID_ID=0003:0000046D:0000C084\nHID_NAME=Gaming Mouse\nHID_PHYS=usb-1-6/input{interface}\nHID_UNIQ=\n"), Path::new(&format!("/sys/devices/usb1/1-6/1-6:1.{interface}/0003:046D:C084.000{instance}"))).unwrap()
        }

        #[test]
        fn composite_usb_interfaces_share_a_stable_id_without_claiming_radio_connectivity() {
            let mut devices = BTreeMap::new();
            merge_device(&mut devices, hid_fixture(0, 1));
            merge_device(&mut devices, hid_fixture(1, 9));
            assert_eq!(devices.len(), 1);
            let device = devices.values().next().unwrap();
            assert_eq!(device.connection, "usb");
            assert_eq!(device.connected, None);
            assert_eq!(device.battery, None);
            assert_eq!(device.kind, "mouse");
        }

        #[test]
        fn usb_port_fallback_deduplicates_interfaces_without_a_serial_or_phys_field() {
            let event = "HID_ID=0003:00000001:00000002\nHID_NAME=Receiver\n";
            let first = hid_device(
                event,
                Path::new("/sys/devices/usb1/1-6/1-6:1.0/0003:0001:0002.0001"),
            )
            .unwrap();
            let second = hid_device(
                event,
                Path::new("/sys/devices/usb1/1-6/1-6:1.1/0003:0001:0002.0009"),
            )
            .unwrap();
            assert_eq!(first.id, second.id);
        }

        #[test]
        fn system_batteries_and_non_battery_supplies_are_excluded() {
            assert!(!peripheral_supply(Some("Battery"), Some("System"), true));
            assert!(!peripheral_supply(Some("Battery"), None, false));
            assert!(!peripheral_supply(Some("USB"), Some("Device"), true));
            assert!(peripheral_supply(Some("Battery"), Some("Device"), false));
            assert!(peripheral_supply(Some("Battery"), None, true));
        }

        #[test]
        fn sysfs_fixture_attaches_peripheral_battery_to_one_device_and_excludes_laptop() {
            use std::{os::unix::fs::symlink, time::SystemTime};
            struct Fixture(PathBuf);
            impl Drop for Fixture {
                fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
            }
            let unique = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
            let fixture = Fixture(std::env::temp_dir().join(format!("device-battery-provider-{}-{unique}", std::process::id())));
            let hid_root = fixture.0.join("hid");
            let power_root = fixture.0.join("power_supply");
            fs::create_dir_all(&hid_root).unwrap();
            fs::create_dir_all(&power_root).unwrap();
            let mut peripheral = PathBuf::new();
            for interface in [0, 1] {
                let physical = fixture.0.join(format!("devices/usb1/1-6/1-6:1.{interface}/0003:0001:0002.000{interface}"));
                fs::create_dir_all(&physical).unwrap();
                fs::write(physical.join("uevent"), format!("HID_ID=0003:00000001:00000002\nHID_NAME=Gaming Mouse\nHID_PHYS=usb-1-6/input{interface}\n")).unwrap();
                symlink(&physical, hid_root.join(format!("hid{interface}"))).unwrap();
                if interface == 1 {
                    peripheral = physical.join("power_supply/hid-battery");
                    fs::create_dir_all(&peripheral).unwrap();
                    symlink(&peripheral, power_root.join("hid-battery")).unwrap();
                }
            }
            // Missing `scope` is allowed only because this battery belongs to HID.
            for (key, value) in [("type", "Battery"), ("capacity", "64"), ("status", "Charging"), ("present", "1")] {
                fs::write(peripheral.join(key), value).unwrap();
            }
            let laptop = power_root.join("BAT0");
            fs::create_dir_all(&laptop).unwrap();
            for (key, value) in [("type", "Battery"), ("capacity", "99"), ("scope", "System")] {
                fs::write(laptop.join(key), value).unwrap();
            }
            let scan_fixture = || {
                let (hid, status) = read_hid(&hid_root);
                assert!(status.available);
                let mut devices = BTreeMap::new();
                for item in &hid { merge_device(&mut devices, item.device.clone()); }
                assert!(read_power_supplies(&power_root, &hid, &mut devices).available);
                devices
            };
            let devices = scan_fixture();
            assert_eq!(devices.len(), 1);
            let device = devices.values().next().unwrap();
            assert_eq!(device.battery, Some(64));
            assert_eq!(device.charging, Some(true));
            assert_eq!(device.connected, None);
            assert_eq!(device.battery_note, None);
            fs::write(peripheral.join("present"), "0").unwrap();
            let absent = scan_fixture();
            assert_eq!(absent.values().next().unwrap().battery, None);
            assert_eq!(absent.values().next().unwrap().charging, None);
        }

        #[test]
        fn invalid_percentages_and_unknown_charging_are_not_invented() {
            assert_eq!(battery_percentage("0"), Some(0));
            assert_eq!(battery_percentage("100\n"), Some(100));
            for value in ["101", "255", "-1", "64%", "", "invalid"] {
                assert_eq!(battery_percentage(value), None);
            }
            assert_eq!(charging_status("Charging"), Some(true));
            assert_eq!(charging_status("Full"), Some(false));
            assert_eq!(charging_status("Unknown"), None);
        }

        fn bluez_fixture(connected: bool, percentage: u16) -> Value {
            json!({"type":"a{oa{sa{sv}}}","data":[{
                "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF":{
                    "org.bluez.Device1":{
                        "Address":{"type":"s","data":"AA:BB:CC:DD:EE:FF"},
                        "Alias":{"type":"s","data":"My headphones"},
                        "Icon":{"type":"s","data":"audio-headset"},
                        "Paired":{"type":"b","data":true},
                        "Connected":{"type":"b","data":connected}
                    },
                    "org.bluez.Battery1":{"Percentage":{"type":"y","data":percentage}}
                }
            }]})
        }

        #[test]
        fn bluez_reads_real_properties_and_drops_cached_disconnected_battery() {
            let connected = parse_bluez(&bluez_fixture(true, 64).to_string()).unwrap();
            assert_eq!(connected[0].id, "bluetooth:aa:bb:cc:dd:ee:ff");
            assert_eq!(connected[0].battery, Some(64));
            assert_eq!(connected[0].charging, None);
            assert_eq!(connected[0].kind, "headset");
            let disconnected = parse_bluez(&bluez_fixture(false, 64).to_string()).unwrap();
            assert_eq!(disconnected[0].battery, None);
            assert_eq!(disconnected[0].connected, Some(false));
            assert_eq!(
                parse_bluez(&bluez_fixture(true, 180).to_string()).unwrap()[0].battery,
                None
            );
            assert!(parse_bluez("{}").is_err());
        }

        #[test]
        fn bluez_disconnect_clears_sysfs_charge_and_merges_bluetooth_identity() {
            let mut devices = BTreeMap::new();
            let mut hid = hid_device(
                "HID_ID=0005:00000001:00000002\nHID_NAME=Headset\nHID_UNIQ=AA:BB:CC:DD:EE:FF\n",
                Path::new("/sys/devices/bluetooth/example"),
            )
            .unwrap();
            hid.battery = Some(90);
            hid.charging = Some(true);
            merge_device(&mut devices, hid);
            for device in parse_bluez(&bluez_fixture(false, 64).to_string()).unwrap() {
                merge_device(&mut devices, device);
            }
            assert_eq!(devices.len(), 1);
            let device = devices.values().next().unwrap();
            assert_eq!(device.name, "My headphones");
            assert_eq!(device.battery, None);
            assert_eq!(device.charging, None);
            assert_eq!(device.connected, Some(false));
        }

        #[test]
        fn inactive_bluetooth_adapter_does_not_override_the_same_devices_live_reading() {
            let mut fixture = bluez_fixture(true, 64);
            let inactive = bluez_fixture(false, 20);
            fixture["data"][0]["/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF"] =
                inactive["data"][0]["/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"].clone();
            let devices = parse_bluez(&fixture.to_string()).unwrap();
            assert_eq!(devices.len(), 1);
            assert_eq!(devices[0].connected, Some(true));
            assert_eq!(devices[0].battery, Some(64));
        }

        #[test]
        fn nearby_unpaired_advertisements_are_not_added_to_the_monitor() {
            let mut fixture = bluez_fixture(false, 40);
            fixture["data"][0]["/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"]["org.bluez.Device1"]
                ["Paired"]["data"] = json!(false);
            assert!(parse_bluez(&fixture.to_string()).unwrap().is_empty());
        }
    }
}
