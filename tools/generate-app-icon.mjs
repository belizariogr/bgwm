import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";
import pngToIco from "png-to-ico";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const svgPath = path.join(root, "assets", "icon", "bgwm.svg");
const icoPath = path.join(root, "assets", "icon", "bgwm.ico");
const sizes = [16, 24, 32, 48, 64, 128, 256];

const svg = fs.readFileSync(svgPath);
const pngBuffers = sizes.map((size) =>
  new Resvg(svg, {
    fitTo: { mode: "width", value: size },
    background: "transparent",
  })
    .render()
    .asPng(),
);

const ico = await pngToIco(pngBuffers);
fs.writeFileSync(icoPath, ico);
console.log(`Wrote ${icoPath} (${ico.length} bytes, sizes: ${sizes.join(", ")})`);
