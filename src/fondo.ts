import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const video = document.getElementById("video") as HTMLVideoElement;
const image = document.getElementById("image") as HTMLImageElement;

function show(path: string) {
  const src = convertFileSrc(path);
  const gif = /\.gif$/i.test(path);
  video.hidden = gif;
  image.hidden = !gif;
  if (gif) {
    video.removeAttribute("src");
    video.load();
    image.src = src;
  } else {
    image.removeAttribute("src");
    video.src = src;
    void video.play();
  }
}

function pause(on: boolean) {
  image.style.visibility = on ? "hidden" : "visible";
  if (video.hidden) return;
  if (on) video.pause();
  else void video.play();
}

void listen<string>("fondo", (e) => show(e.payload));
void listen<boolean>("fondo-pausa", (e) => pause(e.payload));

const current = await invoke<[string, boolean] | null>("live_wallpaper_get");
if (current) {
  show(current[0]);
  pause(current[1]);
}
