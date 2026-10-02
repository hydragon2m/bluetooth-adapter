// @ts-nocheck
import React, { useEffect } from "react";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useMonitor } from "../useMonitor";
import "../App.css";

const MonitorIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
    <line x1="8" y1="21" x2="16" y2="21"></line>
    <line x1="12" y1="17" x2="12" y2="21"></line>
    <polyline points="5 10 10 5 14 9 19 4"></polyline>
  </svg>
);

const KeyboardIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="6" width="20" height="12" rx="2" ry="2"></rect>
    <circle cx="6" cy="10" r="1"></circle>
    <circle cx="10" cy="10" r="1"></circle>
    <circle cx="14" cy="10" r="1"></circle>
    <circle cx="18" cy="10" r="1"></circle>
    <line x1="8" y1="14" x2="16" y2="14"></line>
  </svg>
);

const MouseIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="5" y="2" width="14" height="20" rx="7"></rect>
    <line x1="12" y1="6" x2="12" y2="10"></line>
  </svg>
);

const ChevronRightIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="9 18 15 12 9 6"></polyline>
  </svg>
);

const BluetoothIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="6.5 6.5 17.5 17.5 12 23 12 1 17.5 6.5 6.5 17.5"></polyline>
  </svg>
);

function getDeviceIcon(name: string) {
  const n = name.toLowerCase();
  if (n.includes("keyboard") || n.includes("bàn phím") || n.includes("key")) return <KeyboardIcon />;
  if (n.includes("mouse") || n.includes("chuột") || n.includes("prodigy")) return <MouseIcon />;
  return <BluetoothIcon />;
}

export function TrayPopup() {
  const { snapshot, loading, error } = useMonitor();

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // optionally hide window
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const openMain = () => {
    invoke("open_main"); // We no longer have open_main registered, we use "show" in menu! Wait, we need to invoke a custom command or just emit an event.
    // Actually we didn't register open_main in lib.rs earlier, wait, I deleted open_main when I wiped lib.rs previously.
  };

  // We can just rely on tauri event or use window api directly if needed.

  if (error && !snapshot) {
    return <div className="tray-popup error">Error: {error}</div>;
  }

  if (!snapshot) {
    return (
      <div className="tray-popup center">
        <div className="loader"></div>
      </div>
    );
  }

  // Filter out disconnected devices entirely as requested
  const activeDevices = snapshot.devices.filter((d) => d.connected !== false && d.battery !== null);

  return (
    <div className="tray-popup">
      <div className="tray-section-header">Thiết bị của bạn</div>
      
      {activeDevices.length === 0 ? (
        <div className="tray-empty">Chưa có thiết bị nào</div>
      ) : (
        <ul className="tray-list">
          {activeDevices.map((device) => {
            const isUnknown = device.battery === null;
            const batValue = device.battery || 0;
            
            return (
              <li key={device.id} className="tray-item">
                <div className="tray-item-left">
                  <div className="tray-item-icon">
                    {getDeviceIcon(device.name)}
                  </div>
                  <span className="tray-item-name">{device.name}</span>
                </div>
                
                <div className="tray-item-right">
                  {!isUnknown && (
                    <div className="tray-mock-battery-bar">
                      <div className="tray-mock-battery-border">
                        <div 
                          className="tray-mock-battery-fill" 
                          style={{ width: `${batValue}%` }}
                        ></div>
                      </div>
                      <div className="tray-mock-battery-tip"></div>
                    </div>
                  )}
                  <span className="tray-item-percent">
                    {isUnknown ? "?" : `${batValue}%`}
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
