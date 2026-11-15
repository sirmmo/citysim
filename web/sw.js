// citysim's service worker: keeps one whole build of the game in the
// browser, so the installed app starts without a network and never pairs
// game code of one build with data files of another.
//
// `cargo run -p xtask -- web-dist` stamps BUILD and the two big downloads
// with the build's own addresses (the ones the page asks for, so that the
// browser's HTTP cache answers and nothing is downloaded twice).
var BUILD = "dev";
var WASM = "opencity.wasm";
var PACK = "assets.pack";
var CACHE = "citysim-" + BUILD;
// Small files are fetched afresh; the two big ones carry their build in
// the address already.
var FRESH = ["./", "citysim.js", "mq_js_bundle.js", "manifest.webmanifest",
  "icon-192.png", "icon-512.png", "icon-maskable-512.png"];

self.addEventListener("install", function (e) {
  e.waitUntil(caches.open(CACHE).then(function (cache) {
    var wanted = FRESH.map(function (f) { return new Request(f, { cache: "reload" }); });
    return cache.addAll(wanted.concat([WASM, PACK]));
  }));
});

// A new build waits until the running game is closed, or until the page
// says the player asked for it.
self.addEventListener("message", function (e) {
  if (e.data === "update") { self.skipWaiting(); }
});

self.addEventListener("activate", function (e) {
  e.waitUntil(caches.keys().then(function (keys) {
    return Promise.all(keys.filter(function (k) {
      return k.indexOf("citysim-") === 0 && k !== CACHE;
    }).map(function (k) { return caches.delete(k); }));
  }).then(function () { return self.clients.claim(); }));
});

// The game's own files come from the kept build, whatever `?v=` or
// `?demo` the address carries. Everything else (region servers, files
// that are not part of the build) goes to the network as before.
self.addEventListener("fetch", function (e) {
  var request = e.request;
  if (request.method !== "GET" || new URL(request.url).origin !== self.location.origin) { return; }
  e.respondWith(caches.open(CACHE).then(function (cache) {
    return cache.match(request, { ignoreSearch: true });
  }).then(function (kept) { return kept || fetch(request); }));
});
