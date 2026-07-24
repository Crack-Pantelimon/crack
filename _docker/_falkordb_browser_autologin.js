#!/usr/bin/env node
/**
 * Reverse-proxy in front of falkordb-browser that auto-establishes a NextAuth
 * session against the sibling FalkorDB (no interactive login form).
 *
 * Listens on PORT (default 3000), proxies to BACKEND (default 127.0.0.1:3001).
 * Credentials come from FALKORDB_HOST / FALKORDB_PORT (docker DNS names).
 */
"use strict";

const http = require("node:http");

const LISTEN_PORT = Number(process.env.PORT || 3000);
const BACKEND_HOST = process.env.AUTOLOGIN_BACKEND_HOST || "127.0.0.1";
const BACKEND_PORT = Number(process.env.AUTOLOGIN_BACKEND_PORT || 3001);
const FALKOR_HOST = process.env.FALKORDB_HOST || "falkordb";
const FALKOR_PORT = process.env.FALKORDB_PORT || "6379";
const FALKOR_USER = process.env.FALKORDB_USERNAME || "";
const FALKOR_PASS = process.env.FALKORDB_PASSWORD || "";
const PUBLIC_ORIGIN = process.env.AUTH_URL || "http://localhost:3000";

const SESSION_COOKIE = "authjs.session-token";

function hasSession(cookieHeader) {
  if (!cookieHeader) return false;
  return cookieHeader.split(";").some((c) => {
    const name = c.trim().split("=")[0];
    return name === SESSION_COOKIE || name.endsWith(SESSION_COOKIE) || name.includes("session-token");
  });
}

function parseSetCookies(headers) {
  const raw = headers["set-cookie"];
  if (!raw) return [];
  return Array.isArray(raw) ? raw : [raw];
}

function cookieHeaderFromSetCookies(setCookies) {
  return setCookies
    .map((c) => c.split(";")[0])
    .filter(Boolean)
    .join("; ");
}

function backendRequest(method, path, { headers = {}, body = null } = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        host: BACKEND_HOST,
        port: BACKEND_PORT,
        path,
        method,
        headers,
      },
      (res) => {
        const chunks = [];
        res.on("data", (d) => chunks.push(d));
        res.on("end", () => {
          resolve({
            status: res.statusCode || 500,
            headers: res.headers,
            body: Buffer.concat(chunks),
          });
        });
      }
    );
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}

async function createSession() {
  const csrfRes = await backendRequest("GET", "/api/auth/csrf");
  const csrfJson = JSON.parse(csrfRes.body.toString("utf8"));
  const csrfToken = csrfJson.csrfToken;
  if (!csrfToken) throw new Error("missing csrfToken");

  const cookie = cookieHeaderFromSetCookies(parseSetCookies(csrfRes.headers));
  const form = new URLSearchParams();
  form.set("csrfToken", csrfToken);
  form.set("host", FALKOR_HOST);
  form.set("port", String(FALKOR_PORT));
  form.set("username", FALKOR_USER);
  form.set("password", FALKOR_PASS);
  form.set("tls", "false");
  form.set("callbackUrl", `${PUBLIC_ORIGIN.replace(/\/$/, "")}/graph`);
  form.set("json", "true");
  const body = Buffer.from(form.toString(), "utf8");

  const loginRes = await backendRequest("POST", "/api/auth/callback/credentials", {
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      "content-length": String(body.length),
      cookie,
      origin: PUBLIC_ORIGIN,
      referer: `${PUBLIC_ORIGIN}/login`,
    },
    body,
  });

  const setCookies = [
    ...parseSetCookies(csrfRes.headers),
    ...parseSetCookies(loginRes.headers),
  ];
  // Auth.js may set cookie name with __Secure- / __Host- prefixes.
  const sessionCookie = setCookies.find((c) => {
    const name = c.split("=")[0];
    return (
      name === SESSION_COOKIE ||
      name.endsWith(SESSION_COOKIE) ||
      name.includes("session-token")
    );
  });
  if (!sessionCookie) {
    throw new Error(
      `auto-login failed: status=${loginRes.status} ` +
        `location=${loginRes.headers.location || ""} ` +
        `set-cookie=${setCookies.map((c) => c.split("=")[0]).join(",")} ` +
        `body=${loginRes.body.toString("utf8").slice(0, 200)}`
    );
  }
  return setCookies;
}

function shouldBypassAutologin(urlPath) {
  return (
    urlPath.startsWith("/api/auth/") ||
    urlPath.startsWith("/_next/") ||
    urlPath.startsWith("/icons/") ||
    urlPath.startsWith("/favicon") ||
    urlPath === "/docs" ||
    urlPath.startsWith("/docs/")
  );
}

function proxy(clientReq, clientRes, extraSetCookies = []) {
  const headers = { ...clientReq.headers };
  delete headers["accept-encoding"];
  headers.host = `${BACKEND_HOST}:${BACKEND_PORT}`;

  const preq = http.request(
    {
      host: BACKEND_HOST,
      port: BACKEND_PORT,
      path: clientReq.url,
      method: clientReq.method,
      headers,
    },
    (pres) => {
      const outHeaders = { ...pres.headers };
      if (extraSetCookies.length) {
        const existing = parseSetCookies(outHeaders);
        outHeaders["set-cookie"] = [...existing, ...extraSetCookies];
      }
      clientRes.writeHead(pres.statusCode || 500, outHeaders);
      pres.pipe(clientRes);
    }
  );
  preq.on("error", (err) => {
    if (!clientRes.headersSent) {
      clientRes.writeHead(502, { "content-type": "text/plain" });
    }
    clientRes.end(`autologin proxy error: ${err.message}`);
  });
  clientReq.pipe(preq);
}

const server = http.createServer(async (req, res) => {
  const urlPath = (req.url || "/").split("?")[0];
  try {
    if (hasSession(req.headers.cookie) || shouldBypassAutologin(urlPath)) {
      proxy(req, res);
      return;
    }

    const setCookies = await createSession();
    if (urlPath === "/login" || urlPath === "/") {
      res.writeHead(302, {
        location: "/graph",
        "set-cookie": setCookies,
      });
      res.end();
      return;
    }
    const jar = cookieHeaderFromSetCookies(setCookies);
    req.headers.cookie = req.headers.cookie
      ? `${req.headers.cookie}; ${jar}`
      : jar;
    proxy(req, res, setCookies);
  } catch (err) {
    console.error("[autologin]", err);
    if (!res.headersSent) {
      res.writeHead(500, { "content-type": "text/plain" });
    }
    res.end(`FalkorDB browser auto-login failed: ${err.message}`);
  }
});

server.listen(LISTEN_PORT, "0.0.0.0", () => {
  console.log(
    `[autologin] listening on :${LISTEN_PORT} -> ${BACKEND_HOST}:${BACKEND_PORT} ` +
      `(falkor ${FALKOR_HOST}:${FALKOR_PORT})`
  );
});
