import type { Device, MonitorSettings, MonitorSnapshot } from "./types";

export const defaultSettings: MonitorSettings = {
  refreshSeconds: 5,
  lowBatteryThreshold: 20,
  notificationsEnabled: false,
};

export function previewSnapshot(): MonitorSnapshot {
  return {
    devices: [],
    providers: [{
      id: "desktop",
      name: "Ứng dụng desktop",
      available: false,
      message: "Mở Device Battery bằng ứng dụng Tauri để đọc thiết bị trên máy.",
    }],
    scannedAt: 0,
    history: {},
    settings: { ...defaultSettings },
    notificationError: null,
    monitoringSince: Date.now(),
    trayAvailable: false,
  };
}

// Explicit demonstration fixtures. Never merged into native readings.
export function demoSnapshot(settings = defaultSettings): MonitorSnapshot {
  const now = Date.now();
  const devices: Device[] = [
    { id: "demo-mouse", name: "Gaming Mouse", kind: "mouse", connection: "wireless", battery: 82, charging: false, connected: true, source: "Demo provider", batteryNote: null },
    { id: "demo-keyboard", name: "F17 Pro", kind: "keyboard", connection: "wireless", battery: 64, charging: true, connected: true, source: "Demo provider", batteryNote: null },
    { id: "demo-headset", name: "Studio Headset", kind: "headset", connection: "bluetooth", battery: 18, charging: false, connected: true, source: "Demo provider", batteryNote: null },
    { id: "demo-receiver", name: "USB Receiver", kind: "unknown", connection: "usb", battery: null, charging: null, connected: null, source: "Demo provider", batteryNote: "Ví dụ receiver được nhận diện nhưng không cung cấp thông tin pin hoặc kết nối của thiết bị phía sau." },
  ];
  return {
    devices,
    providers: [{ id: "demo", name: "Demo provider", available: true, message: "Dữ liệu minh họa; không phản ánh thiết bị trên máy và không gửi thông báo hệ thống." }],
    scannedAt: now,
    history: Object.fromEntries(devices.filter((device) => device.battery !== null).map((device) => [
      device.id,
      Array.from({ length: 13 }, (_, index) => ({
        timestamp: now - (12 - index) * 30 * 60 * 1000,
        battery: Math.min(100, Math.max(0, device.battery! + (12 - index) * (device.charging ? -2 : 1))),
      })),
    ])),
    settings: { ...settings },
    notificationError: null,
    monitoringSince: now - 6 * 60 * 60 * 1000,
    trayAvailable: false,
  };
}
