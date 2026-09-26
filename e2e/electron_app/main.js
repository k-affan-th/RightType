// Minimal Electron host for RightType E2E: loads ../target.html (a textarea
// plus an <input type="password">) so the password guard can be exercised in
// a real Electron app. Started by e2e/release_gaps.py via `npx electron`.
const { app, BrowserWindow } = require("electron");
const path = require("path");

app.whenReady().then(() => {
  const win = new BrowserWindow({ width: 900, height: 600, title: "rt-e2e" });
  win.loadFile(path.join(__dirname, "..", "target.html"));
});
app.on("window-all-closed", () => app.quit());
