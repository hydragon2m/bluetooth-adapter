export type DeviceKind = "mouse" | "keyboard" | "headset" | "controller" | "unknown";
export type Connection = "bluetooth" | "usb" | "wireless" | "unknown";

export interface Device {
  id: string;
  name: string;
  kind: DeviceKind;
  connection: Connection;
  battery: number | null;
  charging: boolean | null;
  connected: boolean | null;
  source: string;
  batteryNote: string | null;
}

export interface ProviderStatus {
  id: string;
  name: string;
  available: boolean;
  message: string;
}

export interface BatterySample {
  timestamp: number;
  battery: number;
}

export interface MonitorSettings {
  refreshSeconds: number;
  lowBatteryThreshold: number;
  notificationsEnabled: boolean;
}

export interface MonitorSnapshot {
  devices: Device[];
  providers: ProviderStatus[];
  scannedAt: number;
  history: Record<string, BatterySample[]>;
  settings: MonitorSettings;
  notificationError: string | null;
  storageError?: string | null;
  monitoringSince: number;
  trayAvailable: boolean;
}
