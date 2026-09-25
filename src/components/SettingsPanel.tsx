import React, { useState, useEffect } from "react";
import type { MonitorSettings } from "../types";

interface Props {
  settings: MonitorSettings;
  onUpdate: (settings: MonitorSettings) => void;
}

export function SettingsPanel({ settings, onUpdate }: Props) {
  const [localSettings, setLocalSettings] = useState<MonitorSettings>(settings);

  useEffect(() => {
    setLocalSettings(settings);
  }, [settings]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value, type, checked } = e.target;
    const newSettings = {
      ...localSettings,
      [name]: type === "checkbox" ? checked : parseInt(value, 10) || 0,
    };
    setLocalSettings(newSettings);
  };

  const handleBlur = () => {
    // Basic validation
    let validSettings = { ...localSettings };
    if (validSettings.refreshSeconds < 5) validSettings.refreshSeconds = 5;
    if (validSettings.refreshSeconds > 60) validSettings.refreshSeconds = 60;
    if (validSettings.lowBatteryThreshold < 5) validSettings.lowBatteryThreshold = 5;
    if (validSettings.lowBatteryThreshold > 50) validSettings.lowBatteryThreshold = 50;
    
    setLocalSettings(validSettings);
    onUpdate(validSettings);
  };

  return (
    <div className="card settings-card">
      <h3>Settings</h3>
      <div className="settings-form">
        <div className="form-group">
          <label htmlFor="refreshSeconds">Refresh Interval (sec)</label>
          <div className="range-container">
            <input
              type="range"
              id="refreshSeconds"
              name="refreshSeconds"
              min="5"
              max="60"
              value={localSettings.refreshSeconds}
              onChange={handleChange}
              onBlur={handleBlur}
            />
            <span className="range-val">{localSettings.refreshSeconds}s</span>
          </div>
          <small>Between 5 and 60 seconds.</small>
        </div>

        <div className="form-group">
          <label htmlFor="lowBatteryThreshold">Low Battery Threshold (%)</label>
          <div className="range-container">
            <input
              type="range"
              id="lowBatteryThreshold"
              name="lowBatteryThreshold"
              min="5"
              max="50"
              value={localSettings.lowBatteryThreshold}
              onChange={handleChange}
              onBlur={handleBlur}
            />
            <span className="range-val">{localSettings.lowBatteryThreshold}%</span>
          </div>
          <small>Alert when battery falls below this.</small>
        </div>

        <div className="form-group checkbox-group">
          <input
            type="checkbox"
            id="notificationsEnabled"
            name="notificationsEnabled"
            checked={localSettings.notificationsEnabled}
            onChange={(e) => {
              handleChange(e);
              onUpdate({ ...localSettings, notificationsEnabled: e.target.checked });
            }}
          />
          <label htmlFor="notificationsEnabled">Enable System Notifications</label>
        </div>
      </div>
    </div>
  );
}
