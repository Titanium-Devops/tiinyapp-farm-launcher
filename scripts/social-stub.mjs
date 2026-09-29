#!/usr/bin/env node
// A local farm conversation for launcher screenshots. It deliberately accepts
// only the fixed fake token below and never contacts the live farm.

import http from "node:http";

const port = Number(process.env.FARM_SOCIAL_STUB_PORT || 38465);
const token = `farm_${"a".repeat(40)}`;
let mine = false;
let comments = [];

const view = () => ({
  seeds: mine ? 6 : 5,
  thumbs: mine ? 6 : 5,
  mine,
  comments,
});

const send = (response, status, body) => {
  const text = JSON.stringify(body);
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(text),
    "Cache-Control": "no-store",
  });
  response.end(text);
};

const authorized = (request) => request.headers.authorization === `Bearer ${token}`;

http.createServer((request, response) => {
  const url = new URL(request.url, `http://127.0.0.1:${port}`);
  if (!authorized(request)) {
    send(response, 401, { error: "Sign in first." });
    return;
  }
  if (request.method === "GET" && url.pathname === "/api/seeds/mine") {
    send(response, 200, { seeds: [] });
    return;
  }
  if (request.method === "GET" && url.pathname === "/api/seeds/story-lantern/social") {
    send(response, 200, view());
    return;
  }
  if (request.method === "POST" && url.pathname === "/api/seeds/story-lantern/seed") {
    mine = !mine;
    send(response, 200, view());
    return;
  }
  if (request.method === "POST" && url.pathname === "/api/seeds/story-lantern/comments") {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      let body;
      try { body = JSON.parse(Buffer.concat(chunks).toString("utf8")); }
      catch { send(response, 400, { error: "Send a valid JSON object." }); return; }
      const text = typeof body.text === "string" ? body.text.trim() : "";
      if (!text || [...text].length > 1000) {
        send(response, 400, { error: "Write a comment of 1 to 1000 characters." });
        return;
      }
      comments = [...comments, {
        id: "0123456789abcdef0123456789abcdef",
        author: { handle: "launcher-qa", name: "Launcher QA", avatar: null },
        text,
        at: "2026-09-29T15:30:00.000Z",
        canDelete: true,
      }];
      send(response, 201, view());
    });
    return;
  }
  send(response, 404, { error: "That app is not in this stub." });
}).listen(port, "127.0.0.1", () => {
  process.stdout.write(`farm social stub listening on http://127.0.0.1:${port}\n`);
});
