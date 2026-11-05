import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { applyAppearance, loadPrefs } from "./lib/prefs";

// Before the first paint, so a dark theme never flashes light.
applyAppearance(loadPrefs());
mount(App, { target: document.getElementById("app")! });
