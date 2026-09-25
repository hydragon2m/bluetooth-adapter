import React, { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMonitor } from "../useMonitor";
import "../App.css";

export function TrayPopup() {
  const { snapshot, loading, error } = useMonitor();

  // Handle closing popup on escape or outside click if needed
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
    invoke("open_main");
  };

  const quitApp = () => {
    invoke("quit_app");
  };

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

  const activeDevices = snapshot.devices.filter((d) => d.connected !== false);

  return (
    <div className="tray-popup">
      <div className="tray-header">
        <h4>Device Battery</h4>
      </div>
      
      {activeDevices.length === 0 ? (
        <div className="tray-empty">Chưa có thiết bị nào</div>
      ) : (
        <ul className="tray-list">
          {activeDevices.map((device) => {
            const isUnknown = device.battery === null;
            const isLow = !isUnknown && device.battery! <= snapshot.settings.lowBatteryThreshold;
            
            return (
              <li key={device.id} className="tray-item">
                <div className="tray-item-info">
                  <span className="tray-item-name">{device.name}</span>
                  {device.charging && <span className="tray-item-charge">⚡</span>}
                </div>
                
                {isUnknown ? (
                  <span className="tray-item-battery unknown">?</span>
                ) : (
                  <div className="tray-battery-bar-container">
                    <span className={`tray-item-battery ${isLow ? "danger" : ""}`}>
                      {device.battery}%
                    </span>
                    <div className="tray-battery-bar">
                      <div 
                        className={`tray-battery-fill ${isLow ? "fill-danger" : "fill-success"}`} 
                        style={{ width: `${device.battery}%` }}
                      ></div>
                    </div>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <div className="tray-footer">
        <button className="tray-btn" onClick={openMain}>
          ⚙️ Mở ứng dụng
        </button>
        <button className="tray-btn danger" onClick={quitApp}>
          ❌ Thoát
        </button>
      </div>
    </div>
  );
}
