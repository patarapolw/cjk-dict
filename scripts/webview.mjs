#!/usr/bin/env node
// Drive the running app's webview over the Chrome DevTools Protocol (Windows, WebView2).
// Start the app with remote debugging first; see "Checking the app" in AGENTS.md.
//
//   node scripts/webview.mjs eval "document.title"
//   node scripts/webview.mjs open "#/dictionary?q=逆数"  # navigate within the app, then wait
//   node scripts/webview.mjs screenshot out.png [selector]
//
// PORT (default 9222) selects the DevTools port.

import { writeFileSync } from "node:fs";

const port = process.env.PORT ?? "9222";
const [cmd, ...args] = process.argv.slice(2);

const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) =>
  r.json(),
);
const page = targets.find((t) => t.type === "page");
if (!page) throw new Error(`no page on port ${port}: is the app running?`);

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
let nextId = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  pending.get(msg.id)?.(msg);
  pending.delete(msg.id);
};
function send(method, params = {}) {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    pending.set(id, (msg) =>
      msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result),
    );
    ws.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const { result, exceptionDetails } = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails) {
    throw new Error(exceptionDetails.exception?.description ?? "eval failed");
  }
  return result.value;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  if (cmd === "eval") {
    console.log(JSON.stringify(await evaluate(args.join(" ")), null, 2));
  } else if (cmd === "open") {
    // relative to the current URL; the app uses hash routes, and changing only the hash
    // doesn't reload the page, so the app keeps its state and downloads
    const url = JSON.stringify(args[0] ?? "#/");
    await evaluate(`location.assign(new URL(${url}, location.href))`);
    await sleep(Number(process.env.WAIT ?? 1500)); // search debounce, results, images
    console.log(await evaluate("location.href"));
  } else if (cmd === "screenshot") {
    const [out = "webview.png", selector] = args;
    const params = { format: "png" };
    if (selector) {
      const rect = await evaluate(
        `(() => { const r = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect();
          return r && { x: r.x, y: r.y, width: r.width, height: r.height, scale: 1 }; })()`,
      );
      if (!rect) throw new Error(`no element matches ${selector}`);
      params.clip = rect;
    }
    const { data } = await send("Page.captureScreenshot", params);
    writeFileSync(out, Buffer.from(data, "base64"));
    console.log(out);
  } else {
    console.error(
      "usage: webview.mjs eval <js> | open <path> | screenshot <file> [selector]",
    );
    process.exitCode = 2;
  }
} finally {
  ws.close();
}
