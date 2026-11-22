import init, { WebProbe, WebStartup } from "./pkg/raf_agb.js";

const canvas = document.querySelector("canvas");
const status = document.querySelector("output");
const scale = () => Math.min(window.devicePixelRatio || 1, 2);
const pixelSize = () => {
  const density = scale();
  return [Math.max(1, Math.round(canvas.clientWidth * density)), Math.max(1, Math.round(canvas.clientHeight * density))];
};

async function showProbe() {
  const fixture = new URL(location.href).searchParams.get("fixture");
  const response = fixture ? await fetch(new URL(fixture, location.href)) : null;
  if (response && !response.ok) throw new Error(String(response.status));
  const probe = await WebProbe.open(canvas, ...pixelSize(), await response?.text());
  let orbit = 0.82;
  let pending = false;
  const redraw = () => {
    if (pending || document.hidden) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      if (document.hidden) return;
      try {
        probe.resize(...pixelSize());
        probe.frame(orbit);
      } catch (error) { status.value = String(error); }
    });
  };
  probe.frame(orbit);
  await probe.wait_for_gpu();
  status.value = `WebGPU · ${probe.adapter()}`;
  document.documentElement.dataset.graphics = "ready";
  const resize = new ResizeObserver(redraw);
  resize.observe(canvas);
  document.addEventListener("visibilitychange", redraw);
  canvas.addEventListener("pointerdown", (event) => canvas.setPointerCapture(event.pointerId));
  canvas.addEventListener("pointermove", (event) => {
    if (canvas.hasPointerCapture(event.pointerId)) { orbit += event.movementX * 0.007; redraw(); }
  });
}

async function showStartup() {
  let startup = await WebStartup.open(canvas);
  startup.redraw(...pixelSize(), scale());
  await startup.wait_for_gpu();
  status.value = `WebGPU · ${startup.adapter()}`;
  document.documentElement.dataset.graphics = "ready";
  let pending = false;
  let recovering = false;
  const watchLoss = (device) => device.loss_signal().then(() => {
    if (startup === device) redraw();
  });
  const redraw = () => {
    if (document.hidden || pending || recovering) return;
    pending = true;
    requestAnimationFrame(async () => {
      pending = false;
      if (document.hidden) return;
      try {
        if (startup?.device_lost()) {
          const lost = startup;
          startup = null;
          lost.free();
        }
        if (!startup) {
          recovering = true;
          startup = await WebStartup.open(canvas);
          watchLoss(startup);
        }
        if (!document.hidden) {
          startup.redraw(...pixelSize(), scale());
          status.value = `WebGPU · ${startup.adapter()}`;
          document.documentElement.dataset.graphics = "ready";
        }
      } catch (error) {
        document.documentElement.dataset.graphics = "failed";
        status.value = String(error);
      } finally {
        recovering = false;
      }
    });
  };
  watchLoss(startup);
  new ResizeObserver(redraw).observe(canvas);
  window.addEventListener("resize", redraw);
  document.addEventListener("visibilitychange", redraw);
}

try {
  await init();
  if (new URL(location.href).searchParams.has("startup")) await showStartup();
  else await showProbe();
} catch (error) {
  document.documentElement.dataset.graphics = "failed";
  status.value = String(error);
}
