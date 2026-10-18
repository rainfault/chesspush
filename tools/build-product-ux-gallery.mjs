import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
const root = "screens/product-ux";
const captures = [
  ...JSON.parse(await readFile(join(root, "shell-results.json"))).shots,
  ...JSON.parse(await readFile(join(root, "native-captures.json"))),
];
const records = [];
for (const directory of ["before", "after"]) {
  for (const file of (await readdir(join(root, directory)))
    .filter(
      (f) =>
        f.endsWith(".png") &&
        !["failure.png", "map-native-first.png"].includes(f),
    )
    .sort()) {
    const path = directory + "/" + file,
      bytes = await readFile(join(root, path)),
      known = captures.find((c) => c.path === path);
    if (bytes.toString("hex", 0, 8) !== "89504e470d0a1a0a")
      throw Error("Invalid PNG " + path);
    records.push({
      path,
      width: bytes.readUInt32BE(16),
      height: bytes.readUInt32BE(20),
      ...known,
      theme:
        known?.theme ??
        (file.includes("dark")
          ? "dark"
          : file.includes("light")
            ? "light"
            : "see source"),
      provenance:
        known?.provenance ??
        (directory === "before"
          ? "Prior native/atlas screenshot; see README"
          : "Focused test capture; see acceptance evidence for native/bridge/controlled boundary"),
    });
  }
}
await writeFile(join(root, "manifest.json"), JSON.stringify(records, null, 2));
const escape = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll('"', "&quot;");
await writeFile(
  join(root, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>ChessPush: интерфейс до и после</title><style>body{font:15px/1.5 system-ui;margin:24px;background:#eee;color:#333}header{position:sticky;top:0;background:#eee;padding:10px 0}input{font:inherit;padding:8px;width:70%;max-width:600px}main{display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));gap:20px}article{background:white;padding:12px}img{width:100%;height:245px;object-fit:contain;background:#ccc}p{margin:6px 0;overflow-wrap:anywhere}small{display:block}a{color:#156cb1}</style><header><h1>ChessPush · до и после</h1><p>${records.length} PNG. Происхождение и границы проверок: <a href="../../PRODUCT_UX_ACCEPTANCE.md">протокол</a>. <a href="videos/map-board-game.webm">Запись переходов</a> · <a href="videos/context-repertoire-navigation.webm">Репертуар и возврат</a></p><input aria-label="Поиск снимков" placeholder="Поиск: before, native, dark, 900, editor…"></header><main>${records.map((r) => `<article data-search="${escape(JSON.stringify(r).toLowerCase())}"><a href="${escape(r.path)}"><img loading="lazy" src="${escape(r.path)}" alt="${escape(r.path)}"></a><p>${escape(r.path)}</p><small>${r.width}×${r.height} · ${escape(r.theme)}${r.dpr ? ` · DPR ${r.dpr}` : ""}</small><small>${escape(r.provenance)}</small></article>`).join("")}</main><script>document.querySelector('input').addEventListener('input',e=>{const q=e.target.value.toLowerCase();document.querySelectorAll('article').forEach(a=>a.hidden=!a.dataset.search.includes(q))})</script>`,
);
console.log(
  JSON.stringify({
    PNG: records.length,
    native: records.filter((r) => r.provenance.startsWith("native Tauri"))
      .length,
  }),
);
