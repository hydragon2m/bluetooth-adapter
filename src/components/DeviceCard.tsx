import type { Device, BatterySample } from "../types";

interface Props {
  device: Device;
  history: BatterySample[];
}

export function DeviceCard({ device, history }: Props) {
  const isUnknown = device.battery === null;
  const isLow = !isUnknown && device.battery! <= 20; // Will use settings threshold later if passed, but 20 is default fallback for styling

  // Simple sparkline path generator
  const getSparkline = () => {
    if (history.length < 2) return null;
    const maxVal = 100;
    const minVal = 0;
    const width = 100;
    const height = 30;
    
    const minTime = history[0].timestamp;
    const maxTime = history[history.length - 1].timestamp;
    const timeRange = maxTime - minTime || 1;

    const points = history.map((h) => {
      const x = ((h.timestamp - minTime) / timeRange) * width;
      const y = height - ((h.battery - minVal) / (maxVal - minVal)) * height;
      return `${x},${y}`;
    });

    return (
      <svg viewBox={`0 0 ${width} ${height}`} className="sparkline" preserveAspectRatio="none">
        <polyline
          fill="none"
          stroke="var(--primary-color)"
          strokeWidth="2"
          points={points.join(" ")}
        />
      </svg>
    );
  };

  const getIcon = () => {
    switch (device.kind) {
      case "mouse": return "🖱️";
      case "keyboard": return "⌨️";
      case "headset": return "🎧";
      case "controller": return "🎮";
      default: return "🔌";
    }
  };

  const isDisconnected = device.connected === false;

  return (
    <div className={`card device-card ${isDisconnected ? "disconnected" : ""}`}>
      <div className="device-header">
        <div className="device-icon">{getIcon()}</div>
        <div className="device-info">
          <h3>{device.name}</h3>
          <span className="device-meta">
            {device.connection} • {device.kind}
            {isDisconnected && " • Disconnected"}
          </span>
        </div>
      </div>

      <div className="device-body">
        <div className="battery-section">
          {isUnknown ? (
            <div className="battery-unknown">
              <span className="battery-value">?</span>
              <span className="battery-label">Unknown</span>
            </div>
          ) : (
            <>
              <div className="battery-indicator">
                <svg viewBox="0 0 36 36" className="circular-chart">
                  <path
                    className="circle-bg"
                    d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
                  />
                  <path
                    className={`circle ${isLow ? "circle-low" : ""}`}
                    strokeDasharray={`${device.battery}, 100`}
                    d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
                  />
                </svg>
                <div className="battery-value-text">
                  <span className="value">{device.battery}%</span>
                </div>
              </div>
              <div className="battery-details">
                {device.charging && <span className="charging-badge">⚡ Charging</span>}
              </div>
            </>
          )}
        </div>
        
        <div className="history-section">
          {getSparkline()}
        </div>
      </div>

      {device.batteryNote && (
        <div className="device-note">
          <small>{device.batteryNote}</small>
        </div>
      )}
    </div>
  );
}
