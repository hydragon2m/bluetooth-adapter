import { useMonitor } from "./useMonitor";
import { DeviceCard } from "./components/DeviceCard";
import { SettingsPanel } from "./components/SettingsPanel";
import { TrayPopup } from "./components/TrayPopup";
import "./App.css";

function App() {
  const urlParams = new URLSearchParams(window.location.search);
  const isTray = urlParams.get("tray") === "true";

  if (isTray) {
    return <TrayPopup />;
  }

  const {
    snapshot,
    loading,
    error,
    mode,
    refresh,
    updateSettings,
    enableDemo,
    disableDemo,
  } = useMonitor();

  if (error && !snapshot) {
    return (
      <main className="container center">
        <div className="error-card">
          <h2>Error initializing monitor</h2>
          <p>{error}</p>
        </div>
      </main>
    );
  }

  if (!snapshot) {
    return (
      <main className="container center">
        <div className="loader"></div>
        <p>Loading devices...</p>
      </main>
    );
  }

  const isDemo = mode === "demo";

  return (
    <main className={`container ${isDemo ? "demo-mode" : ""}`}>
      <header className="header">
        <div className="header-title">
          <h1>Device Battery</h1>
          {isDemo && <span className="demo-badge">Demo Mode</span>}
        </div>
        <div className="header-actions">
          <button className="btn btn-secondary" onClick={refresh} disabled={loading}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>
          {mode !== "native" && (
            <button
              className="btn btn-outline"
              onClick={isDemo ? disableDemo : enableDemo}
            >
              {isDemo ? "Exit Demo" : "View Demo"}
            </button>
          )}
        </div>
      </header>

      {error && (
        <div className="alert alert-error">
          <p>{error}</p>
        </div>
      )}

      {snapshot.notificationError && (
        <div className="alert alert-warning">
          <p>Notification Error: {snapshot.notificationError}</p>
        </div>
      )}

      <div className="grid">
        <section className="devices-section">
          <h2>Connected Devices ({snapshot.devices.length})</h2>
          {snapshot.devices.length === 0 ? (
            <div className="empty-state">
              <p>No devices found.</p>
            </div>
          ) : (
            <div className="device-list">
              {snapshot.devices.map((device) => (
                <DeviceCard
                  key={device.id}
                  device={device}
                  history={snapshot.history[device.id] || []}
                />
              ))}
            </div>
          )}
        </section>

        <section className="sidebar">
          <SettingsPanel
            settings={snapshot.settings}
            onUpdate={updateSettings}
          />
          
          <div className="providers-info">
            <h3>Providers</h3>
            <ul className="provider-list">
              {snapshot.providers.map((p) => (
                <li key={p.id} className={p.available ? "provider-active" : "provider-inactive"}>
                  <strong>{p.name}</strong>
                  <p>{p.message}</p>
                </li>
              ))}
            </ul>
          </div>
        </section>
      </div>
    </main>
  );
}

export default App;
