import "@fontsource-variable/roboto-flex/full.css";
import "material-symbols/rounded.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");
export default mount(App, { target });
